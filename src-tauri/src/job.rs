//! 任务模型。所有字段都会序列化后推给前端。

use crate::formats::{domain_of, ext_of, Domain};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    #[default]
    Balanced,
    Small,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub source: String,
    pub file_name: String,
    pub size: u64,
    pub domain: Option<Domain>,
    pub source_ext: String,
    pub target_format: String,
    /// 导入时相对根目录的子目录（如 "photos/2024"）。
    /// 仅在「保持目录结构」且设置了输出目录时用于还原层级。
    #[serde(default)]
    pub rel_dir: String,
    pub status: JobStatus,
    pub progress: f32,
    /// 运行中的附加信息，如 "2.4x" 或 "第 3/12 页"
    pub detail: Option<String>,
    pub output_path: Option<String>,
    /// 转换完成后的文件大小，用于与源文件对比
    pub output_size: Option<u64>,
    pub error: Option<String>,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
}

impl Job {
    pub fn new(source: PathBuf, target_format: String) -> Self {
        let file_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let source_ext = ext_of(source.to_string_lossy().as_ref());
        let size = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
        Job {
            id: Uuid::new_v4().to_string(),
            source: source.to_string_lossy().to_string(),
            file_name,
            size,
            domain: domain_of(&source_ext),
            source_ext,
            target_format,
            rel_dir: String::new(),
            status: JobStatus::Pending,
            progress: 0.0,
            detail: None,
            output_path: None,
            output_size: None,
            error: None,
            started_at: None,
            finished_at: None,
        }
    }

    /// 记录导入时的相对子目录（文件夹批量导入用）
    pub fn in_subdir(mut self, rel: String) -> Self {
        self.rel_dir = rel;
        self
    }

    /// 输出路径：默认与源文件同目录，同名换扩展名；若设置了全局输出目录则落到该目录。
    ///
    /// `preserve_tree` 为真且该任务来自文件夹批量导入时，会在输出目录下
    /// 还原原有的子目录层级，避免几百个同名不同目录的文件全挤在一层。
    /// 重名时追加 (1)(2)…
    pub fn resolve_output(
        &self,
        overwrite: bool,
        output_folder: &str,
        preserve_tree: bool,
    ) -> PathBuf {
        let src = Path::new(&self.source);
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();

        let dir: PathBuf = if !output_folder.is_empty() {
            let base = PathBuf::from(output_folder);
            if preserve_tree && !self.rel_dir.is_empty() {
                base.join(safe_rel_dir(&self.rel_dir))
            } else {
                base
            }
        } else {
            src.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
        };
        let _ = std::fs::create_dir_all(&dir);

        let candidate = dir.join(format!("{}.{}", stem, self.target_format));

        // 同格式转换时候选路径可能与源文件相同——绝不能把源文件当输出覆盖掉
        let same_as_source = paths_eq(&candidate, src);
        if !same_as_source && (overwrite || !candidate.exists()) {
            return candidate;
        }
        for n in 1..1000 {
            let p = dir.join(format!("{} ({}).{}", stem, n, self.target_format));
            if !p.exists() && !paths_eq(&p, src) {
                return p;
            }
        }
        dir.join(format!("{}-{}.{}", stem, &self.id[..8], self.target_format))
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 宽松的路径相等判断：Windows 文件系统不区分大小写，其余平台按原文比较。
fn paths_eq(a: &Path, b: &Path) -> bool {
    let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
    if cfg!(windows) {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

/// 把相对子目录拼成安全路径：只保留普通片段，丢弃 `..` / 根 / 前缀，
/// 防止扫描结果里出现越出输出目录的路径（防患于未然）。
fn safe_rel_dir(rel: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        out.push(part);
    }
    out
}
