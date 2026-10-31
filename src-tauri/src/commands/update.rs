//! 应用自身更新：检查 → 下载 → 安装。
//!
//! 真正的逻辑都在 `crate::updater`（抓 GitHub Release、镜像择优、下载、安装），
//! 这里只做三件事：把设置里的偏好读出来、把结果整理成前端要的形状、
//! 把「上次检查时间」写回去。
//!
//! 为什么不放在 `settings.rs` 里：更新检查会联网、会写磁盘、会启动子进程，
//! 和「读写设置」不是一类东西；混在一起以后又会出现一个 800 行的文件。

use crate::job::now_ms;
use crate::updater::{self, AppMirror, AppMirrorTest, AppUpdateInfo};
use crate::{lock_ok, AppState};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// 一次检查的结论。
///
/// 失败时 `error` 有值，界面直接展示原因 —— 不能假装「已是最新」，
/// 那会让用户以为真的没问题，实际是网络断了。
#[tauri::command]
pub async fn app_update_check(
    state: State<'_, Arc<AppState>>,
    force: bool,
) -> Result<AppUpdateInfo, String> {
    let (mirror, freq, last, skipped) = {
        let g = &lock_ok(&state.settings).global;
        (
            g.update_mirror.clone(),
            g.update_check_freq.clone(),
            g.last_update_check,
            g.skipped_version.clone(),
        )
    };
    let current = env!("CARGO_PKG_VERSION").to_string();

    // 没到下次检查时间就直接返回，不发请求。
    // `force` 是设置页「立即检查」按钮用的，可以无视间隔。
    if !force && !updater::is_due(last, &freq) {
        return Ok(AppUpdateInfo {
            current_version: current,
            latest_version: None,
            update_available: false,
            html_url: None,
            published_at: None,
            notes: String::new(),
            assets: Vec::new(),
            mirror_name: String::new(),
            mirror_prefix: String::new(),
            checked_at: last,
            error: None,
            skipped_by_interval: true,
            skipped_by_user: false,
        });
    }

    let result = updater::fetch_latest(&mirror).await;

    // 失败也记时间：网络不好时不应该每打开一次设置页就卡 12 秒重试一遍。
    // 注意先释放锁再落盘 —— persist_settings 内部会再取一次同一把 std::Mutex，
    // 持锁调用会直接死锁。
    {
        lock_ok(&state.settings).global.last_update_check = now_ms();
    }
    crate::commands::settings::persist_settings(state.inner());

    match result {
        Ok((rel, m)) => {
            let update_available = updater::is_newer(&rel.tag, &current);
            let skipped_by_user = !skipped.is_empty() && skipped == rel.tag;
            Ok(AppUpdateInfo {
                current_version: current,
                latest_version: Some(rel.tag),
                update_available,
                html_url: Some(rel.html_url),
                published_at: Some(rel.published_at),
                notes: rel.notes,
                assets: rel.assets,
                mirror_name: m.name,
                mirror_prefix: m.prefix,
                checked_at: now_ms(),
                error: None,
                skipped_by_interval: false,
                skipped_by_user,
            })
        }
        Err(e) => Ok(AppUpdateInfo {
            current_version: current,
            latest_version: None,
            update_available: false,
            html_url: None,
            published_at: None,
            notes: String::new(),
            assets: Vec::new(),
            mirror_name: String::new(),
            mirror_prefix: String::new(),
            checked_at: now_ms(),
            error: Some(e),
            skipped_by_interval: false,
            skipped_by_user: false,
        }),
    }
}

/// 下载结果，给前端拼提示用。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownload {
    /// 落盘后的完整路径
    pub path: String,
    pub version: String,
    pub asset: String,
    pub size: u64,
    /// 实际生效的镜像名（auto 模式下能看出走的哪条）
    pub mirror: String,
}

/// 只下载、不安装。
#[tauri::command]
pub async fn app_update_download(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UpdateDownload, String> {
    let mirror = lock_ok(&state.settings).global.update_mirror.clone();
    let (rel, _m) = updater::fetch_latest(&mirror).await?;

    let asset = updater::pick_asset(&rel.assets).ok_or_else(|| {
        format!(
            "{} 没有提供适用于当前系统的安装包，请到发布页手动下载",
            rel.tag
        )
    })?;

    // 存到系统下载目录下的 Convy 子目录：用户装完还能自己找到这个文件，
    // 放到 temp 里会被系统清理，出问题也说不清装的是哪个包
    let dir = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("Convy");
    let dest = dir.join(&asset.name);

    let name = asset.name.clone();
    let total = asset.size;
    let emitter = app.clone();
    let mirror_name = updater::download_asset(asset, &dest, &mirror, move |p, msg| {
        let _ = emitter.emit(
            "app:update-progress",
            serde_json::json!({
                "phase": "download",
                "progress": p,
                "message": msg,
                "asset": name,
                "total": total,
            }),
        );
    })
    .await?;

    Ok(UpdateDownload {
        path: dest.to_string_lossy().to_string(),
        version: rel.tag,
        asset: asset.name.clone(),
        size: asset.size,
        mirror: mirror_name,
    })
}

/// 启动安装。
///
/// Windows 上是静默安装，安装器需要替换正在运行的 exe，所以应用必须让位：
/// 给 1.5 秒让前端把「即将退出」的提示渲染出来，然后主动退出。
/// macOS / Linux 上只能把安装包交给系统默认程序（见 `updater::install`）。
#[tauri::command]
pub async fn app_update_install(
    app: AppHandle,
    path: String,
) -> Result<updater::InstallOutcome, String> {
    let outcome = updater::install(&PathBuf::from(&path))?;
    if outcome.mode == "silent" {
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            handle.exit(0);
        });
    }
    Ok(outcome)
}

/// 内置更新源列表。
#[tauri::command]
pub fn app_update_mirrors() -> Vec<AppMirror> {
    updater::list_mirrors()
}

/// 逐个测速，设置页用来告诉用户「哪条路快」。
#[tauri::command]
pub async fn app_update_test_mirrors() -> Vec<AppMirrorTest> {
    updater::test_all_mirrors().await
}
