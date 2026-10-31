//! 转换历史：把每次任务的终态追加到 `history.jsonl`。
//!
//! 为什么用 JSONL 而不是一个 JSON 数组：追加是 O(1)，不需要每次重写整个文件；
//! 中途断电最多损失最后一行，不会把历史全弄坏。
//!
//! 记录内容刻意保持「够用就好」：源文件、目标格式、结果、耗时、体积变化、错误摘要。
//! 不记录完整设置快照 —— 体积会长得很快，而「用相同参数重跑」可以通过
//! 重新入队 + 当前设置实现；真正需要复现的参数应该固化成预设。

use crate::formats::Domain;
use crate::job::JobStatus;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 历史文件保留的最大条数（超出后裁剪最旧的）
pub const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub source: String,
    pub file_name: String,
    pub domain: Option<Domain>,
    pub target_format: String,
    pub status: JobStatus,
    pub input_size: u64,
    pub output_size: Option<u64>,
    pub output_path: Option<String>,
    /// 转换耗时（毫秒）
    pub duration_ms: u64,
    pub error: Option<String>,
    pub finished_at: u64,
}

pub fn history_path(config_dir: &Path) -> PathBuf {
    config_dir.join("history.jsonl")
}

/// 追加一条记录；顺带在超过上限时把文件裁到最近 `MAX_ENTRIES` 条。
pub fn append(path: &Path, entry: &HistoryEntry) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    let ok = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{line}"))
        .is_ok();
    if ok {
        trim(path, MAX_ENTRIES);
    }
}

/// 读取历史，最新的在前。
pub fn load(path: &Path, limit: usize) -> Vec<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out: Vec<HistoryEntry> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<HistoryEntry>(l).ok())
        .collect();
    out.reverse();
    out.truncate(limit);
    out
}

pub fn clear(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("清空历史失败：{e}"))?;
    }
    Ok(())
}

/// 只保留最近 `keep` 条，避免文件无限增长。
fn trim(path: &Path, keep: usize) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= keep {
        return;
    }
    let tail = &lines[lines.len() - keep..];
    // 先写临时文件再改名，避免裁剪过程中崩溃把历史写坏
    let tmp = path.with_extension("jsonl.tmp");
    let body = format!("{}\n", tail.join("\n"));
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, status: JobStatus) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            source: format!("src-{id}.mp4"),
            file_name: "src.mp4".into(),
            domain: Some(Domain::Video),
            target_format: "mkv".into(),
            status,
            input_size: 1000,
            output_size: Some(800),
            output_path: Some("out.mkv".into()),
            duration_ms: 1200,
            error: None,
            finished_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn append_and_load_newest_first() {
        let dir = std::env::temp_dir().join("convy-history-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = history_path(&dir);

        append(&path, &entry("a", JobStatus::Done));
        append(&path, &entry("b", JobStatus::Failed));
        let got = load(&path, 10);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "b", "最新的应排在最前");
        assert_eq!(got[1].id, "a");

        assert_eq!(load(&path, 1).len(), 1);
        clear(&path).unwrap();
        assert!(load(&path, 10).is_empty());
    }

    #[test]
    fn history_is_trimmed_to_cap() {
        let dir = std::env::temp_dir().join("convy-history-trim");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = history_path(&dir);

        for i in 0..(MAX_ENTRIES + 20) {
            append(&path, &entry(&format!("{i}"), JobStatus::Done));
        }
        let all = load(&path, MAX_ENTRIES + 100);
        assert_eq!(all.len(), MAX_ENTRIES, "超出上限应被裁剪");
        assert_eq!(all[0].id, format!("{}", MAX_ENTRIES + 19));
    }
}
