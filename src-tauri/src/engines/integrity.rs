//! 下载完整性校验。
//!
//! 为什么需要
//! ----------
//! 引擎托管会从**第三方加速镜像**下载压缩包，解压后立刻执行里面的可执行文件
//! （先跑 `--version` 探测版本，之后每次转换都调用它）。如果镜像被投毒，
//! 等于把任意代码执行的入口交给了对方，而用户点的是「一键下载」。
//!
//! 信任模型
//! --------
//! 唯一有效的不变量是：**哈希必须来自与文件不同的通道**。
//!
//! - 文件可以走镜像（快，但不可信）
//! - 哈希只能走官方直连（慢，但可信）
//!
//! 如果哈希也从镜像取，攻击者同时替换文件和哈希即可通过校验，等于没校验。
//! 所以本模块的 [`fetch_expected`] 一律直连，调用方**不得**给它套镜像前缀。
//!
//! 另一条同样重要的约束：镜像里放的往往不是官方那个包，而是**另一个构建**
//! （例如 FFmpeg 官方源是 gyan.dev，镜像源是 BtbN 的构建）。两者的哈希天然不同，
//! 拿官方哈希去校验镜像包只会全部失败。因此只有「下载地址就是官方地址」时才做
//! 哈希校验，镜像场景只能如实标记为「未校验」，由界面告诉用户。

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// 哈希文件本身很小（几十字节），给一个远小于引擎包的上限，
/// 避免地址误配成安装包直链时把上百 MB 拉进内存。
const MAX_HASH_BODY: usize = 64 * 1024;

/// 一次校验的结论。`verified = false` 不等于失败 —— 它表示「没有可信哈希可比对」，
/// 调用方应继续安装，但必须把这件事告诉用户。
#[derive(Debug, Clone)]
pub struct VerifyOutcome {
    /// 是否真的用可信哈希比对过
    pub verified: bool,
    /// 实际算出来的哈希（永远有值，便于日志与排错）
    pub actual: String,
    /// 比对用的期望哈希
    pub expected: Option<String>,
    /// 未校验时给用户看的原因
    pub note: Option<String>,
}

impl VerifyOutcome {
    /// 给界面用的一句话说明。
    pub fn summary(&self) -> String {
        match (&self.verified, &self.note) {
            (true, _) => "已校验 SHA-256".to_string(),
            (false, Some(n)) => format!("未校验：{n}"),
            (false, None) => "未校验".to_string(),
        }
    }
}

/// 从 `.sha256` / `SHA256SUMS` 文本里提取第一个 64 位十六进制哈希。
///
/// 兼容三种实际遇到的格式：
/// - `<hash>`（gyan.dev 风格，文件里只有哈希）
/// - `<hash>  <filename>`（`sha256sum` 默认输出）
/// - `<hash> *<filename>`（二进制模式）
pub fn parse_sha256(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|t| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|t| t.to_ascii_lowercase())
}

/// 流式计算文件 SHA-256，不把整个包读进内存（FFmpeg 包有 100 MB+）。
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("无法读取下载文件：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取下载文件失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// 从官方地址取期望哈希。**不要给 url 套镜像前缀** —— 套了就失去校验意义。
pub async fn fetch_expected(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Convy/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("创建请求失败：{e}"))?;

    let resp = client.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            "获取官方校验值超时".to_string()
        } else {
            format!("无法获取官方校验值：{e}")
        }
    })?;

    if !resp.status().is_success() {
        return Err(format!("官方校验值返回 HTTP {}", resp.status().as_u16()));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_HASH_BODY {
        return Err("官方校验值内容异常（体积过大）".to_string());
    }
    let text = String::from_utf8_lossy(&bytes);
    parse_sha256(&text).ok_or_else(|| "官方校验值格式无法识别".to_string())
}

/// 比对哈希。不一致时给出两个值，方便用户核对而不是只看到一句「失败」。
pub fn compare(actual: &str, expected: &str) -> Result<(), String> {
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "完整性校验失败：下载内容与官方哈希不一致，已中止安装。\n\
             期望 {expected}\n实际 {actual}\n\
             可能是镜像被篡改或下载中断，请换一个下载源重试。"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba";

    #[test]
    fn parses_plain_hash_file() {
        // gyan.dev 的真实格式：只有一行哈希
        assert_eq!(parse_sha256(&format!("{H}\n")).as_deref(), Some(H));
    }

    #[test]
    fn parses_sha256sum_output() {
        let text = format!("{H}  ffmpeg-release-essentials.zip\n");
        assert_eq!(parse_sha256(&text).as_deref(), Some(H));
        let binary_mode = format!("{H} *ffmpeg.zip\n");
        assert_eq!(parse_sha256(&binary_mode).as_deref(), Some(H));
    }

    #[test]
    fn parses_multiline_sums_and_picks_first() {
        let other = "a".repeat(64);
        let text = format!("{H}  a.zip\n{other}  b.zip\n");
        assert_eq!(parse_sha256(&text).as_deref(), Some(H));
    }

    #[test]
    fn rejects_non_hash_content() {
        // 地址误配成 HTML 页面时不能瞎猜一个「哈希」出来
        assert!(parse_sha256("<!DOCTYPE html><html></html>").is_none());
        assert!(parse_sha256("").is_none());
        // 长度对但不是十六进制
        assert!(parse_sha256(&"z".repeat(64)).is_none());
    }

    #[test]
    fn hash_is_case_insensitive() {
        assert!(compare(&H.to_ascii_uppercase(), H).is_ok());
        assert!(compare(H, &H.to_ascii_uppercase()).is_ok());
    }

    #[test]
    fn mismatch_reports_both_values() {
        let err = compare("00", H).unwrap_err();
        assert!(err.contains("完整性校验失败"));
        assert!(err.contains(H), "错误信息里应包含期望值，便于核对");
    }

    #[test]
    fn hashes_file_contents() {
        let dir = std::env::temp_dir().join(format!("convy-int-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.bin");
        std::fs::write(&p, b"abc").unwrap();
        // "abc" 的 SHA-256 是众所周知的固定值
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_distinguishes_verified_from_unverified() {
        let ok = VerifyOutcome {
            verified: true,
            actual: H.into(),
            expected: Some(H.into()),
            note: None,
        };
        assert!(ok.summary().contains("已校验"));

        let skipped = VerifyOutcome {
            verified: false,
            actual: H.into(),
            expected: None,
            note: Some("镜像源是另一个构建".into()),
        };
        assert!(skipped.summary().contains("未校验"));
        assert!(skipped.summary().contains("镜像"));
    }

    /// 联网测试：清单里填的每个 `sha256Url` 都必须真的能取回一个 SHA-256。
    ///
    /// 这条测试守的是「配置腐烂」—— 官方换了地址、改了文件格式，代码本身不会报错，
    /// 只会悄悄退化成「未校验」，而用户看到的仍然是一次成功的安装。
    /// 手动跑：`cargo test --lib -- --ignored live_manifest_hash`
    #[tokio::test]
    #[ignore = "联网：确认清单里的官方校验地址仍然有效"]
    async fn live_manifest_hash_endpoints_respond() {
        let m = crate::engines::manifest::EngineManifest::load(std::path::Path::new(
            "/nonexistent-convy-test-root",
        ));
        let mut checked = 0;
        for def in &m.engines {
            for (key, spec) in &def.downloads {
                let Some(url) = spec.sha256_url.as_deref() else {
                    continue;
                };
                let got = fetch_expected(url)
                    .await
                    .unwrap_or_else(|e| panic!("{}/{} 的校验地址已失效：{e}", def.id, key));
                assert_eq!(
                    got.len(),
                    64,
                    "{}/{} 返回的不是 SHA-256：{got}",
                    def.id,
                    key
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "清单里至少要有一个配置了 sha256Url 的下载源，否则校验永远不会生效"
        );
    }
}
