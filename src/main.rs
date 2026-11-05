//! Bookcheckbot 실행 진입점.
//! 인자 없이 실행하면 egui GUI 창이 뜨고, `--pages`를 주면 기존처럼 CLI로 바로 캡처합니다.

// GUI 전용 빌드에서는 콘솔 창 없이 창만 뜨게 해 여러 번 실행해도 검은 콘솔이 남지 않습니다.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod capture;
mod cli;
mod gui;

use anyhow::Result;
use clap::Parser;

use crate::cli::{validate_args, Args};

fn main() {
    let args = Args::parse();
    if let Err(err) = run(args) {
        // windows_subsystem = "windows" 빌드에서는 콘솔이 없으므로 메시지 창으로 알립니다.
        report_error(&format!("{err:#}"));
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    match validate_args(&args)? {
        Some(config) => {
            // CLI 모드: 인자를 그대로 써서 실행하고 종료합니다.
            capture::enable_dpi_awareness();
            capture::run(
                &config,
                &|| false,
                |line| println!("{line}"),
                |_| {},
            )?;
            Ok(())
        }
        None => {
            // GUI 모드
            capture::enable_dpi_awareness();
            gui::launch(&args)
        }
    }
}

fn report_error(message: &str) {
    eprintln!("{message}");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        // MessageBoxW: 0x10 = MB_ICONERROR, 0x0 = MB_OK
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let title = "Bookcheckbot 오류";
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!(
                    "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show({}, {}, 0, 16)",
                    ps_quote(message),
                    ps_quote(title)
                ),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
}

/// PowerShell 단일 인용 문자열로 안전하게 감쌉니다.
#[cfg(target_os = "windows")]
fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
