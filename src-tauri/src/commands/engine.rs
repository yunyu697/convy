//! 引擎管理：报告、版本检查、自检、下载 / 更新、镜像、版本回退。

use crate::engines;
use crate::engines::registry::{
    EngineCheck, EngineManager, EngineStatus, EngineUpdateResult, MirrorTest, ProgressCb,
};
use crate::job::now_ms;
use crate::probe::{EngineReport, Engines};
use crate::selfcheck;
use crate::AppState;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn engine_report(state: State<'_, Arc<AppState>>) -> EngineReport {
    state.engines_snapshot().report()
}

#[tauri::command]
pub fn engine_list(state: State<'_, Arc<AppState>>) -> Vec<EngineStatus> {
    let engines = state.engines_snapshot();
    state.engine_manager.list(&engines)
}

#[tauri::command]
pub async fn engine_check_update(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<EngineCheck, String> {
    let st = state.inner().clone();
    check_one(st, id).await
}

/// 逐个检查所有引擎的最新版本（并发请求，探测本机版本放阻塞线程池）。
#[tauri::command]
pub async fn engine_check_all(state: State<'_, Arc<AppState>>) -> Result<Vec<EngineCheck>, String> {
    let st = state.inner().clone();
    let ids: Vec<String> = st
        .engine_manager
        .manifest
        .engines
        .iter()
        .map(|e| e.id.clone())
        .collect();

    // 本机版本要跑 `--version`（soffice 可能要 1~2 秒），放阻塞线程池避免卡住运行时
    let mgr = st.engine_manager.clone();
    let engines = st.engines_snapshot();
    let ids_for_current = ids.clone();
    let currents = tokio::task::spawn_blocking(move || {
        ids_for_current
            .into_iter()
            .map(|id| {
                let v = mgr.current_version(&engines, &id);
                (id, v)
            })
            .collect::<HashMap<String, Option<String>>>()
    })
    .await
    .map_err(|e| e.to_string())?;

    let currents = Arc::new(currents);
    let futures = ids.iter().map(|id| {
        let mgr = st.engine_manager.clone();
        let id = id.clone();
        let currents = currents.clone();
        async move {
            let current = currents.get(&id).cloned().flatten();
            let source = mgr.latest_source(&id).map(|(url, _)| url);
            let (latest, error) = match mgr.latest_version(&id).await {
                Ok(v) => (v, None),
                Err(e) => (None, Some(e)),
            };
            let update_available = matches!((&latest, &current), (Some(l), Some(c))
                if engines::registry::version_cmp(l, c) == std::cmp::Ordering::Greater);
            EngineCheck {
                id,
                current,
                latest,
                update_available,
                error,
                source,
                checked_at: now_ms(),
            }
        }
    });
    Ok(futures_util::future::join_all(futures).await)
}

/// 引擎自检：本地生成极小素材，真实跑一遍五类转换。
#[tauri::command]
pub async fn engine_selfcheck(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<selfcheck::CheckItem>, String> {
    let st = state.inner().clone();
    if !st.engines_ready.load(Ordering::SeqCst) {
        return Err("正在检测转换引擎，请稍候再试".to_string());
    }
    let engines = st.engines_snapshot();
    let settings = st.settings_snapshot();
    let dir = std::env::temp_dir().join(format!("convy-selfcheck-{}", now_ms()));
    let items = selfcheck::run(&engines, &settings, &dir).await;
    // 无论成败都清掉临时素材
    let _ = std::fs::remove_dir_all(&dir);
    Ok(items)
}

async fn check_one(state: Arc<AppState>, id: String) -> Result<EngineCheck, String> {
    let engines = state.engines_snapshot();
    let mgr = state.engine_manager.clone();
    // current_version 会 spawn 子进程，同样放阻塞线程池
    let mgr2 = mgr.clone();
    let id2 = id.clone();
    let current = tokio::task::spawn_blocking(move || mgr2.current_version(&engines, &id2))
        .await
        .map_err(|e| e.to_string())?;

    let source = mgr.latest_source(&id).map(|(url, _)| url);
    let (latest, error) = match mgr.latest_version(&id).await {
        Ok(v) => (v, None),
        Err(e) => (None, Some(e)),
    };
    let update_available = matches!((&latest, &current), (Some(l), Some(c))
        if engines::registry::version_cmp(l, c) == std::cmp::Ordering::Greater);
    Ok(EngineCheck {
        id,
        current,
        latest,
        update_available,
        error,
        source,
        checked_at: now_ms(),
    })
}

#[tauri::command]
pub async fn engine_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    mirror_url: Option<String>,
) -> Result<EngineUpdateResult, String> {
    let id_for_cb = id.clone();
    let cb: ProgressCb = Arc::new({
        let app = app.clone();
        move |phase, progress, message| {
            let _ = app.emit(
                "engine:progress",
                json!({ "id": id_for_cb, "phase": phase, "progress": progress, "message": message }),
            );
        }
    });
    let result = state
        .engine_manager
        .update(&app, &id, mirror_url.as_deref(), cb)
        .await?;
    rediscover_engines(&app, state.inner());
    Ok(result)
}

#[tauri::command]
pub async fn engine_test_mirror(url: String) -> MirrorTest {
    EngineManager::test_mirror(&url).await
}

#[tauri::command]
pub async fn engine_add_mirror(
    id: String,
    name: String,
    url: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.engine_manager.add_mirror(&id, &name, &url)
}

#[tauri::command]
pub async fn engine_remove_mirror(
    id: String,
    url: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.engine_manager.remove_mirror(&id, &url)
}

#[tauri::command]
pub async fn engine_rollback(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    version: String,
) -> Result<(), String> {
    state.engine_manager.set_active(&id, &version)?;
    rediscover_engines(&app, state.inner());
    Ok(())
}

/// 下载/回退后重新探测引擎，使新二进制立即生效。
pub fn rediscover_engines(app: &AppHandle, state: &Arc<AppState>) {
    let bundle = state.bundle_dir.clone();
    let managed = state.engine_manager.root.clone();
    let new_eng = Engines::detect(&bundle, &managed);
    *state.engines.lock().unwrap() = new_eng;
    state.engines_ready.store(true, Ordering::SeqCst);
    let _ = app.emit("engines:changed", ());
}
