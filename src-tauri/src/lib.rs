#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Convy · 万象转换 —— 应用装配层。
//!
//! 这里只做三件事：声明模块、定义共享状态 [`AppState`]、装配 Tauri 并启动。
//! 具体命令在 `commands::*`，队列执行在 `queue`，转换能力在 `engines`。
//!
//! 之所以这么分：命令数量到 30 个以后，全堆在一个文件里已经不好找了，
//! 而它们彼此之间其实没有耦合，唯一的共享物就是这个 `AppState`。

pub mod commands;
pub mod engines;
pub mod formats;
pub mod history;
pub mod import;
pub mod job;
pub mod probe;
pub mod queue;
pub mod runner;
pub mod selfcheck;
pub mod settings;
pub mod spawn;
pub mod updater;

use engines::registry::EngineManager;
use job::Job;
use probe::Engines;
use runner::RunningMap;
use settings::{ConvOptions, HwMode, Settings};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{Emitter, Manager};

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
    pub(crate) fn engines_snapshot(&self) -> Engines {
        self.engines.lock().unwrap().clone()
    }
    pub(crate) fn settings_snapshot(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    pub(crate) fn max_parallel(&self) -> usize {
        let g = &self.settings.lock().unwrap().global;
        (g.concurrent_jobs as usize).clamp(1, 8)
    }
}

/// 应用配置目录（`settings.json` / `presets.json` / `history.jsonl` 都在这里）。
pub(crate) fn config_dir(state: &Arc<AppState>) -> PathBuf {
    state
        .engine_manager
        .root
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("convy"))
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
                            commands::engine::rediscover_engines(&app2, &st2);
                        }
                    });
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::engine::engine_report,
            commands::system::ui_ready,
            commands::system::list_targets,
            commands::queue::add_files,
            commands::queue::set_target,
            commands::queue::retry_job,
            commands::queue::remove_job,
            commands::queue::clear_finished,
            commands::history::list_history,
            commands::history::clear_history,
            commands::queue::set_overwrite,
            commands::queue::cancel_job,
            commands::queue::start_queue,
            commands::system::reveal_output,
            commands::system::open_path,
            commands::system::open_url,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::list_presets,
            commands::settings::save_preset,
            commands::settings::delete_preset,
            commands::engine::engine_list,
            commands::engine::engine_check_update,
            commands::engine::engine_check_all,
            commands::engine::engine_selfcheck,
            commands::engine::engine_update,
            commands::engine::engine_test_mirror,
            commands::engine::engine_add_mirror,
            commands::engine::engine_remove_mirror,
            commands::engine::engine_rollback,
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
