//! 文件导入：把拖入 / 选中的路径展开成一批可转换的文件。
//!
//! 支持直接拖入文件夹——递归收集其中受支持的文件，并记下相对根目录的
//! 子目录层级，供「保持目录结构」输出时还原。
//!
//! 防御性设计：
//! - 符号链接一律跳过（避免目录环导致无限递归）
//! - 隐藏文件 / 系统隐藏项跳过（`.DS_Store`、`$RECYCLE.BIN` 之类）
//! - 深度与文件数双上限（误拖整个磁盘不会把内存和队列打爆）

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 单次导入的文件数上限。
pub const MAX_FILES: usize = 3000;
/// 递归深度上限。
pub const MAX_DEPTH: usize = 20;

#[derive(Debug, Default, Clone)]
pub struct Scanned {
    /// (绝对路径, 相对根目录的子目录；文件直接位于根目录时为空串)
    pub files: Vec<(PathBuf, String)>,
    /// 展开的目录数（含子目录）
    pub dirs: usize,
    /// 扩展名不受支持而跳过的文件数
    pub unsupported: usize,
    /// 隐藏 / 系统项跳过数
    pub hidden: usize,
    /// 是否因为达到 `MAX_FILES` 上限而中断
    pub truncated: bool,
}

/// 展开一批路径（文件或文件夹），返回可转换的文件清单。
///
/// `supported` 决定哪些扩展名值得收进来（通常是格式矩阵的键）。
pub fn scan(roots: &[String], supported: fn(&str) -> bool) -> Scanned {
    let mut out = Scanned::default();
    let mut seen: HashSet<String> = HashSet::new();

    for root in roots {
        let path = PathBuf::from(root);
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        // 顶层就是符号链接：不跟进，避免环
        if meta.file_type().is_symlink() {
            out.hidden += 1;
            continue;
        }
        if meta.is_file() {
            consider(&path, String::new(), supported, &mut out, &mut seen);
        } else if meta.is_dir() {
            out.dirs += 1;
            walk(&path, "", 0, supported, &mut out, &mut seen);
        }
        if out.truncated {
            break;
        }
    }

    // 目录遍历顺序依赖文件系统，统一排序让队列顺序稳定可预期
    out.files.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn walk(
    dir: &Path,
    rel: &str,
    depth: usize,
    supported: fn(&str) -> bool,
    out: &mut Scanned,
    seen: &mut HashSet<String>,
) {
    if out.truncated || depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut subdirs: Vec<(PathBuf, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if is_hidden(&name, &meta) || is_junk_dir(&name) {
            out.hidden += 1;
            continue;
        }
        let ft = meta.file_type();
        if ft.is_symlink() {
            out.hidden += 1;
        } else if ft.is_dir() {
            out.dirs += 1;
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            subdirs.push((path, child));
        } else if ft.is_file() {
            consider(&path, rel.to_string(), supported, out, seen);
            if out.truncated {
                return;
            }
        }
    }

    for (path, child) in subdirs {
        walk(&path, &child, depth + 1, supported, out, seen);
    }
}

fn consider(
    path: &Path,
    rel: String,
    supported: fn(&str) -> bool,
    out: &mut Scanned,
    seen: &mut HashSet<String>,
) {
    if out.files.len() >= MAX_FILES {
        out.truncated = true;
        return;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !supported(&ext) {
        out.unsupported += 1;
        return;
    }
    // 同一文件被多个根目录覆盖时只收一次
    let key = path.to_string_lossy().to_lowercase();
    if !seen.insert(key) {
        return;
    }
    out.files.push((path.to_path_buf(), rel));
}

fn is_hidden(name: &str, meta: &std::fs::Metadata) -> bool {
    name.starts_with('.') || has_hidden_attr(meta)
}

#[cfg(windows)]
fn has_hidden_attr(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // FILE_ATTRIBUTE_HIDDEN (0x2) | FILE_ATTRIBUTE_SYSTEM (0x4)
    meta.file_attributes() & 0x2 != 0 || meta.file_attributes() & 0x4 != 0
}

#[cfg(not(windows))]
fn has_hidden_attr(_meta: &std::fs::Metadata) -> bool {
    false
}

/// 回收站 / 卷影信息这类目录既没权限也没内容，直接跳过。
fn is_junk_dir(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "$recycle.bin" | "system volume information" | "lost+found"
    )
}
