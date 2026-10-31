//! 执行器：启动引擎进程、解析进度、处理取消。
//!
//! 三个容易踩的点都在这里处理了：
//! 1. stdout / stderr 必须同时读，否则管道写满会让子进程阻塞
//! 2. 取消要杀整个进程树，只 kill 父进程会留下孤儿
//! 3. 输出先写临时文件，成功后再原子改名，避免半成品覆盖原文件

use crate::engines::{Plan, ProgressMode};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, Default)]
pub struct RunOutcome {
    pub success: bool,
    pub canceled: bool,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

pub type ProgressFn = Arc<dyn Fn(f32, Option<String>) + Send + Sync>;

/// 正在运行的进程表：job_id -> pid，用于取消。
pub type RunningMap = Arc<tokio::sync::Mutex<HashMap<String, u32>>>;

pub async fn execute(
    plan: &Plan,
    cancel: Arc<AtomicBool>,
    on_progress: ProgressFn,
    pid_slot: Option<Arc<tokio::sync::Mutex<Option<u32>>>>,
) -> RunOutcome {
    let mut cmd = tokio::process::Command::new(&plan.program);
    cmd.args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (k, v) in &plan.envs {
        cmd.env(k, v);
    }
    if let Some(dir) = &plan.work_dir {
        cmd.current_dir(dir);
    }

    #[cfg(windows)]
    {
        // 不弹出控制台黑框
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        // 自成进程组，取消时可整组击杀
        cmd.process_group(0);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return RunOutcome {
                success: false,
                canceled: false,
                stderr: format!("无法启动 {}：{}", plan.program.display(), e),
                exit_code: None,
            }
        }
    };

    let pid = child.id();
    if let (Some(slot), Some(p)) = (&pid_slot, pid) {
        *slot.lock().await = Some(p);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // stdout：FFmpeg 的 -progress 输出
    let stdout_handle = {
        let cb = on_progress.clone();
        let mode = plan.progress;
        stdout.map(|s| {
            tokio::spawn(async move {
                read_stream(s, move |line| {
                    if let ProgressMode::Ffmpeg { duration_us } = mode {
                        if let Some((p, d)) = parse_ffmpeg_progress(line, duration_us) {
                            cb(p, d);
                        }
                    }
                })
                .await
            })
        })
    };

    // stderr：错误信息 + Calibre 的百分比
    let stderr_handle = {
        let cb = on_progress.clone();
        let mode = plan.progress;
        stderr.map(|s| {
            tokio::spawn(async move {
                let mut buf = String::new();
                read_stream(s, |line| {
                    if let ProgressMode::Percent = mode {
                        if let Some(p) = parse_percent(line) {
                            cb(p, None);
                        }
                    }
                    if buf.len() < 64 * 1024 {
                        buf.push_str(line);
                        buf.push('\n');
                    }
                })
                .await;
                buf
            })
        })
    };

    // 主循环：等待退出 / 消费进度 / 响应取消
    let mut canceled = false;
    let status = loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep(std::time::Duration::from_millis(120)) => {
                if cancel.load(Ordering::Relaxed) {
                    if let Some(p) = pid { kill_tree(p); }
                    canceled = true;
                    let _ = child.wait().await;
                    break None;
                }
            }
            s = child.wait() => { break s.ok(); }
        }
    };

    if let Some(h) = stdout_handle {
        let _ = h.await;
    }
    let err_text = match stderr_handle {
        Some(h) => h.await.unwrap_or_default(),
        None => String::new(),
    };

    if let Some(slot) = &pid_slot {
        *slot.lock().await = None;
    }

    let success = !canceled && status.map(|s| s.success()).unwrap_or(false);

    RunOutcome {
        success,
        canceled,
        stderr: err_text,
        exit_code: status.and_then(|s| s.code()),
    }
}

/// 按 \r 或 \n 切分流式输出（Calibre 用 \r 刷新同一行）。
async fn read_stream<R: AsyncRead + Unpin>(mut r: R, mut on_line: impl FnMut(&str)) {
    let mut buf = [0u8; 8192];
    let mut acc: Vec<u8> = Vec::new();

    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' {
                if !acc.is_empty() {
                    let line = String::from_utf8_lossy(&acc).to_string();
                    if !line.trim().is_empty() {
                        on_line(&line);
                    }
                    acc.clear();
                }
            } else {
                acc.push(b);
                if acc.len() > 16 * 1024 {
                    acc.clear();
                }
            }
        }
    }
    if !acc.is_empty() {
        let line = String::from_utf8_lossy(&acc).to_string();
        if !line.trim().is_empty() {
            on_line(&line);
        }
    }
}

fn parse_ffmpeg_progress(line: &str, duration_us: u64) -> Option<(f32, Option<String>)> {
    if let Some(v) = line.strip_prefix("out_time_us=") {
        let us: u64 = v.trim().parse().ok()?;
        if duration_us > 0 {
            let p = (us as f32 / duration_us as f32).clamp(0.0, 0.999);
            return Some((p, None));
        }
    }
    // 旧版 FFmpeg 的 out_time_ms 实际单位也是微秒
    if let Some(v) = line.strip_prefix("out_time_ms=") {
        let us: u64 = v.trim().parse().ok()?;
        if duration_us > 0 {
            let p = (us as f32 / duration_us as f32).clamp(0.0, 0.999);
            return Some((p, None));
        }
    }
    if let Some(v) = line.strip_prefix("speed=") {
        let s = v.trim();
        if s != "N/A" && !s.is_empty() {
            return Some((f32::NAN, Some(s.to_string())));
        }
    }
    None
}

fn parse_percent(line: &str) -> Option<f32> {
    let idx = line.find('%')?;
    let head = &line[..idx];
    let digits: String = head
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if digits.is_empty() {
        return None;
    }
    let v: f32 = digits.parse().ok()?;
    if (0.0..=100.0).contains(&v) {
        Some((v / 100.0).clamp(0.0, 0.999))
    } else {
        None
    }
}

/// 杀掉整个进程树。Windows 用 taskkill /T，Unix 用进程组负号。
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = crate::spawn::silent(std::process::Command::new("taskkill"))
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &format!("-{}", pid)])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// 引擎产出路径 -> 目标路径。跨目录时先复制到同目录临时名再改名，保证原子性。
pub fn finalize(produced: &Path, output: &Path) -> Result<(), String> {
    if produced == output {
        return Ok(());
    }
    if !produced.exists() {
        return Err("引擎未产出文件".into());
    }
    let tmp = output.with_extension(format!(
        "{}.part",
        output.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));
    std::fs::copy(produced, &tmp).map_err(|e| format!("写入输出失败：{}", e))?;
    std::fs::rename(&tmp, output).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("重命名输出失败：{}", e)
    })?;
    let _ = std::fs::remove_file(produced);
    Ok(())
}
