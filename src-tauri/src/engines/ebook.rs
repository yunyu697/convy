//! 电子书引擎：Calibre 的 ebook-convert。
//!
//! 只以独立进程调用，不做任何链接或修改 —— 这是 GPL-3.0 工具在
//! 闭源/开源应用中安全使用的边界。同时用独立配置目录避免与
//! 用户已安装的 Calibre 争抢配置锁。

use super::{Plan, ProgressMode};
use crate::job::Job;
use crate::probe::Engines;
use crate::settings::ConvOptions;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn build(
    engines: &Engines,
    job: &Job,
    input: &Path,
    output: &Path,
    options: &ConvOptions,
) -> Result<Plan, String> {
    let program = engines
        .ebook_convert
        .clone()
        .ok_or_else(|| "未找到 Calibre（ebook-convert），无法转换电子书".to_string())?;

    let work = std::env::temp_dir().join(format!("convy-calibre-{}", &job.id[..8]));
    std::fs::create_dir_all(&work).map_err(|e| format!("创建临时目录失败：{}", e))?;

    let mut args: Vec<OsString> = vec![input.as_os_str().to_owned(), output.as_os_str().to_owned()];

    // 输出设备画像
    let profile = match options.ebook_profile.as_str() {
        "kindle" => "kindle",
        "kindle_dx" => "kindle_dx",
        "tablet" => "tablet",
        "phone" => "phone",
        _ => "",
    };
    if !profile.is_empty() {
        args.extend(["--output-profile".into(), profile.into()]);
    }

    // 元数据覆盖：留空表示保留源数据
    if !options.ebook_title.is_empty() {
        args.extend(["--title".into(), options.ebook_title.clone().into()]);
    }
    if !options.ebook_author.is_empty() {
        args.extend(["--authors".into(), options.ebook_author.clone().into()]);
    }

    // 嵌入正文字体族（文件内需含该字体或依赖系统字体）
    if !options.ebook_font_family.is_empty() {
        args.extend([
            "--embed-font-family".into(),
            options.ebook_font_family.clone().into(),
        ]);
    }

    // 两端对齐（默认开，关闭即左对齐）
    if !options.ebook_justify {
        args.push("--disable-justify".into());
    }

    match job.target_format.as_str() {
        "pdf" => {
            args.extend([
                "--paper-size".into(),
                "a4".into(),
                "--pdf-page-numbers".into(),
                "--pdf-default-font-size".into(),
                options.base_font_size.to_string().into(),
                "--pdf-page-margin-bottom".into(),
                options.margin.to_string().into(),
                "--pdf-page-margin-top".into(),
                options.margin.to_string().into(),
            ]);
        }
        "epub" => {
            args.extend(["--epub-version".into(), "3".into()]);
        }
        "mobi" | "azw3" => {
            args.extend(["--output-profile".into(), "kindle".into()]);
        }
        _ => {}
    }

    // 目录
    if options.insert_toc {
        args.extend(["--use-auto-toc".into()]);
    }

    // 基础字体与行高
    args.extend([
        "--base-font-size".into(),
        options.base_font_size.to_string().into(),
        "--line-height".into(),
        format!("{:.2}", options.line_height).into(),
    ]);

    // 高压缩只对图片资源有意义，能显著缩小电子书体积
    if options.quality == crate::job::Quality::Small {
        args.extend([
            "--jpeg-quality".into(),
            options.ebook_jpeg_quality.min(60).to_string().into(),
        ]);
    } else {
        args.extend([
            "--jpeg-quality".into(),
            options.ebook_jpeg_quality.to_string().into(),
        ]);
    }

    Ok(Plan {
        program,
        args,
        progress: ProgressMode::Percent,
        work_dir: Some(work.clone()),
        cleanup: vec![work.clone()],
        produced: None,
        envs: vec![
            (
                "CALIBRE_CONFIG_DIRECTORY".into(),
                work.join("config").to_string_lossy().to_string(),
            ),
            (
                "CALIBRE_TEMP_DIR".into(),
                work.join("tmp").to_string_lossy().to_string(),
            ),
            // 无沙箱环境下 QtWebEngine 会直接崩溃
            ("QTWEBENGINE_DISABLE_SANDBOX".into(), "1".into()),
            ("CALIBRE_USE_DARK_PALETTE".into(), "0".into()),
        ],
        hardware: false,
    })
}

#[allow(dead_code)]
pub fn default_output(input: &Path, target: &str) -> PathBuf {
    input.with_extension(target)
}
