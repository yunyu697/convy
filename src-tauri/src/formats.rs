//! 格式矩阵：定义每个输入扩展名所属的域，以及可选的输出格式。
//!
//! 这里是产品能力的唯一真相来源，UI 的下拉框和 Rust 侧的路由都读它。

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Video,
    Audio,
    Image,
    Ebook,
    Document,
}

const VIDEO_IN: &[&str] = &[
    "mp4", "mkv", "mov", "avi", "flv", "wmv", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts", "3gp",
    "ogv", "rmvb", "vob", "asf", "mts",
];

const AUDIO_IN: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "oga", "opus", "wma", "aiff", "aif", "ape", "amr",
    "ac3", "mka", "wv",
];

const IMAGE_IN: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "webp", "avif", "gif", "bmp", "tif", "tiff", "ico", "heic",
    "heif", "svg", "psd", "dng", "cr2", "nef", "arw", "tga", "ppm", "exr", "hdr",
];

const EBOOK_IN: &[&str] = &[
    "epub", "mobi", "azw", "azw3", "fb2", "lit", "pdb", "cbz", "cbr", "kfx", "ibooks", "htmlz",
];

const DOC_IN: &[&str] = &[
    "doc", "docx", "odt", "rtf", "txt", "html", "htm", "md", "markdown", "xls", "xlsx", "ods",
    "csv", "tsv", "ppt", "pptx", "odp", "pages", "numbers", "key", "wps", "et", "dps",
];

const VIDEO_OUT: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v", "gif", "ts"];
const AUDIO_OUT: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "wma"];
const IMAGE_OUT: &[&str] = &[
    "jpg", "png", "webp", "avif", "gif", "bmp", "tiff", "ico", "pdf",
];
const EBOOK_OUT: &[&str] = &["epub", "mobi", "azw3", "pdf", "fb2", "txt", "docx", "htmlz"];
const DOC_OUT: &[&str] = &[
    "pdf", "docx", "odt", "rtf", "txt", "html", "md", "epub", "csv",
];

/// 扩展名小写化，无扩展名返回空串。
pub fn ext_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn domain_of(ext: &str) -> Option<Domain> {
    let e = ext.to_ascii_lowercase();
    if VIDEO_IN.contains(&e.as_str()) {
        Some(Domain::Video)
    } else if AUDIO_IN.contains(&e.as_str()) {
        Some(Domain::Audio)
    } else if IMAGE_IN.contains(&e.as_str()) {
        Some(Domain::Image)
    } else if EBOOK_IN.contains(&e.as_str()) {
        Some(Domain::Ebook)
    } else if DOC_IN.contains(&e.as_str()) {
        Some(Domain::Document)
    } else if e == "pdf" {
        // PDF 同时属于文档与电子书，路由时按目标格式决定
        Some(Domain::Document)
    } else {
        None
    }
}

/// 给定源扩展名，返回可转换的目标格式列表。
///
/// 视频/音频/图片允许目标格式与源格式相同——用户可能只想改编码器、
/// 码率或压缩参数而不换容器；文档/电子书同格式没有转换意义，仍排除自身。
pub fn targets_for(ext: &str) -> Vec<&'static str> {
    let e = ext.to_ascii_lowercase();
    let mut out: Vec<&'static str> = match domain_of(&e) {
        Some(Domain::Video) => VIDEO_OUT.iter().chain(AUDIO_OUT).copied().collect(),
        Some(Domain::Audio) => AUDIO_OUT.to_vec(),
        Some(Domain::Image) => IMAGE_OUT.to_vec(),
        Some(Domain::Ebook) => EBOOK_OUT.to_vec(),
        Some(Domain::Document) => {
            if e == "pdf" {
                // PDF 作为源：提取文本/重排/转图
                vec!["docx", "txt", "html", "epub", "mobi", "jpg", "png"]
            } else if matches!(e.as_str(), "md" | "markdown" | "html" | "htm") {
                vec!["pdf", "docx", "epub", "txt", "odt", "rtf"]
            } else if matches!(e.as_str(), "xls" | "xlsx" | "ods" | "csv" | "tsv") {
                vec!["pdf", "csv", "xlsx", "ods", "html"]
            } else {
                DOC_OUT.to_vec()
            }
        }
        None => vec![],
    };
    let media = matches!(
        domain_of(&e),
        Some(Domain::Video | Domain::Audio | Domain::Image)
    );
    if !media {
        out.retain(|t| *t != e);
    }
    out.dedup();
    out
}

pub fn is_audio_target(ext: &str) -> bool {
    AUDIO_OUT.contains(&ext)
}
