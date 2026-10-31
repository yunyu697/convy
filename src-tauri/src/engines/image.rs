//! 图片引擎。
//!
//! 主用 ImageMagick：格式覆盖最广（PSD / RAW / ICO 多尺寸 / HEIC），且跨平台行为一致。
//! libvips 作为回退，在 magick 缺失时接管常见格式；再不行用 FFmpeg 兜底常见格式。
//! 精细化参数来自 `ConvOptions`：缩放、各格式质量、是否清除元数据。

use super::{Plan, ProgressMode};
use crate::probe::Engines;
use crate::settings::ConvOptions;
use std::ffi::OsString;
use std::path::Path;

pub fn build(
    engines: &Engines,
    input: &Path,
    output: &Path,
    target: &str,
    options: &ConvOptions,
) -> Result<Plan, String> {
    if let Some(magick) = engines.magick.clone() {
        return Ok(build_magick(&magick, input, output, target, options));
    }
    if let Some(vips) = engines.vips.clone() {
        return build_vips(&vips, input, output, target, options);
    }
    if let Some(ffmpeg) = engines.ffmpeg.clone() {
        return build_ffmpeg(&ffmpeg, input, output, target, options);
    }
    Err("未找到 ImageMagick / libvips / FFmpeg，无法转换图片".into())
}

/// 根据宽高选项构造缩放滤镜参数（保持比例）。
fn resize_args(options: &ConvOptions) -> Vec<OsString> {
    let mut args = Vec::new();
    match (options.width, options.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            args.extend(["-resize".into(), format!("{}x{}!", w, h).into()]);
        }
        (Some(w), _) if w > 0 => {
            if options.keep_aspect {
                args.extend(["-resize".into(), format!("{}", w).into()]);
            } else {
                args.extend(["-resize".into(), format!("{}x", w).into()]);
            }
        }
        (_, Some(h)) if h > 0 => {
            args.extend(["-resize".into(), format!("x{}", h).into()]);
        }
        _ => {}
    }
    args
}

/// 图片的旋转 / 翻转滤镜（FFmpeg 兜底路径用）。
fn ffmpeg_picture_filters(options: &ConvOptions) -> Vec<String> {
    let mut f = Vec::new();
    match options.rotation.as_str() {
        "90" => f.push("transpose=1".into()),
        "180" => f.push("transpose=1,transpose=1".into()),
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

fn build_ffmpeg(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    target: &str,
    options: &ConvOptions,
) -> Result<Plan, String> {
    const SUPPORTED: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tiff", "gif", "avif"];
    if !SUPPORTED.contains(&target) {
        return Err(format!(
            "输出 {} 需要 ImageMagick，当前环境未安装。可在「引擎」页安装 ImageMagick 后重试。",
            target.to_uppercase()
        ));
    }

    let q = match target {
        "jpg" | "jpeg" => options.jpg_quality.unwrap_or(85),
        "webp" => options.webp_quality.unwrap_or(82),
        _ => 5,
    };

    let mut a: Vec<OsString> = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        input.as_os_str().to_owned(),
    ];
    if matches!(target, "jpg" | "jpeg" | "webp") {
        a.extend(["-q:v".into(), q.to_string().into()]);
    }
    // 缩放 + 旋转 + 翻转合成一条滤镜链
    let mut filters = ffmpeg_picture_filters(options);
    if let Some(w) = options.width.filter(|w| *w > 0) {
        let h = options
            .height
            .filter(|h| *h > 0)
            .map(|h| h.to_string())
            .unwrap_or_else(|| "-1".into());
        filters.insert(0, format!("scale={}:{}", w, h));
    }
    if !filters.is_empty() {
        a.extend(["-vf".into(), filters.join(",").into()]);
    }
    if options.strip {
        a.extend(["-map_metadata".into(), "-1".into()]);
    }
    a.push(output.as_os_str().to_owned());

    Ok(Plan {
        program: ffmpeg.to_path_buf(),
        args: a,
        progress: ProgressMode::Simple,
        work_dir: None,
        cleanup: vec![],
        produced: None,
        envs: vec![],
        hardware: false,
    })
}

fn build_magick(
    magick: &Path,
    input: &Path,
    output: &Path,
    target: &str,
    options: &ConvOptions,
) -> Plan {
    let mut a: Vec<OsString> = vec![input.as_os_str().to_owned()];
    a.extend(resize_args(options));
    if options.strip {
        a.push("-strip".into());
    }

    // 旋转（顺时针）与翻转
    match options.rotation.as_str() {
        "90" => a.extend(["-rotate".into(), "90".into()]),
        "180" => a.extend(["-rotate".into(), "180".into()]),
        "270" => a.extend(["-rotate".into(), "270".into()]),
        _ => {}
    }
    if options.flip_h {
        a.push("-flop".into());
    }
    if options.flip_v {
        a.push("-flip".into());
    }

    // JPG 不支持透明通道：按背景色拍平，避免黑底
    if matches!(target, "jpg" | "jpeg") {
        let bg = if options.img_background.is_empty() {
            "white"
        } else {
            options.img_background.as_str()
        };
        a.extend([
            "-background".into(),
            bg.into(),
            "-alpha".into(),
            "remove".into(),
            "-alpha".into(),
            "off".into(),
        ]);
    }

    match target {
        "ico" => {
            a.extend([
                "-define".into(),
                "icon:auto-resize=256,128,64,48,32,16".into(),
            ]);
        }
        "jpg" | "jpeg" => {
            a.extend([
                "-quality".into(),
                options.jpg_quality.unwrap_or(85).to_string().into(),
                "-interlace".into(),
                "Plane".into(),
            ]);
        }
        "webp" => {
            a.extend([
                "-quality".into(),
                options.webp_quality.unwrap_or(82).to_string().into(),
                "-define".into(),
                "webp:method=6".into(),
            ]);
        }
        "png" => {
            a.extend([
                "-define".into(),
                format!("png:compression-level={}", options.png_level.unwrap_or(9)).into(),
                "-define".into(),
                "png:compression-filter=5".into(),
            ]);
        }
        "avif" => {
            a.extend([
                "-quality".into(),
                options.avif_quality.unwrap_or(55).to_string().into(),
            ]);
        }
        "tiff" => {
            a.extend(["-compress".into(), "lzw".into()]);
        }
        "pdf" => {
            a.extend([
                "-quality".into(),
                options.jpg_quality.unwrap_or(85).to_string().into(),
                "-density".into(),
                "150".into(),
            ]);
        }
        _ => {}
    }

    a.push(output.as_os_str().to_owned());

    Plan {
        program: magick.to_path_buf(),
        args: a,
        progress: ProgressMode::Simple,
        work_dir: None,
        cleanup: vec![],
        produced: None,
        envs: vec![],
        hardware: false,
    }
}

fn build_vips(
    vips: &Path,
    input: &Path,
    output: &Path,
    target: &str,
    options: &ConvOptions,
) -> Result<Plan, String> {
    const VIPS_OUT: &[&str] = &["jpg", "png", "webp", "tiff", "avif", "gif"];
    if !VIPS_OUT.contains(&target) {
        return Err(format!("libvips 不支持输出 {}", target));
    }

    let q = match target {
        "jpg" | "webp" => format!(
            "{}[Q={}]",
            output.display(),
            options.jpg_quality.unwrap_or(85)
        ),
        _ => output.display().to_string(),
    };

    let mut args = vec![
        "copy".into(),
        input.as_os_str().to_owned(),
        OsString::from(q),
    ];
    if let Some(w) = options.width.filter(|w| *w > 0) {
        args.push("--size".into());
        let h = options.height.filter(|h| *h > 0).unwrap_or(0);
        args.push(format!("{}x{}", w, h).into());
    }

    Ok(Plan {
        program: vips.to_path_buf(),
        args,
        progress: ProgressMode::Simple,
        work_dir: None,
        cleanup: vec![],
        produced: None,
        envs: vec![],
        hardware: false,
    })
}
