//! 화면 캡처/페이지 넘김 실행 엔진. CLI와 GUI 워커 스레드가 함께 사용합니다.
//! 사용자가 실제로 보고 캡처가 허용된 화면만 일반 GDI 캡처와 키/마우스 입력으로 저장합니다.

use std::{
    fs,
    path::PathBuf,
    thread::sleep,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use enigo::{Enigo, KeyboardControllable, MouseButton, MouseControllable};
use screenshots::Screen;

use crate::cli::{filename, to_monitor_relative, Config, TurnAction};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct MonitorBounds {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// 실행 중 UI에 전달되는 진행 상황.
/// 캡처 엔진은 이 값을 스레드 경계 없이 직접 받기 때문에(콜백), 일부 필드는 상태 표시용입니다.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Progress {
    Starting { monitor: MonitorBounds, region: Option<Rect> },
    Countdown(u64),
    Started,
    Saved { page: u32, last_page: u32, path: PathBuf },
    Finished { saved: u32, out: PathBuf },
}

// Windows 11에서 물리 픽셀 좌표를 일관되게 다루기 위해 DPI 인식을 우선 요청합니다.
// 실패해도 OS 보호 정책을 우회하지 않고, 100% 배율 사용을 안내합니다.
#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
}

#[cfg(target_os = "windows")]
pub fn enable_dpi_awareness() {
    // -4 = DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
    if unsafe { SetProcessDpiAwarenessContext(-4) } == 0 {
        eprintln!("안내: DPI 인식 설정이 적용되지 않았습니다. 배율 100%에서 좌표를 확인하세요.");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn enable_dpi_awareness() {}

/// 렌더링/입력 부분을 주입 가능하게 분리해 GUI 없는 테스트에서도 페이지 흐름을 검증합니다.
pub fn capture_sequence<C, T>(start: u32, last: u32, mut capture: C, mut turn: T) -> Result<()>
where
    C: FnMut(u32) -> Result<()>,
    T: FnMut() -> Result<()>,
{
    for page in start..=last {
        capture(page)?;
        if page < last {
            turn()?;
        }
    }
    Ok(())
}

pub fn appears_blank(image: &screenshots::image::RgbaImage) -> bool {
    // 표본이 거의 전부 흰색/검정색이면 차단 또는 진짜 빈 페이지일 수 있습니다.
    // 잘못된 확정을 피하기 위해 '가능성'으로만 안내합니다.
    let mut samples = 0usize;
    let mut light = 0usize;
    let mut dark = 0usize;
    for p in image.pixels().step_by(503) {
        samples += 1;
        let rgb = &p.0[..3];
        if rgb.iter().all(|&c| c >= 250) {
            light += 1;
        }
        if rgb.iter().all(|&c| c <= 5) {
            dark += 1;
        }
    }
    samples > 0 && (light * 1000 >= samples * 995 || dark * 1000 >= samples * 995)
}

/// 순서대로 표시/진행 콜백을 거쳐 캡처를 수행합니다. `cancelled`가 true면 중단합니다.
pub fn run<L, P, C>(config: &Config, cancelled: &C, mut log: L, mut progress: P) -> Result<u32>
where
    L: FnMut(String),
    P: FnMut(Progress),
    C: Fn() -> bool + ?Sized,
{
    let screens = Screen::all().context("모니터 목록을 가져오지 못했습니다.")?;
    if config.screen >= screens.len() {
        bail!(
            "모니터 인덱스 {}를 찾을 수 없습니다. 사용 가능: 0..{}",
            config.screen,
            screens.len().saturating_sub(1)
        );
    }

    let screen = screens[config.screen];
    let info = screen.display_info;
    let bounds = MonitorBounds {
        x: info.x,
        y: info.y,
        w: info.width,
        h: info.height,
    };
    let relative_region = config
        .region
        .map(|rect| to_monitor_relative(rect, bounds))
        .transpose()?;
    progress(Progress::Starting {
        monitor: bounds,
        region: config.region,
    });

    fs::create_dir_all(&config.out)
        .with_context(|| format!("저장 폴더 생성 실패: {}", config.out.display()))?;

    let mut enigo = Enigo::new();

    log("=== Bookcheckbot: 전자책 화면 캡처 ===".to_string());
    log(format!("저장 폴더: {}", config.out.display()));
    log(format!(
        "페이지 범위: {} ~ {}",
        config.start_page, config.last_page
    ));
    log(format!(
        "모니터 {}: 화면 원점=({}, {}), 크기={}x{}",
        config.screen, info.x, info.y, info.width, info.height
    ));
    match config.region {
        Some(rect) => log(format!(
            "선택 영역(절대 좌표): x={}, y={}, w={}, h={}",
            rect.x, rect.y, rect.w, rect.h
        )),
        None => log("캡처 영역: 선택한 모니터 전체".to_string()),
    }
    log(format!("페이지 넘김 방식: {:?}", config.action));
    log("주의: 이 도구는 DRM이나 캡처 제한을 우회하지 않습니다.".to_string());
    log("주의: 뷰어를 시작 페이지에 직접 맞추고 창 크기를 고정하세요.".to_string());

    if config.start_delay > 0 {
        log(format!(
            "{}초 뒤 시작합니다. 전자책 뷰어에 포커스를 맞춰 주세요.",
            config.start_delay
        ));
        for remaining in (1..=config.start_delay).rev() {
            if cancelled() {
                bail!("사용자가 중단했습니다.");
            }
            log(format!("{remaining}..."));
            progress(Progress::Countdown(remaining));
            sleep(Duration::from_secs(1));
        }
    }
    if cancelled() {
        bail!("사용자가 중단했습니다.");
    }
    progress(Progress::Started);
    log("캡처 중에는 키보드와 마우스를 건드리지 마세요. 중단: 창의 중단 버튼 또는 Ctrl+C"
        .to_string());

    let mut saved = 0u32;
    let result = capture_sequence(
        config.start_page,
        config.last_page,
        |page| {
            if cancelled() {
                bail!("사용자가 중단했습니다.");
            }
            sleep(Duration::from_millis(config.render_wait));
            let path = config.out.join(filename(page));
            let image = match relative_region {
                Some(rect) => screen.capture_area(rect.x, rect.y, rect.w, rect.h),
                None => screen.capture(),
            }
            .with_context(|| {
                format!("{page}페이지 캡처 실패. 캡처가 차단되었다면 우회하지 말고 중단하세요.")
            })?;

            image
                .save(&path)
                .with_context(|| format!("{page}페이지 PNG 저장 실패: {}", path.display()))?;

            saved += 1;
            log(format!(
                "[{page}/{}] 저장: {}",
                config.last_page,
                path.display()
            ));
            progress(Progress::Saved {
                page,
                last_page: config.last_page,
                path,
            });

            if appears_blank(&image) {
                bail!(
                    "{page}페이지가 거의 완전히 흰색 또는 검정색입니다. 실제 빈 페이지일 수도 있고 캡처가 차단되었을 수도 있습니다. 저장된 이미지를 확인하세요. 화면이 미완성이라면 렌더링 대기/넘김 대기를 늘리고, DRM/캡처 제한이면 우회하지 말고 중단하세요."
                );
            }
            Ok(())
        },
        || {
            if cancelled() {
                bail!("사용자가 중단했습니다.");
            }
            match config.action {
                TurnAction::Key(key) => enigo.key_click(key),
                TurnAction::Click { x, y } => {
                    enigo.mouse_move_to(x, y);
                    sleep(Duration::from_millis(50));
                    enigo.mouse_click(MouseButton::Left);
                }
            }
            sleep(Duration::from_millis(config.turn_wait));
            Ok(())
        },
    );

    result?;
    log(format!("완료! 출력 폴더: {}", config.out.display()));
    progress(Progress::Finished {
        saved,
        out: config.out.clone(),
    });
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_pages_create_three_names_and_only_two_turns() {
        let mut names = Vec::new();
        let mut turns = 0u32;
        capture_sequence(
            1,
            3,
            |page| {
                names.push(filename(page));
                Ok(())
            },
            || {
                turns += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(names, ["page_0001.png", "page_0002.png", "page_0003.png"]);
        assert_eq!(turns, 2);
    }

    #[test]
    fn two_pages_and_resuming_work() {
        let mut pages = Vec::new();
        capture_sequence(1, 2, |page| { pages.push(page); Ok(()) }, || Ok(())).unwrap();
        assert_eq!(pages, [1, 2]);
        pages.clear();
        capture_sequence(151, 153, |page| { pages.push(page); Ok(()) }, || Ok(())).unwrap();
        assert_eq!(pages, [151, 152, 153]);
    }

    #[test]
    fn error_stops_capture_sequence_immediately() {
        let mut pages = Vec::new();
        let result = capture_sequence(
            1,
            3,
            |page| {
                if page == 2 {
                    bail!("사용자가 중단했습니다.");
                }
                pages.push(page);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(pages, [1]);
    }

    #[test]
    fn blank_screen_detection() {
        let white = screenshots::image::RgbaImage::from_pixel(
            20,
            20,
            screenshots::image::Rgba([255, 255, 255, 255]),
        );
        assert!(appears_blank(&white));
        let grey = screenshots::image::RgbaImage::from_pixel(
            20,
            20,
            screenshots::image::Rgba([120, 120, 120, 255]),
        );
        assert!(!appears_blank(&grey));
    }
}
