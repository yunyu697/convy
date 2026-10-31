//! 文档引擎：LibreOffice（headless）与 Pandoc。
//!
//! 关键坑：LibreOffice 多个 headless 进程共用同一个用户配置目录会直接失败。
//! 每个任务必须用 `-env:UserInstallation=` 指向独立 profile 目录，
//! 且路径必须转成 file:// URL（含空格/中文时要做百分号编码）。

use super::{Plan, ProgressMode};
use crate::job::Job;
use crate::probe::Engines;
use crate::settings::ConvOptions;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn build_soffice(
    engines: &Engines,
    job: &Job,
    input: &Path,
    _output: &Path,
    from: &str,
    to: &str,
    _options: &ConvOptions,
) -> Result<Plan, String> {
    let program = engines
        .soffice
        .clone()
        .ok_or_else(|| "未找到 LibreOffice，无法转换该文档格式".to_string())?;

    // 输出到独立临时目录再搬运，避免 LibreOffice 自行决定文件名
    let tmp = std::env::temp_dir().join(format!("convy-lo-{}", &job.id[..8]));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败：{}", e))?;

    let profile = tmp.join("profile");
    let stem = Path::new(&job.source)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    let produced = tmp.join(format!("{}.{}", stem, to));

    let convert_to = if to == "csv" {
        "csv:\"Text - txt - csv (StarCalc)\":44,34,76".to_string()
    } else if to == "docx" {
        "docx:\"MS Word 2007 XML\"".to_string()
    } else if to == "pdf" {
        // 表格/演示用对应导出器，纯文本用 writer
        if matches!(from, "xls" | "xlsx" | "ods" | "csv" | "tsv") {
            "pdf:calc_pdf_Export".to_string()
        } else if matches!(from, "ppt" | "pptx" | "odp") {
            "pdf:impress_pdf_Export".to_string()
        } else {
            "pdf:writer_pdf_Export".to_string()
        }
    } else {
        to.to_string()
    };

    let args: Vec<OsString> = vec![
        "--headless".into(),
        "--norestore".into(),
        "--invisible".into(),
        "--nologo".into(),
        "--nodefault".into(),
        "--nolockcheck".into(),
        format!("-env:UserInstallation={}", to_file_url(&profile)).into(),
        "--convert-to".into(),
        convert_to.into(),
        "--outdir".into(),
        tmp.as_os_str().to_owned(),
        input.as_os_str().to_owned(),
    ];

    Ok(Plan {
        program,
        args,
        progress: ProgressMode::Simple,
        work_dir: Some(tmp.clone()),
        cleanup: vec![tmp],
        produced: Some(produced),
        envs: vec![
            ("HOME".into(), profile.to_string_lossy().to_string()),
            (
                "TMPDIR".into(),
                std::env::temp_dir().to_string_lossy().to_string(),
            ),
        ],
        hardware: false,
    })
}

pub fn build_pandoc(
    engines: &Engines,
    input: &Path,
    output: &Path,
    from: &str,
    to: &str,
    options: &ConvOptions,
) -> Result<Plan, String> {
    let program = engines
        .pandoc
        .clone()
        .ok_or_else(|| "未找到 Pandoc".to_string())?;

    let fmt = match from {
        "md" | "markdown" => "markdown+smart",
        "html" | "htm" => "html",
        other => other,
    };

    let mut args: Vec<OsString> = vec![
        input.as_os_str().to_owned(),
        "-f".into(),
        fmt.into(),
        "-o".into(),
        output.as_os_str().to_owned(),
        "--standalone".into(),
        "--resource-path".into(),
        input
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .as_os_str()
            .to_owned(),
    ];

    // PDF 需要外部排版引擎，WeasyPrint 比 LaTeX 轻量得多（用户可在设置里切换）
    if to == "pdf" {
        let engine = if options.pdf_engine.is_empty() {
            "weasyprint".to_string()
        } else {
            options.pdf_engine.clone()
        };
        args.extend([format!("--pdf-engine={}", engine).into()]);
    }

    // 自动生成目录（docx / pdf / epub 支持）
    if options.doc_toc && matches!(to, "docx" | "pdf" | "epub") {
        args.push("--toc".into());
    }

    Ok(Plan {
        program,
        args,
        progress: ProgressMode::Simple,
        work_dir: None,
        cleanup: vec![],
        produced: None,
        envs: vec![],
        hardware: false,
    })
}

/// 把本地路径转成 LibreOffice 能识别的 file:// URL，含空格/中文做百分号编码。
fn to_file_url(p: &Path) -> String {
    let raw = p.to_string_lossy().replace('\\', "/");
    let abs = if raw.starts_with('/') {
        raw
    } else {
        format!("/{}", raw)
    };

    let mut out = String::from("file://");
    for b in abs.as_bytes() {
        match b {
            b' ' => out.push_str("%20"),
            b if *b < 0x21 || *b > 0x7e => out.push_str(&format!("%{:02X}", b)),
            b => out.push(*b as char),
        }
    }
    out
}

#[allow(dead_code)]
pub fn default_output(input: &Path, target: &str) -> PathBuf {
    input.with_extension(target)
}
