//! 引擎调度：把 (源格式, 目标格式) 映射到具体的转换方案。

pub mod document;
pub mod ebook;
pub mod ffmpeg;
pub mod image;
pub mod integrity;
pub mod layout;
pub mod manifest;
pub mod registry;

use crate::formats::{domain_of, Domain};
use crate::job::Job;
use crate::probe::Engines;
use crate::settings::ConvOptions;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// 一次转换的完整执行方案。runner 只认这个结构，不关心是哪个引擎。
#[derive(Debug, Clone)]
pub struct Plan {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub progress: ProgressMode,
    /// 需要独立配置目录时使用（LibreOffice / Calibre）
    pub work_dir: Option<PathBuf>,
    /// 执行结束后需要清理的临时目录
    pub cleanup: Vec<PathBuf>,
    /// 引擎实际产出的文件路径；若与目标路径不同，runner 负责移动
    pub produced: Option<PathBuf>,
    pub envs: Vec<(String, String)>,
    /// 该方案是否依赖硬件编码器（失败时可降级重试）
    pub hardware: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ProgressMode {
    /// 用 `-progress pipe:1` 的 out_time_us 除以总时长
    Ffmpeg { duration_us: u64 },
    /// 从输出里抓 `NN%`
    Percent,
    /// 无法细分，只能 0 -> 1
    Simple,
}

/// 决定用哪个引擎。`force_encoder` 用于硬件编码失败后的降级重试。
pub async fn plan_for(
    engines: &Engines,
    job: &Job,
    options: &ConvOptions,
    output: &Path,
    force_encoder: Option<&str>,
) -> Result<Plan, String> {
    let from = job.source_ext.as_str();
    let to = job.target_format.as_str();
    let input = Path::new(&job.source);
    let src_domain = domain_of(from);

    // 1. 视频 / 音频 -> FFmpeg
    if matches!(src_domain, Some(Domain::Video) | Some(Domain::Audio)) {
        let duration = match engines.ffprobe.as_ref() {
            Some(p) => crate::probe::probe_duration_us(p, input).await,
            None => None,
        };
        let width = if to == "gif" {
            engines
                .ffprobe
                .as_ref()
                .and_then(|p| ffmpeg::ffprobe_width(p, input))
        } else {
            None
        };

        // 保持源参数：探测源文件的码率/采样率/声道，覆盖面板默认值。
        // 只改编码、其余与原文件一致的场景全靠这一步。GIF 有自己的参数体系，跳过。
        let mut opts = options.clone();
        if opts.keep_source && to != "gif" {
            if let Some(p) = engines.ffprobe.as_ref() {
                let streams = crate::probe::probe_streams(p, input).await;
                crate::settings::apply_keep_source(&mut opts, &streams);
            }
        }

        let mut plan = ffmpeg::build(
            engines,
            input,
            output,
            to,
            &opts,
            duration,
            width,
            force_encoder,
        )?;
        plan.hardware = true;
        return Ok(plan);
    }

    // 2. 图片 -> ImageMagick / libvips
    if src_domain == Some(Domain::Image) {
        return image::build(engines, input, output, to, options);
    }

    // 3. 电子书域 -> Calibre
    if src_domain == Some(Domain::Ebook) {
        return ebook::build(engines, job, input, output, options);
    }

    // 4. 文档域：按目标格式细分
    if src_domain == Some(Domain::Document) {
        // PDF 源转电子书 -> Calibre 效果最好
        if from == "pdf" && matches!(to, "epub" | "mobi" | "azw3" | "fb2") {
            return ebook::build(engines, job, input, output, options);
        }
        // PDF 源转 docx/txt/html -> Calibre 的提取能力优于 LibreOffice
        if from == "pdf" && matches!(to, "docx" | "txt" | "html") {
            return ebook::build(engines, job, input, output, options);
        }
        // PDF 源转图片 -> ImageMagick（依赖 ghostscript）
        if from == "pdf" && matches!(to, "jpg" | "png") {
            return image::build(engines, input, output, to, options);
        }
        // Markdown / HTML 源：Pandoc 保真度最高
        if matches!(from, "md" | "markdown" | "html" | "htm")
            && matches!(to, "docx" | "epub" | "odt" | "rtf" | "txt" | "pdf")
            && engines.pandoc.is_some()
            && options.doc_via != "soffice"
        {
            return document::build_pandoc(engines, input, output, from, to, options);
        }
        // 其余 Office 格式 -> LibreOffice
        return document::build_soffice(engines, job, input, output, from, to, options);
    }

    Err(format!("不支持的源格式：{}", from))
}
