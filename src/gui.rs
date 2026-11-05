//! Bookcheckbot GUI 창 (egui/eframe).
//! CLI 옵션과 같은 설정을 화면에서 입력하고, 시작을 누르면 워커 스레드가 캡처를 진행합니다.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc,
    },
    thread,
};

use anyhow::Result;
use clap::Parser;
use eframe::egui::{self, Color32, RichText};
use screenshots::Screen;

use crate::{
    capture::{self, MonitorBounds, Progress},
    cli::{parse_key, validate_args, Args, TurnMode},
};

#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
use crate::{
    capture::Rect,
    cli::{filename, TurnAction},
};

const ACCENT: Color32 = Color32::from_rgb(58, 125, 68);
const ERROR: Color32 = Color32::from_rgb(200, 72, 60);
const MUTED: Color32 = Color32::from_rgb(120, 120, 120);
const KEY_RIGHT: &str = "right";
const KEY_PAGEDOWN: &str = "pagedown";
const KEY_SPACE: &str = "space";

pub fn launch(args: &Args) -> Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Bookcheckbot - 전자책 화면 캡처")
        .with_inner_size([760.0, 900.0])
        .with_min_inner_size([600.0, 680.0])
        // 저장된 창 크기가 비정상이면 창이 화면 밖으로 나갈 수 있으므로 유효 범위로 제한합니다.
        .with_clamp_size_to_monitor_size(true);
    // 화면 가운데에 창이 뜨도록 위치를 계산합니다.
    if let Some((width, height)) = current_monitor_size() {
        let window = egui::vec2(
            760.0_f32.min(width - 40.0).max(600.0),
            900.0_f32.min(height - 80.0).max(680.0),
        );
        viewport = viewport
            .with_inner_size(window)
            .with_position([(width - window.x) * 0.5, (height - window.y) * 0.35]);
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "bookcheckbot",
        options,
        Box::new(|cc| Ok(Box::new(BookcheckbotApp::new(cc, args)))),
    )
    .map_err(|err| anyhow::anyhow!("GUI 창을 열지 못했습니다: {err}"))
}

/// 주 모니터의 픽셀 크기(없으면 None).
fn current_monitor_size() -> Option<(f32, f32)> {
    let screen = screenshots::Screen::from_point(0, 0).ok()?;
    Some((
        screen.display_info.width as f32,
        screen.display_info.height as f32,
    ))
}

/// egui 기본 폰트에는 한글이 없어 네모(□)로 보입니다.
/// Windows에 기본 설치된 한글 폰트를 읽어 기본 폰트 뒤에 붙입니다.
fn install_korean_font(ctx: &egui::Context) {
    const CANDIDATES: [&str; 5] = [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\malgunsl.ttf",
        r"C:\Windows\Fonts\gulim.ttc",
        r"C:\Windows\Fonts\batang.ttc",
        r"C:\Windows\Fonts\DotumChe.ttf",
    ];

    let Some(bytes) = CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    else {
        eprintln!("안내: 한글 폰트를 찾지 못했습니다. Windows 글꼴(맑은 고딕)을 확인하세요.");
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "korean".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("korean".to_owned());
    }
    ctx.set_fonts(fonts);
}

struct BookcheckbotApp {
    // 화면 입력값
    last_page: String,
    start_page: String,
    out_dir: String,
    screen_index: usize,
    use_region: bool,
    x: String,
    y: String,
    w: String,
    h: String,
    turn: TurnMode,
    key: String,
    click_x: String,
    click_y: String,
    start_delay: u64,
    render_wait: u64,
    turn_wait: u64,

    // 실행 상태
    running: bool,
    cancel: Arc<AtomicBool>,
    rx: Option<Receiver<Result<u32, String>>>,
    progress: Option<Progress>,
    summary: Option<String>,
    error: Option<String>,
    monitors: Vec<MonitorBounds>,
    monitors_error: Option<String>,
}

impl BookcheckbotApp {
    fn new(cc: &eframe::CreationContext<'_>, args: &Args) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        install_korean_font(&cc.egui_ctx);

        let (monitors, monitors_error) = match Screen::all() {
            Ok(screens) => (
                screens
                    .iter()
                    .map(|s| MonitorBounds {
                        x: s.display_info.x,
                        y: s.display_info.y,
                        w: s.display_info.width,
                        h: s.display_info.height,
                    })
                    .collect(),
                None,
            ),
            Err(err) => (
                Vec::new(),
                Some(format!("모니터 목록을 가져오지 못했습니다: {err}")),
            ),
        };

        Self::with_monitors(args.clone(), monitors, monitors_error)
    }

    /// 모니터 목록을 직접 받아 앱 상태를 만듭니다(테스트에서 창 없이 사용).
    fn with_monitors(
        args: Args,
        monitors: Vec<MonitorBounds>,
        monitors_error: Option<String>,
    ) -> Self {
        let screen_index = if args.screen < monitors.len() {
            args.screen
        } else {
            0
        };

        Self {
            last_page: args.pages.map(|p| p.to_string()).unwrap_or_default(),
            start_page: args.start.to_string(),
            out_dir: args.out.display().to_string(),
            screen_index,
            use_region: args.x.is_some() && args.y.is_some() && args.w.is_some() && args.h.is_some(),
            x: args.x.map(|v| v.to_string()).unwrap_or_default(),
            y: args.y.map(|v| v.to_string()).unwrap_or_default(),
            w: args.w.map(|v| v.to_string()).unwrap_or_default(),
            h: args.h.map(|v| v.to_string()).unwrap_or_default(),
            turn: args.turn,
            key: args.key.clone(),
            click_x: args.click_x.map(|v| v.to_string()).unwrap_or_default(),
            click_y: args.click_y.map(|v| v.to_string()).unwrap_or_default(),
            start_delay: args.start_delay,
            render_wait: args.render_wait,
            turn_wait: args.turn_wait,
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
            progress: None,
            summary: None,
            error: None,
            monitors,
            monitors_error,
        }
    }


    /// 화면 입력값을 CLI와 동일한 검증 로직으로 바꿔 캡처 설정을 만듭니다.
    fn build_config(&self) -> std::result::Result<crate::cli::Config, String> {
        let number = |label: &str, text: &str, min: i64| -> std::result::Result<i64, String> {
            let trimmed = text.trim().replace(',', "");
            if trimmed.is_empty() {
                return Err(format!("{label}을(를) 입력하세요."));
            }
            let value: i64 = trimmed
                .parse()
                .map_err(|_| format!("{label}은(는) 숫자로 입력하세요: {text}"))?;
            if value < min {
                return Err(format!("{label}은(는) {min} 이상이어야 합니다."));
            }
            Ok(value)
        };

        let mut argv: Vec<String> = vec!["bookcheckbot".to_string()];
        let push = |argv: &mut Vec<String>, flag: &str, value: String| {
            argv.push(flag.to_string());
            argv.push(value);
        };

        let last = number("마지막 페이지", &self.last_page, 1)?;
        let start = number("시작 페이지", &self.start_page, 1)?;
        push(&mut argv, "--pages", last.to_string());
        push(&mut argv, "--start", start.to_string());
        push(
            &mut argv,
            "--out",
            if self.out_dir.trim().is_empty() {
                "shots".to_string()
            } else {
                self.out_dir.trim().to_string()
            },
        );
        push(&mut argv, "--screen", self.screen_index.to_string());

        if self.use_region {
            let x = number("영역 X", &self.x, i32::MIN as i64)?;
            let y = number("영역 Y", &self.y, i32::MIN as i64)?;
            let w = number("영역 너비", &self.w, 1)?;
            let h = number("영역 높이", &self.h, 1)?;
            push(&mut argv, "--x", x.to_string());
            push(&mut argv, "--y", y.to_string());
            push(&mut argv, "--w", w.to_string());
            push(&mut argv, "--h", h.to_string());
        }

        match self.turn {
            TurnMode::Key => {
                if parse_key(&self.key).is_err() {
                    return Err(format!(
                        "키보드 넘김 키를 다시 선택하세요. 사용 가능: {KEY_RIGHT}, {KEY_PAGEDOWN}, {KEY_SPACE}"
                    ));
                }
                push(&mut argv, "--turn", "key".to_string());
                push(&mut argv, "--key", self.key.clone());
            }
            TurnMode::Click => {
                let cx = number("클릭 X", &self.click_x, i32::MIN as i64)?;
                let cy = number("클릭 Y", &self.click_y, i32::MIN as i64)?;
                push(&mut argv, "--turn", "click".to_string());
                push(&mut argv, "--click-x", cx.to_string());
                push(&mut argv, "--click-y", cy.to_string());
            }
        }

        push(&mut argv, "--start-delay", self.start_delay.to_string());
        push(&mut argv, "--render-wait", self.render_wait.to_string());
        push(&mut argv, "--turn-wait", self.turn_wait.to_string());

        let args = Args::try_parse_from(&argv).map_err(|err| err.to_string())?;
        match validate_args(&args) {
            Ok(Some(config)) => {
                if !self.monitors.is_empty() && config.screen >= self.monitors.len() {
                    return Err(format!(
                        "모니터 번호 {}를 찾을 수 없습니다. 사용 가능: 0..{}",
                        config.screen,
                        self.monitors.len() - 1
                    ));
                }
                Ok(config)
            }
            Ok(None) => Err("마지막 페이지를 입력하세요.".to_string()),
            Err(err) => Err(format!("{err:#}")),
        }
    }

    fn start(&mut self) {
        if self.running {
            return;
        }
        self.summary = None;
        let config = match self.build_config() {
            Ok(config) => config,
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };

        self.error = None;
        self.progress = None;
        self.running = true;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&self.cancel);
        let (tx, rx): (Sender<Result<u32, String>>, Receiver<Result<u32, String>>) = mpsc::channel();

        thread::spawn(move || {
            // 워커 스레드에서도 같은 DPI 인식 설정을 적용합니다.
            capture::enable_dpi_awareness();
            let result = capture::run(&config, &|| cancelled.load(Ordering::SeqCst), |_| {}, |_| {});
            let _ = tx.send(result.map_err(|err| format!("{err:#}")));
        });

        self.rx = Some(rx);
    }

    fn request_stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(saved)) => {
                self.running = false;
                self.rx = None;
                self.summary = Some(format!(
                    "완료: {saved}장 저장 → {}",
                    self.out_dir.trim()
                ));
            }
            Ok(Err(message)) => {
                self.running = false;
                self.rx = None;
                self.error = Some(message);
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(TryRecvError::Disconnected) => {
                self.running = false;
                self.rx = None;
                self.error = Some("캡처 스레드가 예기치 않게 종료되었습니다.".to_string());
            }
        }
    }

    fn draw_header(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Bookcheckbot").heading().strong());
        ui.label(
            RichText::new(
                "허용된 전자책 뷰어 화면을 페이지별 PNG로 저장합니다. DRM/캡처 제한은 우회하지 않습니다.",
            )
            .color(MUTED),
        );
    }

    fn draw_guide(&self, ui: &mut egui::Ui) {
        let text = [
            "1. 브라우저의 전자책을 시작 페이지로 맞추고 창 크기·확대 비율을 고정하세요.",
            "2. 아래 옵션을 채우고 [캡처 시작]을 누르면 카운트다운 뒤 자동으로 찍습니다.",
            "3. 카운트다운 동안 전자책 뷰어를 한 번 클릭해 포커스를 맞추세요.",
            "4. 캡처 중에는 키보드/마우스를 건드리지 마세요. 멈추려면 [중단].",
            "5. 흰색/검정 화면이면 차단일 수 있습니다. 우회하지 말고 중단하세요.",
        ];
        egui::Frame::new()
            .fill(Color32::from_rgb(244, 247, 240))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(206, 216, 200)))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                for line in text {
                    ui.label(RichText::new(line).size(13.0));
                }
            });
    }

    fn draw_basic(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("basic_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("마지막 페이지");
                ui.add(
                    egui::TextEdit::singleline(&mut self.last_page)
                        .hint_text("예: 458")
                        .desired_width(160.0),
                );
                ui.end_row();

                ui.label("시작 페이지");
                ui.add(
                    egui::TextEdit::singleline(&mut self.start_page)
                        .hint_text("예: 1")
                        .desired_width(160.0),
                );
                ui.end_row();

                ui.label("저장 폴더");
                ui.add(
                    egui::TextEdit::singleline(&mut self.out_dir)
                        .hint_text("예: book1")
                        .desired_width(320.0),
                );
                ui.end_row();

                ui.label("모니터");
                if self.monitors.is_empty() {
                    ui.label(RichText::new("연결된 모니터 정보 없음").color(ERROR));
                } else {
                    let current = self.screen_index.min(self.monitors.len() - 1);
                    let selected = format!("{current}번 ({})", monitor_label(self.monitors[current]));
                    egui::ComboBox::from_id_salt("monitor_combo")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for (index, bounds) in self.monitors.clone().iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.screen_index,
                                    index,
                                    format!("{index}번 ({})", monitor_label(*bounds)),
                                );
                            }
                        });
                }
                ui.end_row();
            });
        if let Some(message) = &self.monitors_error {
            ui.label(RichText::new(message).color(ERROR).size(12.0));
        }
    }

    fn draw_region(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.use_region, "캡처 영역 직접 지정 (끄면 모니터 전체)");
        ui.add_enabled_ui(self.use_region, |ui| {
            ui.label("X / Y (페이지 왼쪽 위)");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.x)
                        .hint_text("210")
                        .desired_width(140.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.y)
                        .hint_text("130")
                        .desired_width(140.0),
                );
            });
            ui.label("너비 / 높이");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.w)
                        .hint_text("1240")
                        .desired_width(140.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.h)
                        .hint_text("970")
                        .desired_width(140.0),
                );
            });
        });
        ui.label(
            RichText::new(
                "좌표는 Windows 전체 화면 기준 절대 좌표입니다. X/Y는 페이지 왼쪽 위, 너비/높이는 페이지 영역의 크기입니다.",
            )
            .color(MUTED)
            .size(12.0),
        );
    }

    fn draw_turn(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("넘김 방식");
            ui.selectable_value(&mut self.turn, TurnMode::Key, "키보드");
            ui.selectable_value(&mut self.turn, TurnMode::Click, "마우스 클릭");
        });
        match self.turn {
            TurnMode::Key => {
                ui.horizontal(|ui| {
                    ui.label("넘김 키");
                    ui.selectable_value(&mut self.key, KEY_RIGHT.to_string(), "→ Right");
                    ui.selectable_value(&mut self.key, KEY_PAGEDOWN.to_string(), "PageDown");
                    ui.selectable_value(&mut self.key, KEY_SPACE.to_string(), "Space");
                });
            }
            TurnMode::Click => {
                egui::Grid::new("click_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("클릭 X");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.click_x)
                                .hint_text("1750")
                                .desired_width(130.0),
                        );
                        ui.end_row();
                        ui.label("클릭 Y");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.click_y)
                                .hint_text("950")
                                .desired_width(130.0),
                        );
                        ui.end_row();
                    });
            }
        }
    }

    fn draw_timing(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("timing_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("시작 카운트다운");
                ui.add(
                    egui::DragValue::new(&mut self.start_delay)
                        .range(0..=60)
                        .suffix(" 초"),
                );
                ui.end_row();

                ui.label("캡처 전 대기");
                ui.add(
                    egui::DragValue::new(&mut self.render_wait)
                        .range(0..=10_000)
                        .speed(10.0)
                        .suffix(" ms"),
                );
                ui.end_row();

                ui.label("넘김 후 대기");
                ui.add(
                    egui::DragValue::new(&mut self.turn_wait)
                        .range(0..=10_000)
                        .speed(10.0)
                        .suffix(" ms"),
                );
                ui.end_row();
            });
    }

    fn draw_actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.running {
                if ui
                    .add_sized(
                        [140.0, 34.0],
                        egui::Button::new(RichText::new("중단").strong().color(Color32::WHITE))
                            .fill(ERROR),
                    )
                    .clicked()
                {
                    self.request_stop();
                }
                ui.add(egui::Spinner::new());
                ui.label(status_text(&self.progress, &self.cancel));
            } else if ui
                .add_sized(
                    [140.0, 34.0],
                    egui::Button::new(RichText::new("캡처 시작").strong().color(Color32::WHITE))
                        .fill(ACCENT),
                )
                .clicked()
            {
                self.start();
            }

            if let Some(summary) = &self.summary {
                ui.label(RichText::new(summary).color(ACCENT));
            }
        });

        if self.running {
            ui.label(
                RichText::new(
                    "진행 중에는 키보드와 마우스를 건드리지 마세요. 이 창은 다른 창보다 우선 표시됩니다.",
                )
                .color(MUTED)
                .size(12.0),
            );
        }

        if let Some(message) = &self.error {
            egui::Frame::new()
                .fill(Color32::from_rgb(253, 240, 238))
                .stroke(egui::Stroke::new(1.0, ERROR))
                .corner_radius(6)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(RichText::new(message).color(ERROR));
                });
        }
    }
}

impl eframe::App for BookcheckbotApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);

        egui::Frame::central_panel(&ctx.global_style())
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_header(ui);
                    ui.add_space(8.0);
                    self.draw_guide(ui);
                    ui.add_space(8.0);

                    ui.add_enabled_ui(!self.running, |ui| {
                        ui.heading(RichText::new("캡처 설정").size(16.0));
                        self.draw_basic(ui);
                        ui.separator();
                        self.draw_region(ui);
                        ui.separator();
                        self.draw_turn(ui);
                        ui.separator();
                        ui.heading(RichText::new("대기 시간").size(16.0));
                        self.draw_timing(ui);
                    });

                    ui.separator();
                    self.draw_actions(ui);
                });
            });
    }
}

/// 모니터 정보를 "1920x1080 @ (0, 0)" 형태로 표시합니다.
pub fn monitor_label(bounds: MonitorBounds) -> String {
    format!("{}x{} @ ({}, {})", bounds.w, bounds.h, bounds.x, bounds.y)
}

fn status_text(progress: &Option<Progress>, cancel: &AtomicBool) -> String {
    if cancel.load(Ordering::SeqCst) {
        return "중단하는 중...".to_string();
    }
    match progress {
        Some(Progress::Countdown(remaining)) => format!("{remaining}초 뒤 시작"),
        Some(Progress::Started) => "캡처 중...".to_string(),
        Some(Progress::Saved {
            page, last_page, ..
        }) => format!("{page}/{last_page} 페이지 저장 중..."),
        _ => "준비 중...".to_string(),
    }
}

/// 저장 파일 이름 규칙을 안내용으로 노출합니다(테스트 포함).
#[cfg(test)]
pub fn sample_filename(page: u32) -> String {
    filename(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn app() -> BookcheckbotApp {
        let mut a = BookcheckbotApp::with_monitors(
            Args::parse_from(["bookcheckbot"]),
            vec![MonitorBounds {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            }],
            None,
        );
        a.last_page = "3".to_string();
        a
    }

    #[test]
    fn valid_form_builds_config() {
        let mut a = app();
        a.last_page = "458".to_string();
        a.start_page = "151".to_string();
        a.out_dir = "book1".to_string();
        let config = a.build_config().expect("설정 생성 성공");
        assert_eq!(config.last_page, 458);
        assert_eq!(config.start_page, 151);
        assert_eq!(config.out, PathBuf::from("book1"));
        assert!(config.region.is_none());
        assert_eq!(config.screen, 0);
    }

    #[test]
    fn region_and_click_values_are_forwarded() {
        let mut a = app();
        a.use_region = true;
        a.x = "-1800".to_string();
        a.y = "100".to_string();
        a.w = "500".to_string();
        a.h = "600".to_string();
        a.turn = TurnMode::Click;
        a.click_x = "1750".to_string();
        a.click_y = "950".to_string();

        let config = a.build_config().expect("설정 생성 성공");
        assert_eq!(
            config.region,
            Some(Rect {
                x: -1800,
                y: 100,
                w: 500,
                h: 600
            })
        );
        match config.action {
            TurnAction::Click { x, y } => assert_eq!((x, y), (1750, 950)),
            TurnAction::Key(_) => panic!("클릭 모드여야 합니다"),
        }
    }

    #[test]
    fn empty_or_invalid_numbers_are_rejected() {
        let mut a = app();
        a.last_page = String::new();
        assert!(a.build_config().is_err());
        a.last_page = "abc".to_string();
        assert!(a.build_config().is_err());
        a.last_page = "1".to_string();
        a.start_page = "2".to_string();
        assert!(a.build_config().is_err());
    }

    #[test]
    fn region_requires_all_four_values() {
        let mut a = app();
        a.use_region = true;
        a.x = "10".to_string();
        assert!(a.build_config().is_err());
        a.y = "20".to_string();
        a.w = "0".to_string();
        a.h = "300".to_string();
        assert!(a.build_config().is_err());
        a.w = "400".to_string();
        assert!(a.build_config().is_ok());
    }

    #[test]
    fn click_mode_requires_coordinates() {
        let mut a = app();
        a.last_page = "5".to_string();
        a.turn = TurnMode::Click;
        assert!(a.build_config().is_err());
        a.click_x = "150".to_string();
        a.click_y = "250".to_string();
        assert!(a.build_config().is_ok());
    }

    #[test]
    fn unknown_monitor_index_is_rejected() {
        let mut a = app();
        a.screen_index = 3;
        assert!(a.build_config().is_err());
    }

    #[test]
    fn labels_and_status_text() {
        assert_eq!(
            monitor_label(MonitorBounds {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080
            }),
            "1920x1080 @ (-1920, 0)"
        );
        let cancel = AtomicBool::new(false);
        assert_eq!(status_text(&None, &cancel), "준비 중...");
        assert_eq!(
            status_text(
                &Some(Progress::Saved {
                    page: 2,
                    last_page: 9,
                    path: PathBuf::from("x")
                }),
                &cancel
            ),
            "2/9 페이지 저장 중..."
        );
        assert_eq!(status_text(&Some(Progress::Countdown(3)), &cancel), "3초 뒤 시작");
        assert_eq!(sample_filename(7), "page_0007.png");
    }
}
