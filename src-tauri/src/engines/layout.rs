//! 托管引擎的目录布局：解压后结构归一化 + 可执行文件定位。
//!
//! 各家发行包的内部结构并不统一，直接按清单里的 `bin` 相对路径取文件很容易失败：
//!
//! ```text
//! gyan ffmpeg  → ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe   （多一层包装目录）
//! BtbN ffmpeg  → ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe  （同上）
//! 自制压缩包    → bin/ffmpeg.exe                                 （无包装目录）
//! ```
//!
//! 所以解压后先「展平」唯一的包装目录，再按 清单相对路径 → 安装时记录的
//! `.convy-bin` → 浅层同名搜索 的顺序定位可执行文件，最后把找到的真实相对路径
//! 记进 `.convy-bin`，供后续探测/回退直接复用。

use std::path::{Path, PathBuf};

/// 版本目录内记录实际可执行文件相对路径的文件
pub const BIN_HINT: &str = ".convy-bin";

/// 展平层数上限：正常发行包最多一层包装目录
const MAX_FLATTEN: usize = 3;
/// 兜底搜索深度上限
const MAX_SEARCH_DEPTH: usize = 4;

/// 若目录下只有一个子目录、没有任何文件，则把该子目录的内容上移一层。
/// 返回是否发生了展平。
pub fn flatten_single_root(root: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(root) else {
        return false;
    };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    if entries.len() != 1 {
        return false;
    }
    let only = &entries[0];
    if !only.is_dir() {
        return false;
    }
    let Ok(inner) = std::fs::read_dir(only) else {
        return false;
    };
    let children: Vec<PathBuf> = inner.flatten().map(|e| e.path()).collect();
    let mut moved_all = true;
    for child in &children {
        let Some(name) = child.file_name() else {
            continue;
        };
        let dest = root.join(name);
        if std::fs::rename(child, &dest).is_err() && copy_recursive(child, &dest).is_err() {
            moved_all = false;
        }
    }
    if moved_all {
        // 上移成功才删掉空壳目录；失败就保留，别弄丢文件
        let _ = std::fs::remove_dir_all(only);
    }
    moved_all
}

/// 反复展平包装目录，直到根下出现文件或无法继续。返回展平次数。
pub fn normalize_layout(root: &Path) -> usize {
    for i in 0..MAX_FLATTEN {
        if !flatten_single_root(root) {
            return i;
        }
    }
    MAX_FLATTEN
}

/// 定位可执行文件。
///
/// 1. `version_dir/expected_rel`（清单里写的相对路径）
/// 2. `.convy-bin` 里记录的历史路径
/// 3. 浅层递归找同名文件（最后兜底，容忍发行包改目录结构）
pub fn resolve_bin(version_dir: &Path, expected_rel: &str) -> Option<PathBuf> {
    if !version_dir.is_dir() {
        return None;
    }
    let direct = version_dir.join(expected_rel);
    if direct.is_file() {
        return Some(direct);
    }
    let name = Path::new(expected_rel)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())?;

    if let Ok(hint) = std::fs::read_to_string(version_dir.join(BIN_HINT)) {
        let rel = hint.trim();
        if !rel.is_empty() {
            let p = version_dir.join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    find_by_name(version_dir, &name, 0)
}

/// 记录实际可执行文件的相对路径，供下次直接命中。
pub fn write_bin_hint(version_dir: &Path, rel: &str) -> std::io::Result<()> {
    std::fs::write(
        version_dir.join(BIN_HINT),
        rel.trim_start_matches(['/', '\\']),
    )
}

/// 按相对路径裁剪用不到的组件（如 ffplay、doc、presets），返回实际删掉的条目。
pub fn prune(version_dir: &Path, entries: &[String]) -> Vec<String> {
    let mut removed = Vec::new();
    for rel in entries {
        let rel = rel.trim();
        if rel.is_empty() || rel == "." || rel.contains("..") {
            continue;
        }
        let p = version_dir.join(rel);
        let ok = if p.is_dir() {
            std::fs::remove_dir_all(&p).is_ok()
        } else if p.is_file() {
            std::fs::remove_file(&p).is_ok()
        } else {
            false
        };
        if ok {
            removed.push(rel.to_string());
        }
    }
    removed
}

fn find_by_name(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth > MAX_SEARCH_DEPTH {
        return None;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if path.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(path);
        }
    }
    // 同一层没找到再往下走，保证先浅后深
    dirs.sort();
    for d in dirs {
        if let Some(p) = find_by_name(&d, name, depth + 1) {
            return Some(p);
        }
    }
    None
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            let name = entry.file_name();
            copy_recursive(&entry.path(), &dest.join(name))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::copy(src, dest).map(|_| ())
    }
}
