//! 转换历史：读取最近记录、清空。

use crate::config_dir;
use crate::history;
use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn list_history(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Vec<history::HistoryEntry> {
    let path = history::history_path(&config_dir(state.inner()));
    history::load(&path, limit.unwrap_or(200))
}

#[tauri::command]
pub fn clear_history(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let path = history::history_path(&config_dir(state.inner()));
    history::clear(&path)
}
