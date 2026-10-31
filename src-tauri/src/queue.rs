//! 队列执行：并发调度、单个任务的重试降级、收尾落历史。
//!
//! 与 `commands::queue` 的分工：那边是 IPC 入口（校验参数、改状态），
//! 这边是真正跑转换的地方，不依赖 `State`，只拿 `Arc<AppState>`，
//! 方便在后台任务里持有。

use crate::config_dir;
use crate::engines::{self, Plan};
use crate::formats::Domain;
use crate::history;
use crate::job::{now_ms, Job, JobStatus};
use crate::probe::Engines;
use crate::runner::{self, RunOutcome};
use crate::settings::ConvOptions;
use crate::AppState;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter};

pub async fn run_queue(app: AppHandle, state: Arc<AppState>, ids: Vec<String>) {
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

pub async fn run_one(app: AppHandle, state: Arc<AppState>, id: String) {
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

    let enc_chain = crate::compute_enc_chain(&engines, &options, &job.target_format);
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
