//! FFmpeg 引擎：视频 / 音频。
//!
//! 精细化参数全部来自 `ConvOptions`：分辨率缩放、视频/音频编码器、码率或 CRF、
//! 帧率、硬件加速模式、faststart、去隔行等。硬件编码器的选择与降级由调用方
//! （run_one）通过 `force_encoder` 驱动，本模块只负责把选项翻译成 ffmpeg 参数。

use super::{Plan, ProgressMode};
use crate::formats;
use crate::probe::Engines;
use crate::settings::{ConvOptions, HwMode};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[allow(clippy::too_many_arguments)]
pub fn build(
    engines: &Engines,
    input: &Path,
    output: &Path,
    target: &str,
    options: &ConvOptions,
    duration_us: Option<u64>,
    source_width: Option<u32>,
    force_encoder: Option<&str>,
) -> Result<Plan, String> {
    let ffmpeg = engines
        .ffmpeg
        .clone()
        .ok_or_else(|| "未找到 FFmpeg".to_string())?;

    let mut a: Vec<OsString> = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        "-i".into(),
        input.as_os_str().to_owned(),
    ];

    let is_audio_out = formats::is_audio_target(target);

    if is_audio_out {
        a.extend(audio_args(options, target));
    } else if target == "gif" {
        a.extend(gif_args(source_width, options));
    } else {
        let encoder = select_video_encoder(engines, options, target, force_encoder);
        let (vc, extra) = video_codec_params(target, &encoder, options);
        a.extend(vc);
        a.extend(extra);

        // 音频流（除非用户选择丢弃）
        if options.audio_codec.as_deref() == Some("none") {
            a.push("-an".into());
        } else if target == "webm" {
            // webm 只能用 opus；码率跟随设置（「保持源参数」时即源音轨码率）
            let br = options
                .audio_bitrate_kb
                .map(|b| format!("{}k", b))
                .unwrap_or_else(|| "96k".into());
            a.extend(["-c:a".into(), "libopus".into(), "-b:a".into(), br.into()]);
        } else if target != "gif" {
            a.extend(audio_stream_args(options, target));
        }

        if matches!(target, "mp4" | "m4v" | "mov") && options.faststart {
            a.extend(["-movflags".into(), "+faststart".into()]);
        }
    }

    // 帧率
    if let Some(fps) = options.fps {
        a.extend(["-r".into(), format!("{:.3}", fps).into()]);
    }

    a.push(output.as_os_str().to_owned());

    Ok(Plan {
        program: ffmpeg,
        args: a,
        progress: ProgressMode::Ffmpeg {
            duration_us: duration_us.unwrap_or(0),
        },
        work_dir: None,
        cleanup: vec![],
        produced: None,
        envs: vec![],
        hardware: matches!(options.hardware, HwMode::Force) || force_encoder.is_some(),
    })
}

/// 目标容器能否容纳该视频编码器。
///
/// webm 只能装 VP8/VP9/AV1，avi 只能装 MPEG-4/MJPEG。
/// 之前忽略了这层约束，导致 mp4→webm 会用 H.264 编码，直接被 ffmpeg 拒绝。
fn container_allows(target: &str, encoder: &str) -> bool {
    let e = encoder.to_ascii_lowercase();
    match target {
        "webm" => {
            e.contains("vpx")
                || e.contains("vp9")
                || e.contains("vp8")
                || e.contains("aom")
                || e.contains("av1")
        }
        "avi" => e == "mpeg4" || e.contains("mjpeg"),
        _ => true,
    }
}

/// 根据设置、目标容器与硬件降级状态选择最终视频编码器名称。
fn select_video_encoder(
    engines: &Engines,
    options: &ConvOptions,
    target: &str,
    force: Option<&str>,
) -> String {
    // 强制指定的编码器也要过容器兼容性检查
    if let Some(f) = force {
        if container_allows(target, f) {
            return f.to_string();
        }
    }

    let vc = options.video_codec.as_deref().unwrap_or("auto");

    // webm 容器：只能 VP9 / AV1
    if target == "webm" {
        return match vc {
            "av1" => "libaom-av1".to_string(),
            _ => engines
                .vpx_chain
                .first()
                .cloned()
                .unwrap_or_else(|| "libvpx-vp9".to_string()),
        };
    }

    // avi 容器：MPEG-4 Part 2 最稳
    if target == "avi" {
        return "mpeg4".to_string();
    }

    match vc {
        "vp9" => "libvpx-vp9".to_string(),
        "av1" => "libaom-av1".to_string(),
        "mpeg4" => "mpeg4".to_string(),
        "hevc" => {
            if !engines.hevc_chain.is_empty() {
                engines.hevc_chain[0].clone()
            } else {
                "libx265".to_string()
            }
        }
        // auto / h264
        _ => {
            let sw = engines
                .h264_chain
                .last()
                .cloned()
                .unwrap_or_else(|| "libx264".to_string());
            match options.hardware {
                HwMode::Off => sw,
                HwMode::Force => options
                    .encoder
                    .clone()
                    .filter(|e| !e.is_empty() && e != "auto")
                    .unwrap_or_else(|| engines.h264_chain.first().cloned().unwrap_or(sw)),
                HwMode::Auto => engines.h264_chain.first().cloned().unwrap_or(sw),
            }
        }
    }
}

/// 视频画面滤镜链：缩放、去隔行、旋转、翻转。
///
/// 统一在一处构造，保证顺序稳定（先几何校正再缩放会导致缩放基准不同，
/// 这里选择先缩放后旋转，与大多数转码工具的观感一致）。
fn video_filters(options: &ConvOptions) -> Vec<String> {
    let mut f = Vec::new();
    if let Some((w, h)) = options.scale {
        f.push(format!("scale={}:{}", w, h));
    }
    if options.deinterlace {
        f.push("yadif".into());
    }
    match options.rotation.as_str() {
        "90" => f.push("transpose=1".into()),
        // 180 = 连续两次 90
        "180" => {
            f.push("transpose=1".into());
            f.push("transpose=1".into());
        }
        "270" => f.push("transpose=2".into()),
        _ => {}
    }
    if options.flip_h {
        f.push("hflip".into());
    }
    if options.flip_v {
        f.push("vflip".into());
    }
    f
}

fn video_codec_params(
    target: &str,
    encoder: &str,
    options: &ConvOptions,
) -> (Vec<OsString>, Vec<OsString>) {
    let crf = options.crf.unwrap_or(match options.quality {
        crate::job::Quality::High => 19,
        crate::job::Quality::Balanced => 23,
        crate::job::Quality::Small => 28,
    });
    let bitrate = options
        .video_bitrate_kb
        .filter(|b| *b > 0)
        .map(|b| format!("{}k", b));

    let filters = video_filters(options);
    let vf = if filters.is_empty() {
        None
    } else {
        Some(filters.join(","))
    };

    // webm / vp9 / av1 走自己的参数体系
    if target == "webm" || encoder.contains("vpx") || encoder.contains("aom") {
        let mut v: Vec<OsString> = vec!["-c:v".into(), encoder.into()];
        if let Some(br) = &bitrate {
            // 显式指定了码率（如「保持源参数」）时按码率编码
            v.extend(["-b:v".into(), br.clone().into()]);
        } else {
            let crf_v = match options.quality {
                crate::job::Quality::High => 28,
                crate::job::Quality::Balanced => 34,
                crate::job::Quality::Small => 40,
            };
            v.extend([
                "-crf".into(),
                crf_v.to_string().into(),
                "-b:v".into(),
                "0".into(),
            ]);
        }
        let extra = vec!["-row-mt".into(), "1".into(), "-cpu-used".into(), "4".into()];
        if let Some(f) = vf {
            v.push("-vf".into());
            v.push(f.into());
        }
        return (v, extra);
    }

    if target == "avi" {
        return (vec!["-c:v".into(), "mpeg4".into()], vec![]);
    }

    let mut v = vec!["-c:v".into(), encoder.into()];
    if let Some(br) = &bitrate {
        v.extend(["-b:v".into(), br.into()]);
    } else {
        v.extend(["-crf".into(), crf.to_string().into()]);
    }
    if let Some(f) = vf {
        v.push("-vf".into());
        v.push(f.into());
    }

    // 部分硬件编码器需要显式像素格式。
    // 注意：显式指定了码率（如「保持源参数」）时，不能用 CQ/QP/global_quality
    // 再覆盖一次，否则码率匹配会失效。
    if encoder.contains("nvenc") {
        if bitrate.is_some() {
            v.extend(["-preset".into(), "p5".into(), "-rc".into(), "vbr".into()]);
        } else {
            v.extend([
                "-preset".into(),
                "p5".into(),
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                crf.to_string().into(),
                "-b:v".into(),
                "0".into(),
            ]);
        }
    } else if encoder.contains("qsv") {
        if bitrate.is_none() {
            v.extend(["-global_quality".into(), crf.to_string().into()]);
        }
    } else if encoder.contains("amf") {
        if bitrate.is_some() {
            v.extend(["-rc".into(), "cbr".into()]);
        } else {
            v.extend([
                "-rc".into(),
                "cqp".into(),
                "-qp_i".into(),
                crf.to_string().into(),
                "-qp_p".into(),
                crf.to_string().into(),
            ]);
        }
    } else if encoder.contains("videotoolbox") {
        let br = bitrate.clone().unwrap_or_else(|| match options.quality {
            crate::job::Quality::High => "8M".into(),
            crate::job::Quality::Balanced => "4M".into(),
            crate::job::Quality::Small => "2M".into(),
        });
        v.push("-b:v".into());
        v.push(br.into());
    } else if encoder.contains("libx") {
        // 软件编码器支持速度档：ultrafast…veryslow
        let preset = options.speed.as_str();
        let preset = if preset.is_empty() { "medium" } else { preset };
        v.extend([
            "-preset".into(),
            preset.into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ]);
    } else if encoder == "mpeg4" {
        v.extend(["-pix_fmt".into(), "yuv420p".into()]);
    }

    (v, vec![])
}

fn audio_args(options: &ConvOptions, target: &str) -> Vec<OsString> {
    let codec = options.audio_codec.as_deref().unwrap_or("auto");
    if codec == "none" {
        return vec!["-vn".into(), "-an".into()];
    }
    let mut a = vec!["-vn".into()];
    a.extend(audio_codec_args(options, target));
    a
}

/// 仅音频流参数（用于视频封装里的音轨）。
fn audio_stream_args(options: &ConvOptions, target: &str) -> Vec<OsString> {
    audio_codec_args(options, target)
}

/// 目标容器允许的默认音频编码。
///
/// 之前这里对 "auto" 直接回落到 `copy`，导致 mp3 容器里塞进 AAC 源轨时
/// ffmpeg 直接报 "Exactly one MP3 audio stream is required"。
fn default_audio_encoder(target: &str) -> &'static str {
    match target {
        "mp3" | "avi" => "libmp3lame",
        "wav" => "pcm_s16le",
        "flac" => "flac",
        "ogg" | "oga" => "libvorbis",
        "opus" => "libopus",
        "wma" => "wmav2",
        "webm" => "libopus",
        "aac" | "m4a" | "m4b" | "mp4" | "m4v" | "mov" | "mkv" | "ts" | "flv" | "wmv" | "3gp" => {
            "aac"
        }
        _ => "aac",
    }
}

fn audio_codec_args(options: &ConvOptions, target: &str) -> Vec<OsString> {
    let codec = options.audio_codec.as_deref().unwrap_or("auto");
    let lib = match codec {
        "mp3" => "libmp3lame",
        "aac" => "aac",
        "wav" => "pcm_s16le",
        "flac" => "flac",
        "ogg" => "libvorbis",
        "opus" => "libopus",
        "wma" => "wmav2",
        "auto" => default_audio_encoder(target),
        _ => "copy",
    };

    let mut a: Vec<OsString> = vec!["-c:a".into(), lib.into()];
    if lib == "copy" || lib.starts_with("pcm") || lib == "flac" {
        // 无损 / 拷贝不需要码率
    } else if let Some(kb) = options.audio_bitrate_kb {
        // Some(kb) = 用户指定的目标码率
        a.extend(["-b:a".into(), format!("{}k", kb).into()]);
    }
    // None = 保持原码率：不显式指定 -b:a，由编码器按源流就近输出
    if let Some(sr) = options.sample_rate {
        a.extend(["-ar".into(), sr.to_string().into()]);
    }
    if let Some(ch) = options.channels {
        a.extend(["-ac".into(), ch.to_string().into()]);
    }
    // 响度归一优先；否则按用户的音量增益调整
    if options.normalize {
        a.extend(["-af".into(), "loudnorm".into()]);
    } else if options.volume_db != 0 {
        a.extend([
            "-af".into(),
            format!("volume={}dB", options.volume_db).into(),
        ]);
    }
    a
}

fn gif_args(source_width: Option<u32>, options: &ConvOptions) -> Vec<OsString> {
    let w = match source_width {
        Some(x) if x > 720 => 720,
        Some(x) if x > 0 => x,
        _ => 720,
    };
    // GIF 也要支持旋转 / 翻转：把画面滤镜追加到调色板滤镜之前
    let mut chain: Vec<String> = Vec::new();
    match options.rotation.as_str() {
        "90" => chain.push("transpose=1".into()),
        "180" => {
            chain.push("transpose=1".into());
            chain.push("transpose=1".into());
        }
        "270" => chain.push("transpose=2".into()),
        _ => {}
    }
    if options.flip_h {
        chain.push("hflip".into());
    }
    if options.flip_v {
        chain.push("vflip".into());
    }
    chain.push(format!(
        "fps=12,scale={}:-2:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=3",
        w
    ));
    vec![
        "-vf".into(),
        chain.join(",").into(),
        "-loop".into(),
        "0".into(),
        "-an".into(),
    ]
}

pub fn ffprobe_width(ffprobe: &Path, input: &Path) -> Option<u32> {
    let out = crate::spawn::silent(std::process::Command::new(ffprobe))
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[allow(dead_code)]
pub fn default_output(input: &Path, target: &str) -> PathBuf {
    input.with_extension(target)
}
