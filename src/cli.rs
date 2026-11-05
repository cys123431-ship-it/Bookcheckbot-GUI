//! 명령줄 인자 처리. GUI 실행 없이 기존 CLI 방식으로도 그대로 쓸 수 있습니다.
//! 사용자가 보는 화면을 일반적인 캡처/입력 API로만 저장하며 DRM/캡처 방지를 우회하지 않습니다.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use enigo::Key;

use crate::capture::{MonitorBounds, Rect};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "bookcheckbot",
    version,
    about = "허용된 전자책 뷰어 화면을 페이지별 PNG로 저장합니다 (개인 열람용). 인자 없이 실행하면 GUI가 열립니다."
)]
pub struct Args {
    /// 마지막 페이지 번호(1 이상). 입력하면 GUI 없이 바로 캡처합니다.
    #[arg(long)]
    pub pages: Option<u32>,

    /// 시작 페이지 번호. 뷰어도 같은 페이지로 직접 이동해야 합니다.
    #[arg(long, default_value_t = 1)]
    pub start: u32,

    /// PNG 저장 폴더
    #[arg(long, default_value = "shots")]
    pub out: PathBuf,

    /// 캡처 영역: Windows 가상 화면 기준 왼쪽 위 X 좌표
    #[arg(long, allow_hyphen_values = true)]
    pub x: Option<i32>,

    /// 캡처 영역: Windows 가상 화면 기준 왼쪽 위 Y 좌표
    #[arg(long, allow_hyphen_values = true)]
    pub y: Option<i32>,

    /// 캡처 영역 너비
    #[arg(long)]
    pub w: Option<u32>,

    /// 캡처 영역 높이
    #[arg(long)]
    pub h: Option<u32>,

    /// 캡처할 모니터 인덱스(0부터)
    #[arg(long, default_value_t = 0)]
    pub screen: usize,

    /// 페이지 넘김 방식: key 또는 click
    #[arg(long, value_enum, default_value_t = TurnMode::Key)]
    pub turn: TurnMode,

    /// 키보드 넘김 키: right, pagedown, space
    #[arg(long, default_value = "right")]
    pub key: String,

    /// 마우스로 넘길 때 클릭할 가상 화면 X 좌표
    #[arg(long, allow_hyphen_values = true)]
    pub click_x: Option<i32>,

    /// 마우스로 넘길 때 클릭할 가상 화면 Y 좌표
    #[arg(long, allow_hyphen_values = true)]
    pub click_y: Option<i32>,

    /// 캡처 전 카운트다운(초)
    #[arg(long, default_value_t = 5)]
    pub start_delay: u64,

    /// 캡처 직전 화면 렌더링 대기(ms)
    #[arg(long, default_value_t = 700)]
    pub render_wait: u64,

    /// 페이지 넘김 직후 대기(ms)
    #[arg(long, default_value_t = 800)]
    pub turn_wait: u64,
}

#[derive(Debug, Copy, Clone, ValueEnum, PartialEq, Eq)]
pub enum TurnMode {
    Key,
    Click,
}

#[derive(Debug, Copy, Clone)]
pub enum TurnAction {
    Key(Key),
    Click { x: i32, y: i32 },
}

#[derive(Debug)]
pub struct Config {
    pub last_page: u32,
    pub start_page: u32,
    pub screen: usize,
    pub out: PathBuf,
    pub region: Option<Rect>,
    pub action: TurnAction,
    pub start_delay: u64,
    pub render_wait: u64,
    pub turn_wait: u64,
}

pub fn parse_key(name: &str) -> Result<Key> {
    match name.to_ascii_lowercase().as_str() {
        "right" | "rightarrow" | "right_arrow" => Ok(Key::RightArrow),
        "pagedown" | "page_down" | "next" => Ok(Key::PageDown),
        "space" | "spacebar" => Ok(Key::Space),
        other => bail!("지원하지 않는 --key 값: {other}. 사용 가능: right, pagedown, space"),
    }
}

/// `--pages`가 있을 때(CLI 모드)만 인자를 검증해 캡처 설정을 만듭니다.
pub fn validate_args(args: &Args) -> Result<Option<Config>> {
    let Some(last_page) = args.pages else {
        return Ok(None);
    };

    if args.start == 0 || last_page == 0 {
        bail!("--pages와 --start는 1 이상의 정수여야 합니다.");
    }
    if last_page < args.start {
        bail!(
            "--pages({last_page})는 --start({})보다 작을 수 없습니다.",
            args.start
        );
    }

    let region = match (args.x, args.y, args.w, args.h) {
        (None, None, None, None) => None,
        (Some(x), Some(y), Some(w), Some(h)) => {
            if w == 0 || h == 0 {
                bail!("영역 크기 --w와 --h는 0보다 커야 합니다.");
            }
            Some(Rect { x, y, w, h })
        }
        _ => bail!("영역 캡처에는 --x --y --w --h 네 옵션이 모두 필요합니다."),
    };

    let action = match args.turn {
        TurnMode::Key => TurnAction::Key(parse_key(&args.key)?),
        TurnMode::Click => TurnAction::Click {
            x: args
                .click_x
                .context("--turn click 사용 시 --click-x가 필요합니다.")?,
            y: args
                .click_y
                .context("--turn click 사용 시 --click-y가 필요합니다.")?,
        },
    };

    Ok(Some(Config {
        last_page,
        start_page: args.start,
        screen: args.screen,
        out: args.out.clone(),
        region,
        action,
        start_delay: args.start_delay,
        render_wait: args.render_wait,
        turn_wait: args.turn_wait,
    }))
}

/// 사용자 입력은 Windows 전체 화면의 절대 좌표.
/// screenshots::Screen::capture_area는 선택한 모니터 내 상대 좌표를 받습니다.
/// 라이브러리는 바깥쪽 영역을 조용히 잘라낼 수 있으므로 명시적으로 거부합니다.
pub fn to_monitor_relative(region: Rect, screen: MonitorBounds) -> Result<Rect> {
    let left = i64::from(region.x);
    let top = i64::from(region.y);
    let right = left + i64::from(region.w);
    let bottom = top + i64::from(region.h);
    let sx = i64::from(screen.x);
    let sy = i64::from(screen.y);
    let sx2 = sx + i64::from(screen.w);
    let sy2 = sy + i64::from(screen.h);

    if left < sx || top < sy || right > sx2 || bottom > sy2 {
        bail!(
            "지정한 캡처 영역 ({}, {}, {}, {})이 선택한 모니터의 범위 ({}, {}, {}, {}) 밖입니다.",
            region.x,
            region.y,
            region.w,
            region.h,
            screen.x,
            screen.y,
            screen.w,
            screen.h
        );
    }

    Ok(Rect {
        x: i32::try_from(left - sx).context("영역 X 좌표 변환 실패")?,
        y: i32::try_from(top - sy).context("영역 Y 좌표 변환 실패")?,
        w: region.w,
        h: region.h,
    })
}

pub fn filename(page: u32) -> String {
    format!("page_{page:04}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Args {
        Args::try_parse_from(parts).expect("CLI 파싱 성공")
    }

    fn config(parts: &[&str]) -> Result<Config> {
        validate_args(&args(parts))?.context("--pages가 필요합니다")
    }

    #[test]
    fn default_options_are_applied() {
        let a = args(&["bookcheckbot", "--pages", "3"]);
        assert_eq!(a.start, 1);
        assert_eq!(a.screen, 0);
        assert_eq!(a.turn, TurnMode::Key);
        assert_eq!(a.out, PathBuf::from("shots"));
        assert_eq!(a.render_wait, 700);
        assert_eq!(a.turn_wait, 800);
        assert_eq!(config(&["bookcheckbot", "--pages", "3"]).unwrap().last_page, 3);
    }

    #[test]
    fn no_pages_means_gui_mode() {
        assert!(validate_args(&args(&["bookcheckbot"])).unwrap().is_none());
    }

    #[test]
    fn invalid_page_range_is_rejected() {
        assert!(config(&["bookcheckbot", "--pages", "1", "--start", "2"]).is_err());
        assert!(config(&["bookcheckbot", "--pages", "0"]).is_err());
        assert!(config(&["bookcheckbot", "--pages", "2", "--start", "0"]).is_err());
    }

    #[test]
    fn partial_and_empty_regions_are_rejected() {
        assert!(config(&["bookcheckbot", "--pages", "3", "--x", "10"]).is_err());
        assert!(config(&[
            "bookcheckbot", "--pages", "3", "--x", "10", "--y", "20", "--w", "0", "--h", "200",
        ])
        .is_err());
    }

    #[test]
    fn click_mode_needs_both_coordinates() {
        assert!(config(&["bookcheckbot", "--pages", "3", "--turn", "click"]).is_err());
        assert!(config(&[
            "bookcheckbot", "--pages", "3", "--turn", "click", "--click-x", "150",
        ])
        .is_err());
        assert!(config(&[
            "bookcheckbot", "--pages", "3", "--turn", "click", "--click-x", "150", "--click-y", "250",
        ])
        .is_ok());
    }

    #[test]
    fn allowed_keys_and_unsupported_key() {
        for key in ["right", "pagedown", "space"] {
            assert!(parse_key(key).is_ok());
        }
        assert!(parse_key("enter").is_err());
    }

    #[test]
    fn monitor_coordinates_support_negative_offsets() {
        let relative = to_monitor_relative(
            Rect { x: -1800, y: 100, w: 500, h: 600 },
            MonitorBounds { x: -1920, y: 0, w: 1920, h: 1080 },
        )
        .unwrap();
        assert_eq!(relative, Rect { x: 120, y: 100, w: 500, h: 600 });
    }

    #[test]
    fn region_outside_selected_monitor_fails() {
        assert!(to_monitor_relative(
            Rect { x: 1700, y: 0, w: 300, h: 100 },
            MonitorBounds { x: 0, y: 0, w: 1920, h: 1080 },
        )
        .is_err());
    }
}
