//! 子进程静默启动：Windows 下给子进程加 `CREATE_NO_WINDOW`，
//! 避免控制台类子进程（ffmpeg / soffice / ebook-convert / cmd 等）
//! 在 GUI 应用里弹出黑色窗口一闪而过。

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 抑制 std::process::Command 在 Windows 上弹出控制台窗口。
pub fn silent(cmd: Command) -> Command {
    // `mut` 只在 Windows 分支用到，包一层块避免非 Windows 平台 unused_mut 警告。
    #[cfg(windows)]
    let cmd = {
        let mut cmd = cmd;
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    };
    cmd
}

/// 抑制 tokio::process::Command 在 Windows 上弹出控制台窗口。
pub fn silent_tokio(cmd: tokio::process::Command) -> tokio::process::Command {
    #[cfg(windows)]
    let cmd = {
        let mut cmd = cmd;
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    };
    cmd
}
