//! 引擎自检：用最小素材真实跑一遍各类别转换，证明「装了，而且真的能用」。
//!
//! 引擎出现最多的故障不是「没装」，而是「装了但用不了」——版本不兼容、
//! 缺少运行库、编码器没编译进去、路径里有同名陷阱。只看文件是否存在无法发现这些，
//! 所以这里直接生成极小的测试素材，走与正式转换完全相同的
//! `plan_for` + `runner::execute` 链路跑一遍，把真实的 stderr 带回来。
//!
//! 素材全部在本地生成，不联网、不需要用户提供文件：
//! - 视频：ffmpeg 的 lavfi 生成 1 秒 160x120 测试画面 + 正弦音轨
//! - 音频：直接写一个 0.3 秒的 WAV（44 字节头 + 静音 PCM，不依赖任何工具）
//! - 图片：手写一个 16x16 的 24 位 BMP（无压缩，十行代码搞定）
//! - 电子书：一个极小的 HTML
//! - 文档：一段极小的 Markdown

use crate::engines;
use crate::formats::Domain;
use crate::job::Job;
use crate::probe::Engines;
use crate::runner::{self, ProgressFn};
use crate::settings::Settings;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub domain: Domain,
    /// 人类可读的分类名，如「视频（mp4 → mkv）」
    pub label: String,
    pub ok: bool,
    /// 成功时的结果摘要
    pub message: String,
    /// 失败时的原因（stderr 尾部）
    pub detail: Option<String>,
    pub elapsed_ms: u64,
    /// 缺少对应引擎，跳过
    pub skipped: bool,
}

/// 依次跑五个类别的最小转换。串行执行，避免几个引擎抢 CPU 互相拖慢。
pub async fn run(engines: &Engines, settings: &Settings, work_dir: &Path) -> Vec<CheckItem> {
    let _ = std::fs::create_dir_all(work_dir);
    let mut out = Vec::new();

    // ---- 视频：ffmpeg 生成素材，再转 mkv
    if engines.ffmpeg.is_none() {
        out.push(skipped(Domain::Video, "视频（mp4 → mkv）", "未安装 FFmpeg"));
    } else {
        let src = work_dir.join("selfcheck.mp4");
        let made = make_video(&src).await;
        match made {
            Ok(()) => out.push(
                convert(
                    engines,
                    settings,
                    Domain::Video,
                    &src,
                    "mkv",
                    "视频（mp4 → mkv）",
                )
                .await,
            ),
            Err(e) => out.push(failed(
                Domain::Video,
                "视频（mp4 → mkv）",
                "生成测试视频失败",
                e,
            )),
        }
    }

    // ---- 音频：写 WAV → mp3
    if engines.ffmpeg.is_none() {
        out.push(skipped(Domain::Audio, "音频（wav → mp3）", "未安装 FFmpeg"));
    } else {
        let src = work_dir.join("selfcheck.wav");
        match write_wav(&src) {
            Ok(()) => out.push(
                convert(
                    engines,
                    settings,
                    Domain::Audio,
                    &src,
                    "mp3",
                    "音频（wav → mp3）",
                )
                .await,
            ),
            Err(e) => out.push(failed(
                Domain::Audio,
                "音频（wav → mp3）",
                "生成测试音频失败",
                e,
            )),
        }
    }

    // ---- 图片：写 BMP → jpg（走 magick → vips → ffmpeg 三级回退）
    let has_image = engines.magick.is_some() || engines.vips.is_some() || engines.ffmpeg.is_some();
    if !has_image {
        out.push(skipped(
            Domain::Image,
            "图片（bmp → jpg）",
            "未安装 ImageMagick / libvips / FFmpeg",
        ));
    } else {
        let src = work_dir.join("selfcheck.bmp");
        match write_bmp(&src) {
            Ok(()) => out.push(
                convert(
                    engines,
                    settings,
                    Domain::Image,
                    &src,
                    "jpg",
                    "图片（bmp → jpg）",
                )
                .await,
            ),
            Err(e) => out.push(failed(
                Domain::Image,
                "图片（bmp → jpg）",
                "生成测试图片失败",
                e,
            )),
        }
    }

    // ---- 电子书：html → epub（Calibre）
    if engines.ebook_convert.is_none() {
        out.push(skipped(
            Domain::Ebook,
            "电子书（html → epub）",
            "未安装 Calibre",
        ));
    } else {
        let src = work_dir.join("selfcheck.html");
        match write_html(&src) {
            Ok(()) => out.push(
                convert(
                    engines,
                    settings,
                    Domain::Ebook,
                    &src,
                    "epub",
                    "电子书（html → epub）",
                )
                .await,
            ),
            Err(e) => out.push(failed(
                Domain::Ebook,
                "电子书（html → epub）",
                "生成测试文档失败",
                e,
            )),
        }
    }

    // ---- 文档：md → docx（Pandoc）；只有 LibreOffice 时改用 html → docx
    let (doc_src, doc_target, doc_label) = if engines.pandoc.is_some() {
        ("selfcheck.md", "docx", "文档（md → docx）")
    } else if engines.soffice.is_some() {
        ("selfcheck.html", "docx", "文档（html → docx）")
    } else {
        ("", "", "文档（md → docx）")
    };
    if doc_src.is_empty() {
        out.push(skipped(
            Domain::Document,
            doc_label,
            "未安装 Pandoc / LibreOffice",
        ));
    } else {
        let src: PathBuf = work_dir.join(doc_src);
        let prepared = if doc_src.ends_with(".md") {
            write_md(&src)
        } else {
            // html 素材上面可能已经写过；不存在就补一份
            if src.exists() {
                Ok(())
            } else {
                write_html(&src)
            }
        };
        match prepared {
            Ok(()) => out.push(
                convert(
                    engines,
                    settings,
                    Domain::Document,
                    &src,
                    doc_target,
                    doc_label,
                )
                .await,
            ),
            Err(e) => out.push(failed(Domain::Document, doc_label, "生成测试文档失败", e)),
        }
    }

    out
}

/// 用与正式转换完全相同的链路跑一次。
async fn convert(
    engines: &Engines,
    settings: &Settings,
    domain: Domain,
    src: &Path,
    target: &str,
    label: &str,
) -> CheckItem {
    let started = Instant::now();
    let job = Job::new(src.to_path_buf(), target.to_string());
    let options = settings.to_options(domain);
    let output = src
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("selfcheck-out.{target}"));
    let _ = std::fs::remove_file(&output);

    let chain = crate::compute_enc_chain(engines, &options, target);
    let cb: ProgressFn = Arc::new(|_, _| {});
    let cancel = Arc::new(AtomicBool::new(false));

    let mut last_err = String::new();
    let mut attempts = chain.iter().map(|c| Some(c.as_str())).collect::<Vec<_>>();
    attempts.push(None);

    for forced in attempts {
        let plan = match engines::plan_for(engines, &job, &options, &output, forced).await {
            Ok(p) => p,
            Err(e) => {
                last_err = e;
                break;
            }
        };
        let outcome = runner::execute(&plan, cancel.clone(), cb.clone(), None).await;
        for d in &plan.cleanup {
            let _ = std::fs::remove_dir_all(d);
        }
        if outcome.success && output.exists() {
            let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            let ms = started.elapsed().as_millis() as u64;
            return CheckItem {
                domain,
                label: label.to_string(),
                ok: true,
                message: format!("{} · 耗时 {} ms", human_size(size), ms),
                detail: None,
                elapsed_ms: ms,
                skipped: false,
            };
        }
        if outcome.canceled {
            last_err = "已取消".into();
            break;
        }
        last_err = if outcome.stderr.trim().is_empty() {
            format!("退出码 {:?}", outcome.exit_code)
        } else {
            tail(&outcome.stderr, 400)
        };
    }

    CheckItem {
        detail: Some(if last_err.is_empty() {
            "未知错误".into()
        } else {
            last_err
        }),
        ..failed(domain, label, "", String::new())
    }
}

fn skipped(domain: Domain, label: &str, why: &str) -> CheckItem {
    CheckItem {
        domain,
        label: label.to_string(),
        ok: false,
        message: format!("跳过：{why}"),
        detail: None,
        elapsed_ms: 0,
        skipped: true,
    }
}

fn failed(domain: Domain, label: &str, why: &str, detail: String) -> CheckItem {
    CheckItem {
        domain,
        label: label.to_string(),
        ok: false,
        message: if why.is_empty() {
            "转换失败".into()
        } else {
            why.into()
        },
        detail: if detail.is_empty() {
            None
        } else {
            Some(detail)
        },
        elapsed_ms: 0,
        skipped: false,
    }
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.trim().chars().collect();
    if chars.len() <= n {
        s.trim().to_string()
    } else {
        format!("…{}", chars[chars.len() - n..].iter().collect::<String>())
    }
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// 用 lavfi 生成 1 秒测试视频；优先 libx264，LGPL 构建退到 mpeg4。
async fn make_video(dest: &Path) -> Result<(), String> {
    for codec in ["libx264", "mpeg4"] {
        let status = crate::spawn::silent_tokio(tokio::process::Command::new("ffmpeg"))
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x120:rate=10:duration=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:v",
                codec,
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(dest)
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if status.success() && dest.exists() {
            return Ok(());
        }
    }
    Err("ffmpeg 未能生成测试视频（可能缺少 lavfi / 编码器）".into())
}

/// 0.3 秒 8kHz 单声道 16 位静音 WAV。
fn write_wav(dest: &Path) -> Result<(), String> {
    let sample_rate = 8000u32;
    let samples = (sample_rate as f32 * 0.3) as u32;
    let data_len = samples * 2;
    let mut buf = Vec::with_capacity(44 + data_len as usize);

    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    buf.resize(44 + data_len as usize, 0);

    std::fs::write(dest, buf).map_err(|e| e.to_string())
}

/// 16x16 24 位 BMP（无压缩），不依赖任何图像库。
fn write_bmp(dest: &Path) -> Result<(), String> {
    const W: u32 = 16;
    const H: u32 = 16;
    let row = (W * 3).div_ceil(4) * 4; // 行按 4 字节对齐
    let pixels = row * H;
    let mut buf = Vec::with_capacity((54 + pixels) as usize);

    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&(54 + pixels).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&54u32.to_le_bytes());
    buf.extend_from_slice(&40u32.to_le_bytes());
    buf.extend_from_slice(&(W as i32).to_le_bytes());
    buf.extend_from_slice(&(H as i32).to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&24u16.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&pixels.to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    for y in 0..H {
        for x in 0..W {
            // 蓝绿渐变，方便在转码后肉眼确认不是纯黑
            buf.push((x * 16) as u8);
            buf.push((y * 16) as u8);
            buf.push(200);
        }
        // 补齐行尾（BMP 每行按 4 字节对齐）
        buf.resize(buf.len() + (row - W * 3) as usize, 0);
    }

    std::fs::write(dest, buf).map_err(|e| e.to_string())
}

fn write_html(dest: &Path) -> Result<(), String> {
    let body =
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Convy 自检</title></head>\
<body><h1>Convy 引擎自检</h1><p>这是一份自动生成的测试文档，用于验证转换引擎是否可用。</p>\
<p>第二段：如果转换成功，你应该能在输出文件里看到这段文字。</p></body></html>";
    std::fs::write(dest, body).map_err(|e| e.to_string())
}

fn write_md(dest: &Path) -> Result<(), String> {
    let body = "# Convy 引擎自检\n\n这是一份自动生成的测试文档，用于验证转换引擎是否可用。\n\n- 第一项\n- 第二项\n";
    std::fs::write(dest, body).map_err(|e| e.to_string())
}
