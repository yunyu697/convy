//! 全局设置与用户预设。
//!
//! 设置的持久化入口 [`persist_settings`] 放在这里：凡是改了 `state.settings`
//! 的命令都要调它，集中在一处可以保证「改了就落盘」这条不变量只有一份实现。

use crate::config_dir;
use crate::formats::{self, Domain};
use crate::job::JobStatus;
use crate::settings::{self, Preset, Settings};
use crate::AppState;
use std::sync::Arc;
use tauri::State;

/// 把内存中的设置写回 `settings.json`。
pub fn persist_settings(state: &Arc<AppState>) {
    let dir = config_dir(state);
    let path = dir.join("settings.json");
    let s = crate::lock_ok(&state.settings).clone();
    let _ = s.save(&path);
}

#[tauri::command]
pub fn get_settings(state: State<'_, Arc<AppState>>) -> Settings {
    state.settings_snapshot()
}

#[tauri::command]
pub async fn save_settings(
    settings: Settings,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    *crate::lock_ok(&state.settings) = settings.clone();
    persist_settings(state.inner());

    // 目标格式设置变化 → 立即同步到当前所有待处理任务，
    // 保证「设置了目标格式就按设置转换」，而不是只影响之后新加的文件。
    let mut jobs = state.jobs.lock().await;
    for job in jobs.iter_mut() {
        if job.status != JobStatus::Pending {
            continue;
        }
        let ext = formats::ext_of(&job.source);
        let Some(d) = formats::domain_of(&ext) else {
            continue;
        };
        let want = settings.default_target(d).to_string();
        if want.is_empty() || want == job.target_format {
            continue;
        }
        if formats::targets_for(&ext).contains(&want.as_str()) {
            job.target_format = want;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_presets(state: State<'_, Arc<AppState>>) -> Vec<Preset> {
    let mut out = settings::builtin_presets();
    let dir = config_dir(state.inner());
    if let Ok(text) = std::fs::read_to_string(settings::presets_path(&dir.join("settings.json"))) {
        if let Ok(user) = serde_json::from_str::<Vec<Preset>>(&text) {
            out.extend(user);
        }
    }
    out
}

#[tauri::command]
pub async fn save_preset(
    name: String,
    domain: Domain,
    settings_json: serde_json::Value,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let dir = config_dir(state.inner());
    let path = settings::presets_path(&dir.join("settings.json"));
    let mut user: Vec<Preset> = if let Ok(text) = std::fs::read_to_string(&path) {
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        Vec::new()
    };
    user.retain(|p| p.name != name);
    user.push(Preset {
        name,
        domain,
        settings: settings_json,
        builtin: false,
    });
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&user).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_preset(name: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let dir = config_dir(state.inner());
    let path = settings::presets_path(&dir.join("settings.json"));
    let mut user: Vec<Preset> = if let Ok(text) = std::fs::read_to_string(&path) {
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        Vec::new()
    };
    user.retain(|p| p.name != name);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&user).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
