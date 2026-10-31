//! 把引擎的英文 stderr 翻译成人话。
//!
//! 为什么需要这一层：转换失败时界面上直接把 `tail(stderr, 600)` 丢给用户，
//! 而普通用户看到的是 `Exactly one MP3 audio stream is required` 这种东西 ——
//! 既不知道发生了什么，也不知道下一步该点哪里。这个项目面向的是"想把文件转个格式"的人，
//! 不是 ffmpeg 用户。
//!
//! 两条原则：
//! 1. **命中就换成中文说明 + 一句可执行的建议**，不解释原理
//! 2. **原文一律保留**（附在后面）—— 藏起来会让排错变成猜谜，也会让开发者失去线索

/// 一条匹配规则。`needle` 必须是小写，用 `contains` 匹配。
struct Rule {
    needle: &'static str,
    hint: &'static str,
}

/// 规则按「越具体越靠前」排。`conversion failed` 这类兜底话术放最后，
/// 否则会把更精确的匹配抢掉。
const RULES: &[Rule] = &[
    // ---------------------------------------------------------- FFmpeg 音视频
    Rule {
        needle: "exactly one mp3 audio stream is required",
        hint: "目标格式只支持单条音轨，但源文件里有多条。请指定保留哪一条音轨，\
               或改选支持多音轨的容器（如 mkv）",
    },
    Rule {
        needle: "does not contain any stream",
        hint: "源文件里没有可转换的内容。文件可能是空壳，或者扩展名与实际格式不符",
    },
    Rule {
        needle: "matches no streams",
        hint: "指定的音轨或字幕轨在源文件里不存在。换一个轨道编号，或让程序自动选择",
    },
    Rule {
        needle: "moov atom not found",
        hint: "MP4 缺少索引（moov），通常是录制或下载中断造成的。这个文件需要先修复才能转换",
    },
    Rule {
        needle: "invalid data found when processing input",
        hint: "文件可能已损坏，或者实际格式与扩展名不符。先用播放器打开确认能正常播放",
    },
    Rule {
        needle: "height not divisible by 2",
        hint: "H.264 要求高度是偶数。把分辨率改成偶数，或者开启等比缩放让程序自己算",
    },
    Rule {
        needle: "width not divisible by 2",
        hint: "H.264 要求宽度是偶数。把分辨率改成偶数，或者开启等比缩放让程序自己算",
    },
    Rule {
        needle: "could not find tag for codec",
        hint: "目标容器不支持所选的编码器。换一个容器（例如 mp4 → mkv），\
               或者把视频编码改回自动",
    },
    Rule {
        needle: "error while opening encoder",
        hint: "编码器初始化失败。多半是分辨率或码率组合不被硬件编码器接受，试试关闭硬件加速",
    },
    Rule {
        needle: "unknown encoder",
        hint: "当前引擎版本不支持这个编码器。到「引擎管理」把引擎更新到最新",
    },
    Rule {
        needle: "unknown decoder",
        hint: "当前引擎版本无法解码这个格式。更新引擎，或者换一个源格式",
    },
    Rule {
        needle: "subtitle codec",
        hint: "字幕编码不被目标容器支持。去掉字幕，或者改用 mkv 容器",
    },
    Rule {
        needle: "cannot allocate memory",
        hint: "内存不足。降低分辨率或并发数，并关掉其他占内存的程序",
    },
    // ---------------------------------------------------------- 环境类（跨引擎）
    Rule {
        needle: "no space left on device",
        hint: "磁盘空间不足。清理输出目录所在的磁盘，或者换一个输出位置",
    },
    Rule {
        needle: "permission denied",
        hint: "没有写入权限。换一个输出目录，并确认目标文件没有被其他程序占用",
    },
    // ---------------------------------------------------------- ImageMagick
    Rule {
        needle: "no decode delegate",
        hint: "这个图片格式需要额外的解码组件。先用 png 或 jpg 中转，\
               或者安装完整的 ImageMagick",
    },
    Rule {
        needle: "cache resources exhausted",
        hint: "ImageMagick 的内存或磁盘缓存达到上限。调小输出分辨率，\
               或者放宽 ImageMagick 的 policy 限制",
    },
    Rule {
        needle: "unable to open image",
        hint: "无法打开图片。检查文件是否损坏，以及路径里是否含有特殊字符",
    },
    // ---------------------------------------------------------- Calibre / 文档
    Rule {
        needle: "no suitable source format",
        hint: "Calibre 不认识这个源格式。先转成 epub 或 html，再往下处理",
    },
    Rule {
        needle: "source file could not be loaded",
        hint: "文档无法被 LibreOffice 打开。文件可能加密、损坏，\
               或者用了当前版本不支持的格式",
    },
    Rule {
        needle: "unknown input format",
        hint: "Pandoc 不认识这个输入格式。改走 LibreOffice 通道试试",
    },
    // ---------------------------------------------------------- 兜底
    Rule {
        needle: "conversion failed",
        hint: "转换过程中断。展开下面的原文看具体原因，常见的是源文件损坏或参数不被支持",
    },
];

/// 在 stderr 里找一条能解释的规则。大小写不敏感。
pub fn explain(stderr: &str) -> Option<&'static str> {
    if stderr.trim().is_empty() {
        return None;
    }
    let lower = stderr.to_ascii_lowercase();
    RULES
        .iter()
        .find(|r| lower.contains(r.needle))
        .map(|r| r.hint)
}

/// 取字符串末尾 `n` 个字符（按字符而非字节，避免切断中文）。
pub fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.trim().chars().collect();
    if chars.len() <= n {
        s.trim().to_string()
    } else {
        format!("…{}", chars[chars.len() - n..].iter().collect::<String>())
    }
}

/// 组装给用户看的失败原因：命中规则时「中文说明 + 引擎原文」，否则只有原文。
pub fn describe(stderr: &str, tail_chars: usize) -> String {
    let raw = tail(stderr, tail_chars);
    match explain(stderr) {
        Some(hint) => format!("{hint}\n引擎原文：{raw}"),
        None => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_common_ffmpeg_errors() {
        let s = "[mp3 @ 0x1] Exactly one MP3 audio stream is required.\n\
                 Could not write header for output file #0";
        let hint = explain(s).expect("应命中多音轨规则");
        assert!(hint.contains("单条音轨"));
        assert!(hint.contains("mkv"));
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(explain("NO SPACE LEFT ON DEVICE").is_some());
        assert!(explain("Permission Denied").is_some());
    }

    #[test]
    fn specific_rule_wins_over_generic() {
        // 同时包含 "conversion failed" 和更具体的 "moov atom not found" 时，
        // 必须给具体的那条 —— 否则用户拿到的永远是一句没用的「转换失败」
        let s = "moov atom not found\nConversion failed!";
        let hint = explain(s).unwrap();
        assert!(hint.contains("moov"), "具体规则应优先，实际：{hint}");
    }

    #[test]
    fn unknown_error_keeps_raw_text() {
        let s = "some totally unheard of engine explosion";
        assert!(explain(s).is_none());
        assert_eq!(describe(s, 600), s);
    }

    #[test]
    fn known_error_keeps_raw_after_hint() {
        let s = "No space left on device";
        let out = describe(s, 600);
        assert!(out.starts_with("磁盘空间不足"));
        assert!(out.contains("引擎原文："));
        assert!(out.contains(s), "原文必须保留，否则没法排错");
    }

    #[test]
    fn empty_stderr_is_not_explained() {
        assert!(explain("").is_none());
        assert!(explain("   \n  ").is_none());
    }

    #[test]
    fn tail_counts_chars_not_bytes() {
        // 中文按字符截断，不能切出半个字
        let s = "转换失败的原因在这里";
        assert_eq!(tail(s, 3), "…在这里");
        assert_eq!(tail("abc", 10), "abc");
    }
}
