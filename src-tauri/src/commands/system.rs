//! 系统交互：界面就绪信号、用系统默认程序打开文件 / 网址。

use crate::formats::targets_for;
use std::path::PathBuf;
use tauri::AppHandle;

#[tauri::command]
pub fn ui_ready() {
    // 前端挂载完成回传，确认 WebView 渲染链路正常
}

#[tauri::command]
pub fn list_targets(ext: String) -> Vec<String> {
    targets_for(&ext)
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

#[tauri::command]
pub fn reveal_output(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("文件不存在".into());
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .args(["-R", &path])
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = p.parent().unwrap_or(std::path::Path::new("."));
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("文件不存在".into());
    }
    #[cfg(windows)]
    {
        // 用 explorer 打开默认程序，避免 cmd /C start 弹出控制台窗口
        let _ = std::process::Command::new("explorer").arg(&path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
    }
    Ok(())
}

/// 在系统默认浏览器中打开网址（引擎官网、下载页等）。
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("只允许打开 http/https 网址".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
