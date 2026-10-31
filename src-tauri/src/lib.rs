#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod engines;
pub mod formats;
pub mod history;
pub mod import;
pub mod job;
pub mod probe;
pub mod runner;
pub mod selfcheck;
pub mod settings;
pub mod spawn;

use engines::registry::{
    EngineCheck, EngineManager, EngineStatus, EngineUpdateResult, MirrorTest, ProgressCb,
};
use engines::Plan;
use formats::{domain_of, targets_for, Domain};
use job::{now_ms, Job, JobStatus};
use probe::{EngineReport, Engines};
use runner::{RunOutcome, RunningMap};
use serde_json::json;
use settings::{ConvOptions, HwMode, Preset, Settings};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub jobs: tokio::sync::Mutex<Vec<Job>>,
    pub engines: StdMutex<Engines>,
    pub cancels: StdMutex<HashMap<String, Arc<AtomicBool>>>,
    pub running: RunningMap,
    pub settings: StdMutex<Settings>,
    pub engine_manager: Arc<EngineManager>,
    pub bundle_dir: PathBuf,
    pub active: AtomicUsize,
    /// 引擎后台探测是否完成（探测在启动后异步进行，避免阻塞窗口渲染）
    pub engines_ready: AtomicBool,
}

impl AppState {
    fn engines_snapshot(&self) -> Engines {
        self.engines.lock().unwrap().clone()
    }
    fn settings_snapshot(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    fn max_parallel(&self) -> usize {
        let g = &self.settings.lock().unwrap().global;
        (g.concurrent_jobs as usize).clamp(1, 8)
    }
}

/// 根据设置计算视频编码器的重试链：硬件优先，软件兜底。
/// 编码器降级链：按优先级列出本次可以尝试的编码器，
/// 前一个是硬件编码器（失败会自动退到后一个软件编码器）。
///
/// 链的构成与目标容器有关——webm 只能走 VPx，avi 只能走 MPEG-4，
/// 其余容器按用户的编码偏好与硬件设置决定。
///
/// 公开出来给测试用，确保测试走的是和 App 完全一致的降级路径。
pub fn compute_enc_chain(engines: &Engines, options: &ConvOptions, target: &str) -> Vec<String> {
    // 容器的硬性约束优先于用户偏好
    if target == "webm" {
        if !engines.vpx_chain.is_empty() {
            return engines.vpx_chain.clone();
        }
        return vec!["libvpx-vp9".into()];
    }
    if target == "avi" {
        return vec!["mpeg4".into()];
    }

    if options.video_codec.as_deref() == Some("hevc") {
        let sw = "libx265".to_string();
        let chain = &engines.hevc_chain;
        match options.hardware {
            HwMode::Off => vec![sw],
            HwMode::Force => {
                let p = options
                    .encoder
                    .clone()
                    .filter(|e| !e.is_empty() && e != "auto");
                match p {
                    Some(e) => vec![e, sw.clone()],
                    None => vec![chain.first().cloned().unwrap_or_else(|| sw.clone()), sw],
                }
            }
            HwMode::Auto => chain.clone(),
        }
    } else if options.video_codec.as_deref() == Some("vp9") {
        if !engines.vpx_chain.is_empty() {
            engines.vpx_chain.clone()
        } else {
            vec!["libvpx-vp9".into()]
        }
    } else if options.video_codec.as_deref() == Some("av1") {
        vec!["libaom-av1".into()]
    } else if options.video_codec.as_deref() == Some("mpeg4") {
        vec!["mpeg4".into()]
    } else {
        let sw = engines
            .h264_chain
            .last()
            .cloned()
            .unwrap_or_else(|| "libx264".to_string());
        let chain = &engines.h264_chain;
        match options.hardware {
            HwMode::Off => vec![sw],
            HwMode::Force => {
                let p = options
                    .encoder
                    .clone()
                    .filter(|e| !e.is_empty() && e != "auto");
                match p {
                    Some(e) => vec![e, sw.clone()],
                    None => vec![chain.first().cloned().unwrap_or_else(|| sw.clone()), sw],
                }
            }
            HwMode::Auto => chain.clone(),
        }
    }
}

fn config_dir(state: &Arc<AppState>) -> PathBuf {
    state
        .engine_manager
        .root
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("convy"))
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn engine_report(state: State<'_, Arc<AppState>>) -> EngineReport {
    state.engines_snapshot().report()
}

#[tauri::command]
fn ui_ready() {
    // 前端挂载完成回传，确认 WebView 渲染链路正常
}

#[tauri::command]
fn list_targets(ext: String) -> Vec<String> {
    targets_for(&ext)
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

#[tauri::command]
async fn add_files(
    paths: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<AddOutcome, String> {
    // 目录递归扫描是阻塞 IO，放到阻塞线程池里跑，别占住异步运行时
    let scanned = tokio::task::spawn_blocking(move || import::scan(&paths, supported_ext))
        .await
        .map_err(|e| e.to_string())?;

    let settings = state.settings_snapshot();
    let mut jobs = state.jobs.lock().await;
    let mut added = Vec::new();

    // 先建一次待处理文件的索引：逐个文件全表扫描是 O(n²)，
    // 3000 个文件时会做上百万次字符串比较，导入明显卡顿。
    let mut pending: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, j) in jobs.iter().enumerate() {
        if j.status == JobStatus::Pending {
            pending.entry(j.source.to_lowercase()).or_insert(i);
        }
    }

    for (path, rel_dir) in &scanned.files {
        let p = path.to_string_lossy().to_string();
        let ext = ext_of(&p);
        let targets = targets_for(&ext);
        if targets.is_empty() {
            continue;
        }
        // 模块设置里指定的默认目标格式优先（仅当该格式对此扩展名有效）
        let want = domain_of(&ext)
            .map(|d| settings.default_target(d))
            .unwrap_or("");
        // 同一文件已有待处理任务：不重复添加，但把目标格式对齐到最新设置
        if let Some(&idx) = pending.get(&p.to_lowercase()) {
            if let Some(existing) = jobs.get_mut(idx) {
                if !want.is_empty() && targets.contains(&want) && existing.target_format != want {
                    existing.target_format = want.to_string();
                }
            }
            continue;
        }
        let target = if !want.is_empty() && targets.contains(&want) {
            want.to_string()
        } else {
            targets[0].to_string()
        };
        let job = Job::new(path.clone(), target).in_subdir(rel_dir.clone());
        pending.insert(p.to_lowercase(), jobs.len());
        jobs.push(job.clone());
        added.push(job);
    }

    Ok(AddOutcome {
        files: scanned.files.len(),
        dirs: scanned.dirs,
        unsupported: scanned.unsupported,
        hidden: scanned.hidden,
        truncated: scanned.truncated,
        jobs: added,
    })
}

/// 导入结果：新加入的任务 + 扫描统计（供前端给出「文件夹里加了几个」的反馈）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddOutcome {
    jobs: Vec<Job>,
    /// 扫描到的受支持文件数（含已存在而被跳过重复添加的）
    files: usize,
    /// 展开的目录数
    dirs: usize,
    /// 扩展名不支持而跳过的文件数
    unsupported: usize,
    /// 隐藏 / 系统项跳过数
    hidden: usize,
    /// 是否因达到数量上限而截断
    truncated: bool,
}

/// 格式矩阵里登记过的扩展名才值得导入（同时排除 .txt 之外的杂物）。
fn supported_ext(ext: &str) -> bool {
    !targets_for(ext).is_empty()
}

#[tauri::command]
async fn set_target(
    id: String,
    format: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut jobs = state.jobs.lock().await;
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| "任务不存在".to_string())?;
    if job.status == JobStatus::Running {
        return Err("任务正在执行，无法修改目标格式".into());
    }
    job.target_format = format;
    job.status = JobStatus::Pending;
    job.progress = 0.0;
    job.error = None;
    Ok(())
}

#[tauri::command]
async fn remove_job(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if let Some(flag) = state.cancels.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    let mut jobs = state.jobs.lock().await;
    jobs.retain(|j| j.id != id);
    Ok(())
}

#[tauri::command]
async fn clear_finished(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut jobs = state.jobs.lock().await;
    jobs.retain(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running));
    Ok(())
}

/// 失败 / 取消的任务重新入队，保留原来的源文件与目标格式。
#[tauri::command]
async fn retry_job(id: String, state: State<'_, Arc<AppState>>) -> Result<Job, String> {
    let mut jobs = state.jobs.lock().await;
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| "任务不存在".to_string())?;
    if job.status == JobStatus::Running {
        return Err("任务正在执行，无法重试".into());
    }
    job.status = JobStatus::Pending;
    job.progress = 0.0;
    job.detail = None;
    job.error = None;
    job.output_path = None;
    job.output_size = None;
    job.started_at = None;
    job.finished_at = None;
    Ok(job.clone())
}

// ---------------------------------------------------------------- 转换历史

#[tauri::command]
fn list_history(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Vec<history::HistoryEntry> {
    let path = history::history_path(&config_dir(state.inner()));
    history::load(&path, limit.unwrap_or(200))
}

#[tauri::command]
fn clear_history(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let path = history::history_path(&config_dir(state.inner()));
    history::clear(&path)
}

#[tauri::command]
async fn set_overwrite(v: bool, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.settings.lock().unwrap().global.overwrite = v;
    persist_settings(state.inner());
    Ok(())
}

#[tauri::command]
async fn cancel_job(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if let Some(flag) = state.cancels.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    if let Some(pid) = state.running.lock().await.get(&id).copied() {
        runner::kill_tree(pid);
    }
    Ok(())
}

#[tauri::command]
async fn start_queue(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    domain: Option<Domain>,
) -> Result<usize, String> {
    // 引擎还在后台探测时先拒绝启动，避免用空快照跑出“引擎未找到”
    if !state.engines_ready.load(Ordering::SeqCst) {
        return Err("正在检测转换引擎，请稍候 1~2 秒再开始".to_string());
    }
    let pending: Vec<String> = {
        let jobs = state.jobs.lock().await;
        jobs.iter()
            .filter(|j| j.status == JobStatus::Pending)
            .filter(|j| domain.map(|d| j.domain == Some(d)).unwrap_or(true))
            .map(|j| j.id.clone())
            .collect()
    };
    if pending.is_empty() {
        return Ok(0);
    }

    let count = pending.len();
    let app2 = app.clone();
    let st = state.inner().clone();

    tauri::async_runtime::spawn(async move {
        run_queue(app2, st, pending).await;
    });

    Ok(count)
}

#[tauri::command]
fn reveal_output(path: String) -> Result<(), String> {
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
        let dir = p.parent().unwrap_or(Path::new("."));
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
    Ok(())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
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
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("只允许打开 http/https 网址".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 设置 / 预设
#[tauri::command]
fn get_settings(state: State<'_, Arc<AppState>>) -> Settings {
    state.settings_snapshot()
}

#[tauri::command]
async fn save_settings(settings: Settings, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    *state.settings.lock().unwrap() = settings.clone();
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
fn list_presets(state: State<'_, Arc<AppState>>) -> Vec<Preset> {
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
async fn save_preset(
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
async fn delete_preset(name: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
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

// ---------------------------------------------------------------- 引擎管理

#[tauri::command]
fn engine_list(state: State<'_, Arc<AppState>>) -> Vec<EngineStatus> {
    let engines = state.engines_snapshot();
    state.engine_manager.list(&engines)
}

#[tauri::command]
async fn engine_check_update(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<EngineCheck, String> {
    let st = state.inner().clone();
    check_one(st, id).await
}

/// 逐个检查所有引擎的最新版本（并发请求，探测本机版本放阻塞线程池）。
#[tauri::command]
async fn engine_check_all(state: State<'_, Arc<AppState>>) -> Result<Vec<EngineCheck>, String> {
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
async fn engine_selfcheck(
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
async fn engine_update(
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
async fn engine_test_mirror(url: String) -> MirrorTest {
    EngineManager::test_mirror(&url).await
}

#[tauri::command]
async fn engine_add_mirror(
    id: String,
    name: String,
    url: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.engine_manager.add_mirror(&id, &name, &url)
}

#[tauri::command]
async fn engine_remove_mirror(
    id: String,
    url: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.engine_manager.remove_mirror(&id, &url)
}

#[tauri::command]
async fn engine_rollback(
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
fn rediscover_engines(app: &AppHandle, state: &Arc<AppState>) {
    let bundle = state.bundle_dir.clone();
    let managed = state.engine_manager.root.clone();
    let new_eng = Engines::detect(&bundle, &managed);
    *state.engines.lock().unwrap() = new_eng;
    state.engines_ready.store(true, Ordering::SeqCst);
    let _ = app.emit("engines:changed", ());
}

fn persist_settings(state: &Arc<AppState>) {
    let dir = config_dir(state);
    let path = dir.join("settings.json");
    let s = state.settings.lock().unwrap().clone();
    let _ = s.save(&path);
}

// ---------------------------------------------------------------- queue

async fn run_queue(app: AppHandle, state: Arc<AppState>, ids: Vec<String>) {
    let sem = Arc::new(tokio::sync::Semaphore::new(state.max_parallel()));
    let mut handles = Vec::new();

    for id in ids {
        let permit = match sem.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => break,
        };
        let app = app.clone();
        let st = state.clone();
        handles.push(tokio::spawn(async move {
            run_one(app, st, id).await;
            drop(permit);
        }));
    }

    for h in handles {
        let _ = h.await;
    }
    let _ = app.emit("queue:done", ());
}

async fn run_one(app: AppHandle, state: Arc<AppState>, id: String) {
    let snapshot = {
        let jobs = state.jobs.lock().await;
        jobs.iter().find(|j| j.id == id).cloned()
    };
    let Some(mut job) = snapshot else { return };
    if job.status != JobStatus::Pending {
        return;
    }

    let settings = state.settings_snapshot();
    let domain = job.domain.unwrap_or(Domain::Video);
    let options: ConvOptions = settings.to_options(domain);
    let overwrite = settings.global.overwrite;
    let engines = state.engines_snapshot();

    let output = job.resolve_output(
        overwrite,
        &settings.global.output_folder,
        settings.global.preserve_tree,
    );

    {
        let mut jobs = state.jobs.lock().await;
        if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
            j.status = JobStatus::Running;
            j.started_at = Some(now_ms());
            j.progress = 0.0;
            job = j.clone();
        }
    }
    let _ = app.emit("job:update", &job);

    let cancel = Arc::new(AtomicBool::new(false));
    state
        .cancels
        .lock()
        .unwrap()
        .insert(id.clone(), cancel.clone());
    state.active.fetch_add(1, Ordering::Relaxed);

    let pid_slot = Arc::new(tokio::sync::Mutex::new(None));
    let state_for_pid = state.clone();
    let id_for_pid = id.clone();
    let pid_tracker = {
        let slot = pid_slot.clone();
        let st = state_for_pid.clone();
        let jid = id_for_pid.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                let p = *slot.lock().await;
                match p {
                    Some(pid) => {
                        st.running.lock().await.insert(jid.clone(), pid);
                    }
                    None => break,
                }
            }
            st.running.lock().await.remove(&jid);
        })
    };

    let app_prog = app.clone();
    let id_prog = id.clone();
    let last = Arc::new(StdMutex::new(0.0f32));
    let on_progress: runner::ProgressFn = Arc::new(move |p, detail| {
        let mut l = last.lock().unwrap();
        if p.is_nan() {
            drop(l);
            let _ = app_prog.emit(
                "job:progress",
                serde_json::json!({ "id": id_prog, "detail": detail }),
            );
            return;
        }
        if p - *l < 0.005 && p < 0.999 {
            return;
        }
        *l = p;
        drop(l);
        let _ = app_prog.emit(
            "job:progress",
            serde_json::json!({ "id": id_prog, "progress": p }),
        );
    });

    let enc_chain = compute_enc_chain(&engines, &options, &job.target_format);
    let mut attempt = 0usize;
    let mut outcome = RunOutcome::default();
    let mut last_err = String::new();

    loop {
        let force = if enc_chain.is_empty() {
            None
        } else {
            enc_chain.get(attempt).map(|s| s.as_str())
        };

        let plan = match build_plan(&engines, &job, &options, &output, force).await {
            Ok(p) => p,
            Err(e) => {
                last_err = e;
                break;
            }
        };

        let o = runner::execute(
            &plan,
            cancel.clone(),
            on_progress.clone(),
            Some(pid_slot.clone()),
        )
        .await;

        if o.success {
            let produced = plan.produced.clone().unwrap_or_else(|| output.clone());
            if produced.exists() {
                if let Err(e) = runner::finalize(&produced, &output) {
                    last_err = e;
                }
                for d in &plan.cleanup {
                    let _ = std::fs::remove_dir_all(d);
                }
                outcome = o;
                break;
            } else {
                last_err = "引擎执行完毕但没有产出文件".to_string();
            }
        } else if o.canceled {
            outcome = o;
            break;
        } else {
            last_err = if o.stderr.trim().is_empty() {
                format!("转换失败（退出码 {:?}）", o.exit_code)
            } else {
                tail(&o.stderr, 600)
            };
        }

        // 视频编码器失败 -> 换下一个候选
        if !enc_chain.is_empty() && attempt + 1 < enc_chain.len() {
            attempt += 1;
            continue;
        }
        outcome = RunOutcome {
            success: false,
            canceled: false,
            stderr: last_err.clone(),
            exit_code: None,
        };
        break;
    }

    pid_tracker.abort();
    state.active.fetch_sub(1, Ordering::Relaxed);
    state.cancels.lock().unwrap().remove(&id);
    state.running.lock().await.remove(&id);

    let mut jobs = state.jobs.lock().await;
    if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
        j.finished_at = Some(now_ms());
        if outcome.canceled {
            j.status = JobStatus::Canceled;
            j.detail = None;
        } else if outcome.success {
            j.status = JobStatus::Done;
            j.progress = 1.0;
            j.detail = None;
            j.output_path = Some(output.to_string_lossy().to_string());
            j.output_size = std::fs::metadata(&output).ok().map(|m| m.len());
        } else {
            j.status = JobStatus::Failed;
            j.error = Some(if last_err.is_empty() {
                "未知错误".into()
            } else {
                last_err
            });
        }
        let final_job = j.clone();
        drop(jobs);

        // 落一条历史：成功/失败/取消都记，方便回溯和「再转一次」
        let finished = final_job.finished_at.unwrap_or_else(now_ms);
        let duration_ms = finished.saturating_sub(final_job.started_at.unwrap_or(finished));
        history::append(
            &history::history_path(&config_dir(&state)),
            &history::HistoryEntry {
                id: final_job.id.clone(),
                source: final_job.source.clone(),
                file_name: final_job.file_name.clone(),
                domain: final_job.domain,
                target_format: final_job.target_format.clone(),
                status: final_job.status,
                input_size: final_job.size,
                output_size: final_job.output_size,
                output_path: final_job.output_path.clone(),
                duration_ms,
                error: final_job.error.clone(),
                finished_at: finished,
            },
        );

        let _ = app.emit("job:update", &final_job);
    }
}

async fn build_plan(
    engines: &Engines,
    job: &Job,
    options: &ConvOptions,
    output: &Path,
    force_encoder: Option<&str>,
) -> Result<Plan, String> {
    engines::plan_for(engines, job, options, output, force_encoder).await
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.trim().chars().collect();
    if chars.len() <= n {
        s.trim().to_string()
    } else {
        format!("…{}", chars[chars.len() - n..].iter().collect::<String>())
    }
}

fn ext_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

// ---------------------------------------------------------------- bootstrap

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let resource = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("convy"));
            let managed_root = data_dir.join("engines");
            let _ = std::fs::create_dir_all(&managed_root);

            let engine_manager = Arc::new(EngineManager::new(managed_root.clone()));

            // 引擎探测会 spawn 多个进程逐一验证（soffice / ebook-convert 的
            // --version 可能耗时数秒），同步执行会阻塞事件循环、窗口迟迟无法
            // 渲染（表现为启动 8 秒白屏）。改为后台线程探测：
            // 界面先用空 Engines 立即渲染，探测完成后通过 engines:changed 刷新。
            let engines = Engines::default();

            let settings_path = data_dir.join("settings.json");
            let settings = Settings::load(&settings_path);

            let state = Arc::new(AppState {
                jobs: tokio::sync::Mutex::new(Vec::new()),
                engines: StdMutex::new(engines),
                cancels: StdMutex::new(HashMap::new()),
                running: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                settings: StdMutex::new(settings),
                engine_manager: engine_manager.clone(),
                bundle_dir: resource.clone(),
                active: AtomicUsize::new(0),
                engines_ready: AtomicBool::new(false),
            });
            let app_handle = app.handle().clone();
            app.manage(state.clone());

            // 后台线程：完整引擎探测 → 更新状态 → 通知前端 → 视需要自动下载
            let detect_state = state.clone();
            let detect_resource = resource.clone();
            let detect_managed = state.engine_manager.root.clone();
            std::thread::spawn(move || {
                let engines = Engines::detect(&detect_resource, &detect_managed);
                log(&format!(
                    "engines detected: ffmpeg={:?} image={:?} ebook={:?} soffice={:?} pandoc={:?} h264={:?}",
                    engines.ffmpeg,
                    engines.magick.as_ref().or(engines.vips.as_ref()),
                    engines.ebook_convert,
                    engines.soffice,
                    engines.pandoc,
                    engines.h264_chain
                ));
                *detect_state.engines.lock().unwrap() = engines;
                detect_state.engines_ready.store(true, Ordering::SeqCst);
                let _ = app_handle.emit("engines:changed", ());

                // 首次启动自动下载核心引擎（如 ffmpeg）。受“自动检查更新”开关控制；
                // 放在探测完成之后，否则空快照会误判“未安装”而重复下载。
                let auto = detect_state.settings.lock().unwrap().global.auto_check_update;
                if auto {
                    let st2 = detect_state.clone();
                    let app2 = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let mgr = st2.engine_manager.clone();
                        for def in &mgr.manifest.engines {
                            if !def.auto {
                                continue;
                            }
                            let key = engines::manifest::platform_key();
                            let is_managed = def
                                .downloads
                                .get(&key)
                                .map(|s| matches!(s.kind.as_str(), "zip" | "tar-gz" | "tar-xz"))
                                .unwrap_or(false);
                            if !is_managed {
                                continue;
                            }
                            let snapshot = st2.engines.lock().unwrap().clone();
                            let installed = mgr
                                .list(&snapshot)
                                .iter()
                                .any(|e| e.id == def.id && e.installed);
                            if installed {
                                continue;
                            }
                            let _ = mgr
                                .update(&app2, &def.id, None, Arc::new(|_, _, _| {}))
                                .await;
                            rediscover_engines(&app2, &st2);
                        }
                    });
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_report,
            ui_ready,
            list_targets,
            add_files,
            set_target,
            retry_job,
            remove_job,
            clear_finished,
            list_history,
            clear_history,
            set_overwrite,
            cancel_job,
            start_queue,
            reveal_output,
            open_path,
            open_url,
            get_settings,
            save_settings,
            list_presets,
            save_preset,
            delete_preset,
            engine_list,
            engine_check_update,
            engine_check_all,
            engine_selfcheck,
            engine_update,
            engine_test_mirror,
            engine_add_mirror,
            engine_remove_mirror,
            engine_rollback,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Convy");
}

#[allow(dead_code)]
fn log(msg: &str) {
    use std::io::Write;
    println!("[convy] {}", msg);
    let path = std::env::temp_dir().join("convy.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "[convy] {}", msg);
    }
}
