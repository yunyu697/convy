//! 任务队列相关的命令：导入、改目标格式、删除、重试、取消、开始转换。

use crate::commands::settings::persist_settings;
use crate::formats::{domain_of, ext_of, targets_for, Domain};
use crate::import;
use crate::job::{Job, JobStatus};
use crate::queue::run_queue;
use crate::runner;
use crate::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn add_files(
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
pub struct AddOutcome {
    pub jobs: Vec<Job>,
    /// 扫描到的受支持文件数（含已存在而被跳过重复添加的）
    pub files: usize,
    /// 展开的目录数
    pub dirs: usize,
    /// 扩展名不支持而跳过的文件数
    pub unsupported: usize,
    /// 隐藏 / 系统项跳过数
    pub hidden: usize,
    /// 是否因达到数量上限而截断
    pub truncated: bool,
}

/// 格式矩阵里登记过的扩展名才值得导入（同时排除 .txt 之外的杂物）。
fn supported_ext(ext: &str) -> bool {
    !targets_for(ext).is_empty()
}

#[tauri::command]
pub async fn set_target(
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
pub async fn remove_job(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if let Some(flag) = crate::lock_ok(&state.cancels).get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    let mut jobs = state.jobs.lock().await;
    jobs.retain(|j| j.id != id);
    Ok(())
}

#[tauri::command]
pub async fn clear_finished(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut jobs = state.jobs.lock().await;
    jobs.retain(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running));
    Ok(())
}

/// 失败 / 取消的任务重新入队，保留原来的源文件与目标格式。
#[tauri::command]
pub async fn retry_job(id: String, state: State<'_, Arc<AppState>>) -> Result<Job, String> {
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

#[tauri::command]
pub async fn cancel_job(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if let Some(flag) = crate::lock_ok(&state.cancels).get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    if let Some(pid) = state.running.lock().await.get(&id).copied() {
        runner::kill_tree(pid);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_overwrite(v: bool, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    crate::lock_ok(&state.settings).global.overwrite = v;
    persist_settings(state.inner());
    Ok(())
}

#[tauri::command]
pub async fn start_queue(
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
