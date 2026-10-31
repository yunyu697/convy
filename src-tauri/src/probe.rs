//! 引擎探测：定位外部可执行文件，并检测 FFmpeg 可用的硬件编码器。
//!
//! 查找优先级：随应用分发的 engines/ 目录 > 系统 PATH > 各平台常见安装路径。
//! 这样开发期直接用系统已装的工具，发布期把二进制丢进 engines/ 即可。

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineReport {
    pub ffmpeg: bool,
    pub ffprobe: bool,
    /// 图片引擎，取值为 "magick" / "vips" / null
    pub image: Option<String>,
    pub ebook: bool,
    pub document: bool,
    pub pandoc: bool,
    /// 当前将用于 H.264 的编码器（人类可读）
    pub h264_encoder: String,
    pub hardware_accel: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Engines {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub magick: Option<PathBuf>,
    pub vips: Option<PathBuf>,
    pub ebook_convert: Option<PathBuf>,
    pub soffice: Option<PathBuf>,
    pub pandoc: Option<PathBuf>,
    /// 按优先级排好的可用 H.264 编码器，最后一项总是软件回退
    pub h264_chain: Vec<String>,
    /// 按优先级排好的可用 HEVC 编码器，最后一项是 libx265（软件）
    pub hevc_chain: Vec<String>,
    /// 按优先级排好的可用 VPx 编码器（webm 容器专用），最后一项是 libvpx（VP8）
    pub vpx_chain: Vec<String>,
    /// libx264 是否可用（LGPL 构建下为 false）
    pub has_libx264: bool,
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) && !base.ends_with(".exe") {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

fn search_path(base: &str) -> Option<PathBuf> {
    let name = exe_name(base);
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(&name))
            .find(|p| p.is_file())
    })
}

fn search_dirs(dirs: &[PathBuf], base: &str) -> Option<PathBuf> {
    let name = exe_name(base);
    dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

/// 按 资源目录 -> PATH -> 常见路径 的顺序查找。
///
/// 随包分发的二进制直接信任；其余候选必须通过 `--version` 校验，
/// 否则会踩到 Windows 上 `convert.exe`（卷转换工具）这类同名陷阱。
fn locate(bundle_dir: &Path, base: &str, platform_dirs: &[&str], marker: &str) -> Option<PathBuf> {
    if let Some(p) = search_dirs(&[bundle_dir.to_path_buf()], base) {
        return Some(p);
    }
    if let Some(p) = search_path(base).filter(|p| verify(p, marker)) {
        return Some(p);
    }
    let dirs: Vec<PathBuf> = platform_dirs.iter().map(PathBuf::from).collect();
    search_dirs(&dirs, base).filter(|p| verify(p, marker))
}

/// 跑一次 `--version`，确认这个可执行文件确实是我们要的工具。
fn verify(p: &Path, marker: &str) -> bool {
    let out = crate::spawn::silent(std::process::Command::new(p))
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match out {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
            .to_lowercase();
            text.contains(marker)
        }
        Err(_) => false,
    }
}

fn soffice_dirs() -> Vec<&'static str> {
    if cfg!(windows) {
        vec![
            r"C:\Program Files\LibreOffice\program",
            r"C:\Program Files (x86)\LibreOffice\program",
        ]
    } else if cfg!(target_os = "macos") {
        vec!["/Applications/LibreOffice.app/Contents/MacOS"]
    } else {
        vec!["/usr/bin", "/usr/lib/libreoffice/program", "/snap/bin"]
    }
}

fn calibre_dirs() -> Vec<&'static str> {
    if cfg!(windows) {
        vec![
            r"C:\Program Files\Calibre2",
            r"C:\Program Files (x86)\Calibre2",
        ]
    } else if cfg!(target_os = "macos") {
        vec!["/Applications/calibre.app/Contents/MacOS"]
    } else {
        vec!["/usr/bin", "/opt/calibre", "/usr/local/bin"]
    }
}

fn ffmpeg_dirs() -> Vec<&'static str> {
    if cfg!(windows) {
        vec![r"C:\ffmpeg\bin", r"C:\Program Files\ffmpeg\bin"]
    } else if cfg!(target_os = "macos") {
        vec!["/opt/homebrew/bin", "/usr/local/bin"]
    } else {
        vec!["/usr/bin", "/usr/local/bin", "/snap/bin"]
    }
}

/// 在自动下载的托管目录里找当前启用的引擎可执行文件。
///
/// 路径规则：`<managed_root>/<id>/<active>/<bin>`，active 读自 active.txt。
/// 真实的相对路径由 `engines::layout` 解析（清单路径 → .convy-bin → 浅层搜索），
/// 因为发行包解压后常多一层包装目录，写死层级会找不到。
fn managed_bin(root: &Path, id: &str, bin: &str) -> Option<PathBuf> {
    let active_file = root.join(id).join("active.txt");
    let active = std::fs::read_to_string(&active_file)
        .ok()?
        .trim()
        .to_string();
    if active.is_empty() {
        return None;
    }
    crate::engines::layout::resolve_bin(&root.join(id).join(active), &exe_name(bin))
}

/// 试编码 2 帧测试画面，确认硬件编码器在本机真正可用。
///
/// 仅凭 `ffmpeg -encoders` 列表判断是不够的：大多数发行版 ffmpeg
/// 都编译了 nvenc/qsv/amf，但本机没有对应 GPU 时编码会直接失败，
/// 导致报告错误地显示「硬件加速」。这里真实跑一次微缩编码来验证。
fn hw_encoder_works(ffmpeg: &Path, encoder: &str) -> bool {
    let out = crate::spawn::silent(std::process::Command::new(ffmpeg))
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=64x64:d=0.2:r=10",
            "-frames:v",
            "2",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    matches!(out, Ok(o) if o.status.success())
}

impl Engines {
    /// `bundle_dir` 为随包分发的 engines/，`managed_root` 为应用数据目录下自动下载的引擎根。
    pub fn detect(bundle_dir: &Path, managed_root: &Path) -> Self {
        let ffmpeg = locate(bundle_dir, "ffmpeg", &ffmpeg_dirs(), "ffmpeg")
            .or_else(|| managed_bin(managed_root, "ffmpeg", "ffmpeg.exe"));
        let ffprobe = locate(bundle_dir, "ffprobe", &ffmpeg_dirs(), "ffprobe")
            .or_else(|| managed_bin(managed_root, "ffmpeg", "ffprobe.exe"));
        // Windows 的 system32 里有个 convert.exe（卷转换工具），只在类 Unix 上才认这个别名
        let magick = locate(bundle_dir, "magick", &[], "imagemagick")
            .or_else(|| managed_bin(managed_root, "imagemagick", "magick.exe"))
            .or_else(|| {
                if cfg!(windows) {
                    None
                } else {
                    locate(bundle_dir, "convert", &[], "imagemagick")
                }
            });
        let vips = locate(bundle_dir, "vips", &[], "vips");
        let ebook_convert = locate(bundle_dir, "ebook-convert", &calibre_dirs(), "calibre")
            .or_else(|| managed_bin(managed_root, "calibre", "ebook-convert.exe"));
        let soffice = locate(bundle_dir, "soffice", &soffice_dirs(), "libreoffice")
            .or_else(|| managed_bin(managed_root, "libreoffice", "soffice.exe"));
        let pandoc = locate(bundle_dir, "pandoc", &[], "pandoc")
            .or_else(|| managed_bin(managed_root, "pandoc", "pandoc.exe"));

        let mut eng = Engines {
            ffmpeg,
            ffprobe,
            magick,
            vips,
            ebook_convert,
            soffice,
            pandoc,
            h264_chain: vec!["libx264".into()],
            hevc_chain: vec!["libx265".into()],
            vpx_chain: vec!["libvpx-vp9".into()],
            has_libx264: true,
        };

        if let Some(ff) = eng.ffmpeg.clone() {
            let encoders = list_encoders(&ff);
            let has = |n: &str| encoders.iter().any(|e| e == n);

            eng.has_libx264 = has("libx264");

            // 硬件编码器按平台习惯排序，软件编码器永远兜底；
            // 硬件候选必须通过真机试编码验证，避免「有编译无 GPU」的误报
            let mut chain: Vec<String> = Vec::new();
            let candidates: &[&str] = if cfg!(target_os = "macos") {
                &["h264_videotoolbox"]
            } else if cfg!(windows) {
                &["h264_nvenc", "h264_qsv", "h264_amf", "h264_mf"]
            } else {
                &["h264_nvenc", "h264_qsv", "h264_vaapi"]
            };
            for c in candidates {
                if has(c) && hw_encoder_works(&ff, c) {
                    chain.push((*c).to_string());
                }
            }
            if eng.has_libx264 {
                chain.push("libx264".into());
            } else if has("libopenh264") {
                chain.push("libopenh264".into());
            } else if has("mpeg4") {
                chain.push("mpeg4".into());
            }
            eng.h264_chain = chain;

            // HEVC 链：硬件优先，libx265 软件兜底
            let mut hevc: Vec<String> = Vec::new();
            let hevc_cands: &[&str] = if cfg!(target_os = "macos") {
                &["hevc_videotoolbox"]
            } else if cfg!(windows) {
                &["hevc_nvenc", "hevc_qsv", "hevc_amf"]
            } else {
                &["hevc_nvenc", "hevc_qsv", "hevc_vaapi"]
            };
            for c in hevc_cands {
                if has(c) && hw_encoder_works(&ff, c) {
                    hevc.push((*c).to_string());
                }
            }
            if has("libx265") {
                hevc.push("libx265".into());
            }
            eng.hevc_chain = hevc;

            // VPx 链：webm 容器的软件编码器。VP9 优先，VP8 兜底。
            let mut vpx: Vec<String> = Vec::new();
            if has("libvpx-vp9") {
                vpx.push("libvpx-vp9".into());
            }
            if has("libvpx") {
                vpx.push("libvpx".into());
            }
            eng.vpx_chain = vpx;
        }

        eng
    }

    pub fn report(&self) -> EngineReport {
        let mut warnings = Vec::new();
        if self.ffmpeg.is_none() {
            warnings.push("未找到 FFmpeg，视频/音频转换不可用".into());
        }
        if self.ffprobe.is_none() {
            warnings.push("未找到 ffprobe，无法显示转码进度".into());
        }
        if self.magick.is_none() && self.vips.is_none() {
            if self.ffmpeg.is_some() {
                warnings
                    .push("未找到 ImageMagick，图片仅支持常见格式（ICO / PSD 等不可用）".into());
            } else {
                warnings.push("未找到图片转换引擎，图片转换不可用".into());
            }
        }
        if self.ebook_convert.is_none() {
            warnings.push("未找到 Calibre，电子书转换不可用".into());
        }
        if self.soffice.is_none() && self.pandoc.is_none() {
            warnings.push("未找到 LibreOffice / Pandoc，文档转换不可用".into());
        }

        let hw = self.h264_chain.iter().find(|e| {
            e.as_str() != "libx264" && e.as_str() != "mpeg4" && e.as_str() != "libopenh264"
        });

        let encoder_label = match hw.map(|s| s.as_str()) {
            Some("h264_nvenc") => "NVENC（NVIDIA 硬件）".to_string(),
            Some("h264_qsv") => "QSV（Intel 硬件）".to_string(),
            Some("h264_amf") => "AMF（AMD 硬件）".to_string(),
            Some("h264_mf") => "Media Foundation（硬件）".to_string(),
            Some("h264_videotoolbox") => "VideoToolbox（Apple 硬件）".to_string(),
            Some("h264_vaapi") => "VAAPI（Linux 硬件）".to_string(),
            Some(other) => other.to_string(),
            None => {
                if self.has_libx264 {
                    "libx264（软件）".to_string()
                } else {
                    "无可用 H.264 编码器".to_string()
                }
            }
        };

        EngineReport {
            ffmpeg: self.ffmpeg.is_some(),
            ffprobe: self.ffprobe.is_some(),
            image: if self.magick.is_some() {
                Some("magick".into())
            } else if self.vips.is_some() {
                Some("vips".into())
            } else if self.ffmpeg.is_some() {
                Some("ffmpeg".into())
            } else {
                None
            },
            ebook: self.ebook_convert.is_some(),
            document: self.soffice.is_some(),
            pandoc: self.pandoc.is_some(),
            h264_encoder: encoder_label,
            hardware_accel: hw.is_some(),
            warnings,
        }
    }
}

/// 解析 `ffmpeg -encoders`，返回编码器名列表。
fn list_encoders(ffmpeg: &Path) -> Vec<String> {
    let out = crate::spawn::silent(std::process::Command::new(ffmpeg))
        .args(["-hide_banner", "-loglevel", "quiet", "-encoders"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output();

    let Ok(out) = out else { return Vec::new() };
    let text = String::from_utf8_lossy(&out.stdout).to_string();

    text.lines()
        .filter(|l| l.starts_with(" V") || l.starts_with(" A") || l.starts_with(" S"))
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(|s| s.to_string())
        .collect()
}

/// 源文件的关键流参数，用于「保持源参数」模式：
/// 只改编码时，其余参数应与源文件一致，而不是套用面板默认值。
#[derive(Debug, Clone, Default)]
pub struct SourceStreams {
    /// 视频流码率（kbps）
    pub video_bitrate_kbps: Option<u32>,
    /// 音频流码率（kbps）
    pub audio_bitrate_kbps: Option<u32>,
    /// 音频采样率（Hz）
    pub sample_rate: Option<u32>,
    /// 音频声道数
    pub channels: Option<u8>,
}

/// 用 ffprobe 读取源文件的视频/音频流参数。
///
/// 视频流自带的 `bit_rate` 在部分容器里缺失（如某些 MP4 只写 format 级码率），
/// 此时用「容器总码率 − 音频码率」估算视频码率，避免退化成 CRF 重编码。
pub async fn probe_streams(ffprobe: &Path, input: &Path) -> SourceStreams {
    let out = crate::spawn::silent_tokio(tokio::process::Command::new(ffprobe))
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,bit_rate,sample_rate,channels:format=bit_rate",
            "-of",
            "json",
        ])
        .arg(input)
        .stdin(Stdio::null())
        .output()
        .await;

    let Ok(out) = out else {
        return SourceStreams::default();
    };
    let v: serde_json::Value = match serde_json::from_slice(&out.stdout) {
        Ok(v) => v,
        Err(_) => return SourceStreams::default(),
    };

    let mut s = SourceStreams::default();
    let mut format_kbps: Option<u32> = None;

    if let Some(fmt) = v.get("format") {
        format_kbps = parse_kbps(fmt.get("bit_rate"));
    }

    if let Some(streams) = v.get("streams").and_then(|x| x.as_array()) {
        for st in streams {
            match st.get("codec_type").and_then(|x| x.as_str()).unwrap_or("") {
                "video" => {
                    if s.video_bitrate_kbps.is_none() {
                        s.video_bitrate_kbps = parse_kbps(st.get("bit_rate"));
                    }
                }
                "audio" => {
                    if s.audio_bitrate_kbps.is_none() {
                        s.audio_bitrate_kbps = parse_kbps(st.get("bit_rate"));
                    }
                    if s.sample_rate.is_none() {
                        s.sample_rate = st
                            .get("sample_rate")
                            .and_then(|x| x.as_str())
                            .and_then(|x| x.parse::<u32>().ok())
                            .filter(|x| *x > 0);
                    }
                    if s.channels.is_none() {
                        s.channels = st
                            .get("channels")
                            .and_then(|x| x.as_u64())
                            .map(|c| c as u8)
                            .filter(|c| *c > 0);
                    }
                }
                _ => {}
            }
        }
    }

    // 视频流码率缺失时用容器总码率估算（减去音频码率）
    if s.video_bitrate_kbps.is_none() {
        if let Some(total) = format_kbps {
            let audio = s.audio_bitrate_kbps.unwrap_or(0);
            let est = total.saturating_sub(audio);
            if est > 0 {
                s.video_bitrate_kbps = Some(est);
            }
        }
    }

    s
}

/// ffprobe 的 bit_rate 是字符串形式的 bps，转成 kbps。
fn parse_kbps(v: Option<&serde_json::Value>) -> Option<u32> {
    v.and_then(|x| x.as_str())
        .and_then(|x| x.parse::<u64>().ok())
        .map(|bps| (bps / 1000) as u32)
        .filter(|kbps| *kbps > 0)
}

/// 用 ffprobe 读取媒体时长（微秒），失败返回 None。
pub async fn probe_duration_us(ffprobe: &Path, input: &Path) -> Option<u64> {
    let out = crate::spawn::silent_tokio(tokio::process::Command::new(ffprobe))
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;

    let s = String::from_utf8_lossy(&out.stdout);
    let secs: f64 = s.trim().parse().ok()?;
    if secs.is_finite() && secs > 0.0 {
        Some((secs * 1_000_000.0) as u64)
    } else {
        None
    }
}
