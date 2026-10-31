//! 应用自更新：从 GitHub Releases 读取最新版本、更新日志与安装包地址。
//!
//! 为什么不用 tauri-plugin-updater
//! --------------------------------
//! 它要求发布时对安装包做私钥签名、把 `.sig` 一并发布，还得长期保管私钥。
//! 这个项目目前只发 NSIS / MSI，为它引入一整套签名环节的成本大于收益。
//! 所以更新链路由本模块自己实现：检查 → 提示 → 下载 → 校验 → 安装。
//!
//! 完整性
//! ------
//! 安装包可能经第三方镜像下载，而装完就是在本机执行它 —— 这是整条链路上
//! 唯一一处「下载即执行」的动作，所以下载完必须核对 SHA-256。校验值由 CI 在
//! 打包后当场生成（`SHA256SUMS`），与安装包同批发布；详见 [`fetch_sums`]。
//!
//! 镜像
//! ----
//! GitHub Releases 在国内经常连不上，所以内置若干「前缀式」加速：把官方 URL
//! 整体拼在加速域名后面（`https://加速域名/https://api.github.com/…`）。
//! 先并发探测延迟择优，再依次尝试直到取回合法内容 —— 「连得上」和「返回的
//! 内容正确」是两回事，不能只凭探测结果就定死一个源。

use crate::engines::registry::version_cmp;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

/// 仓库地址。release.yml 就是往这里发包的，搬家时改这两个常量即可。
const REPO_OWNER: &str = "yunyu697";
const REPO_NAME: &str = "convy";

/// GitHub API 要求带 UA，否则直接 403。
const USER_AGENT: &str = concat!("Convy/", env!("CARGO_PKG_VERSION"));

/// 单次更新检查最多读多少字节。Release notes 再长也不该是几 MB，
/// 万一上游返回异常内容，也不能把整个响应拉进内存。
const MAX_BODY: usize = 512 * 1024;

// ---------------------------------------------------------------- 镜像

struct BuiltinMirror {
    id: &'static str,
    name: &'static str,
    /// 加速前缀；空 = 直连官方
    prefix: &'static str,
}

/// 内置镜像清单。直连放最后，作为其他加速都不可用时的兜底。
const BUILTIN: &[BuiltinMirror] = &[
    BuiltinMirror {
        id: "ghfast",
        name: "ghfast.top",
        prefix: "https://ghfast.top/",
    },
    BuiltinMirror {
        id: "gh-proxy",
        name: "gh-proxy.com",
        prefix: "https://gh-proxy.com/",
    },
    BuiltinMirror {
        id: "ghproxy",
        name: "ghproxy.net",
        prefix: "https://ghproxy.net/",
    },
    BuiltinMirror {
        id: "hub-mirror",
        name: "hub.gitmirror.com",
        prefix: "https://hub.gitmirror.com/",
    },
    BuiltinMirror {
        id: "official",
        name: "GitHub 官方（直连）",
        prefix: "",
    },
];

/// 前端用的一份镜像描述。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMirror {
    pub id: String,
    pub name: String,
    /// 加速前缀，空 = 直连官方
    pub prefix: String,
    pub builtin: bool,
}

/// 镜像探测结果：给设置页列表展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMirrorTest {
    pub id: String,
    pub name: String,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub message: String,
}

pub fn list_mirrors() -> Vec<AppMirror> {
    BUILTIN
        .iter()
        .map(|m| AppMirror {
            id: m.id.to_string(),
            name: m.name.to_string(),
            prefix: m.prefix.to_string(),
            builtin: true,
        })
        .collect()
}

/// 把加速前缀套到官方地址上。
fn apply_prefix(prefix: &str, url: &str) -> String {
    if prefix.trim().is_empty() {
        url.to_string()
    } else {
        format!("{prefix}{url}")
    }
}

fn official_api_url() -> String {
    format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest")
}

fn official_download_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/{tag}/{asset}")
}

// ---------------------------------------------------------------- 主体类型

/// 一个安装包资源。同一个 Release 里通常同时有 NSIS 的 `.exe` 和 MSI 的 `.msi`。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAsset {
    pub name: String,
    /// 直连下载地址
    pub url: String,
    pub size: u64,
    /// nsis / msi / zip / other
    pub kind: String,
}

/// 一次检查结果。失败时 `error` 有值，界面直接展示原因，而不是假装「已是最新」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    /// Release 页面地址（浏览器打开）
    pub html_url: Option<String>,
    pub published_at: Option<String>,
    /// GitHub Release 正文 —— 也就是发版时写的更新日志
    pub notes: String,
    pub assets: Vec<ReleaseAsset>,
    /// 本次实际用的镜像
    pub mirror_name: String,
    pub mirror_prefix: String,
    pub checked_at: u64,
    pub error: Option<String>,
    /// 因为「还没到下次检查时间」而跳过
    pub skipped_by_interval: bool,
    /// 用户之前选过「跳过此版本」
    pub skipped_by_user: bool,
}

/// 交给上层组装的结果：剥掉 GitHub 的原始字段，避免 API 细节外溢。
#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub html_url: String,
    pub published_at: String,
    pub notes: String,
    pub assets: Vec<ReleaseAsset>,
}

/// GitHub API 返回的 Release（只取用得上的字段）。
#[derive(Debug, Deserialize)]
struct GhRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

// ---------------------------------------------------------------- 检查

/// 拿到最新版本信息。`pref_mirror` 为镜像 id 或 `"auto"`。
///
/// auto 时先并发探测全部镜像的延迟，按快到慢依次尝试，直到有一个真的返回
/// 可解析的内容；剩下的继续兜底。
pub async fn fetch_latest(pref_mirror: &str) -> Result<(Release, AppMirror), String> {
    let api = official_api_url();
    let order = mirror_order(pref_mirror).await;
    let mut last_err = String::from("没有可用的更新源");

    for m in &order {
        match fetch_release(&apply_prefix(&m.prefix, &api)).await {
            Ok(rel) => return Ok((rel, m.clone())),
            Err(e) => last_err = format!("{}：{}", m.name, e),
        }
    }
    Err(last_err)
}

/// 按偏好排出尝试顺序。
async fn mirror_order(pref_mirror: &str) -> Vec<AppMirror> {
    let all = list_mirrors();
    if !pref_mirror.is_empty() && pref_mirror != "auto" {
        if let Some(m) = all.iter().find(|m| m.id == pref_mirror) {
            return vec![m.clone()];
        }
    }
    // auto：探测成功的按延迟升序排前面，失败的照原顺序补在后面兜底
    let mut scored = test_all_mirrors().await;
    scored.retain(|s| s.ok);
    scored.sort_by_key(|s| s.latency_ms.unwrap_or(u64::MAX));

    let mut order: Vec<AppMirror> = scored
        .iter()
        .filter_map(|s| all.iter().find(|m| m.id == s.id).cloned())
        .collect();
    for m in all {
        if !order.iter().any(|o| o.id == m.id) {
            order.push(m);
        }
    }
    order
}

async fn fetch_release(url: &str) -> Result<Release, String> {
    let text = fetch_text(url).await?;
    let rel: GhRelease =
        serde_json::from_str(&text).map_err(|e| format!("无法解析版本信息：{e}"))?;
    if rel.draft {
        return Err("最新版本还是草稿".to_string());
    }
    let tag = rel.tag_name.trim().to_string();
    if tag.is_empty() {
        return Err("响应里没有版本号".to_string());
    }
    // 预发布版本不推给普通用户：自动更新提示应该只指向稳定发行版
    if rel.prerelease {
        return Err("只有预发布版本".to_string());
    }

    let assets = rel
        .assets
        .into_iter()
        .map(|a| ReleaseAsset {
            kind: asset_kind(&a.name).to_string(),
            name: a.name.clone(),
            url: if a.browser_download_url.is_empty() {
                official_download_url(&tag, &a.name)
            } else {
                a.browser_download_url
            },
            size: a.size,
        })
        .collect();

    Ok(Release {
        tag,
        html_url: rel.html_url,
        published_at: rel.published_at,
        notes: clean_notes(&rel.body),
        assets,
    })
}

fn asset_kind(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".msi") {
        "msi"
    } else if lower.ends_with(".exe") {
        "nsis"
    } else if lower.ends_with(".dmg") {
        "dmg"
    } else if lower.ends_with(".appimage") {
        "appimage"
    } else if lower.ends_with(".deb") {
        "deb"
    } else if lower.ends_with(".rpm") {
        "rpm"
    } else if lower.ends_with(".tar.gz") {
        "tar-gz"
    } else if lower.ends_with(".zip") {
        "zip"
    } else {
        "other"
    }
}

/// Release 正文常常带着成片空行和 `\r\n`，直接丢给前端渲染很丑。
fn clean_notes(raw: &str) -> String {
    let text = raw.replace("\r\n", "\n");
    let mut lines: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// 带 UA、超时、体积上限的文本抓取（与引擎版本检测同一套约束）。
async fn fetch_text(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("创建请求失败：{e}"))?;

    let mut req = client.get(url);
    if url.contains("api.github.com") {
        req = req
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "检查超时（12 秒无响应），请检查网络或代理".to_string()
        } else {
            format!("无法连接更新服务器：{e}")
        }
    })?;

    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            403 | 429 => "更新接口限流或拒绝访问，请稍后再试".to_string(),
            404 => "还没有发布任何正式版本".to_string(),
            c => format!("更新服务器返回 HTTP {c}"),
        });
    }

    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取版本信息失败：{e}"))?;
        let remain = MAX_BODY.saturating_sub(buf.len());
        if remain == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..chunk.len().min(remain)]);
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

// ---------------------------------------------------------------- 镜像测速

/// 逐个镜像探测延迟，返回给设置页展示。
pub async fn test_all_mirrors() -> Vec<AppMirrorTest> {
    let all = list_mirrors();
    let url = official_api_url();
    let api_url = &url;
    let probes = all
        .iter()
        .map(|m| async move { (m, probe_mirror(&apply_prefix(&m.prefix, api_url)).await) })
        .collect::<Vec<_>>();
    futures_util::future::join_all(probes)
        .await
        .into_iter()
        .map(|(m, t)| AppMirrorTest {
            id: m.id.clone(),
            name: m.name.clone(),
            ok: t.ok,
            latency_ms: t.latency_ms,
            message: t.message,
        })
        .collect()
}

/// 收到响应头即计时结束，不下载正文 —— 只想知道「能不能连上、多快」。
async fn probe_mirror(url: &str) -> ProbeResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(USER_AGENT)
        .build()
        .unwrap_or_default();
    let start = Instant::now();
    match client.get(url).send().await {
        Ok(resp) => {
            let ms = start.elapsed().as_millis() as u64;
            let status = resp.status();
            ProbeResult {
                ok: status.is_success(),
                latency_ms: Some(ms),
                message: format!("HTTP {} · {} ms", status.as_u16(), ms),
            }
        }
        Err(e) => ProbeResult {
            ok: false,
            latency_ms: None,
            message: if e.is_timeout() {
                "超时（8 秒无响应）".into()
            } else {
                format!("连接失败：{e}")
            },
        },
    }
}

struct ProbeResult {
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
}

// ---------------------------------------------------------------- 判定

/// 远程版本是否比本机新。两边都可能带前缀字符，`version_cmp` 会剥掉 `v` / `n`。
pub fn is_newer(latest: &str, current: &str) -> bool {
    if latest.trim().is_empty() || current.trim().is_empty() {
        return false;
    }
    version_cmp(latest, current) == Ordering::Greater
}

/// 检查频率对应的最小间隔（毫秒）。`"never"` 返回 0，由调用方当作「不检查」。
pub fn interval_ms(freq: &str) -> u64 {
    const DAY: u64 = 24 * 60 * 60 * 1000;
    match freq {
        "daily" => DAY,
        "weekly" => 7 * DAY,
        "monthly" => 30 * DAY,
        _ => 0,
    }
}

/// 距离上次检查是否已到下一次的时间。
pub fn is_due(last_check_at: u64, freq: &str) -> bool {
    let gap = interval_ms(freq);
    if gap == 0 {
        return false;
    }
    let now = crate::job::now_ms();
    last_check_at == 0 || now.saturating_sub(last_check_at) >= gap
}

// ---------------------------------------------------------------- 选包与下载

/// 当前平台优先选择的安装包类型，按顺序尝试。
///
/// Windows 优先 NSIS（`.exe`）而不是 MSI：Tauri 的 NSIS 包支持 `/S` 静默安装，
/// 而 MSI 走 msiexec 需要提权弹 UAC，体验差一截。
pub fn preferred_kinds() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["nsis", "msi", "zip"]
    }
    #[cfg(target_os = "macos")]
    {
        &["dmg", "zip"]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &["appimage", "deb", "rpm", "tar-gz"]
    }
}

/// 从 Release 的资产里挑出本平台该下的那个。
/// 当前机器架构在发行包文件名里的常见写法。
///
/// 同一个 x86_64 在三个平台有三种叫法：Windows 的 `x64`、Linux 的 `amd64`、
/// 以及 Rust triple 里的 `x86_64`。少认一种就会「明明有包却说没有」。
fn arch_tokens() -> &'static [&'static str] {
    #[cfg(target_arch = "x86_64")]
    {
        &["x86_64", "x86-64", "x64", "amd64"]
    }
    #[cfg(target_arch = "aarch64")]
    {
        &["aarch64", "arm64"]
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        &[]
    }
}

/// 与当前架构**相对**的那种架构的写法，用来把别人的包排除掉。
fn other_arch_tokens() -> &'static [&'static str] {
    #[cfg(target_arch = "x86_64")]
    {
        &["aarch64", "arm64"]
    }
    #[cfg(target_arch = "aarch64")]
    {
        &["x86_64", "x86-64", "x64", "amd64"]
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        &[]
    }
}

/// 挑一个安装包。
///
/// 先按平台偏好的类型排序（Windows 优先 NSIS 而非便携 zip），再在同类里按架构筛 ——
/// 只看类型是不够的：macOS 每次发两个 dmg（arm64 + x64），
/// 不筛架构的话 Intel 机器有相当概率拿到 arm64 那个，装上打不开。
pub fn pick_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    let mine = arch_tokens();
    let theirs = other_arch_tokens();
    let lower = |a: &ReleaseAsset| a.name.to_ascii_lowercase();
    let has = |a: &ReleaseAsset, toks: &[&str]| {
        let n = lower(a);
        toks.iter().any(|t| n.contains(t))
    };

    for kind in preferred_kinds() {
        let same_kind: Vec<&ReleaseAsset> = assets.iter().filter(|a| a.kind == *kind).collect();
        let Some(first) = same_kind.first() else {
            continue;
        };
        // 1) 文件名里明确写了本机架构
        if let Some(hit) = same_kind.iter().find(|a| has(a, mine)) {
            return Some(hit);
        }
        // 2) 没写架构的（通用包），或者架构写法我们一个都不认识
        if let Some(hit) = same_kind.iter().find(|a| !has(a, theirs)) {
            return Some(hit);
        }
        // 3) 同类里全是别的架构 —— 宁可报「没有适用包」，也不要让用户下载一个装不上的包
        if !theirs.is_empty() {
            continue;
        }
        return Some(first);
    }
    None
}

/// 下载安装包。返回实际生效的镜像名。
///
/// 与引擎下载同样的策略：按镜像顺序逐个试，失败就换下一个。
/// 这里没有做断点续传 —— 安装包只有几 MB，重下比维护续传状态便宜。
pub async fn download_asset(
    asset: &ReleaseAsset,
    dest: &Path,
    pref_mirror: &str,
    on_progress: impl Fn(f64, String) + Send + Sync,
) -> Result<String, String> {
    let order = mirror_order(pref_mirror).await;
    let mut last_err = String::from("没有可用的下载源");

    for m in &order {
        let url = apply_prefix(&m.prefix, &asset.url);
        on_progress(0.0, format!("正在连接 {}…", m.name));
        match download_stream(&url, dest, asset.size, &on_progress).await {
            Ok(()) => {
                on_progress(1.0, "下载完成".to_string());
                return Ok(m.name.clone());
            }
            Err(e) => {
                // 半个文件留在磁盘上没有任何价值，下一次尝试也从零开始
                let _ = tokio::fs::remove_file(dest).await;
                last_err = format!("{}：{}", m.name, e);
            }
        }
    }
    Err(last_err)
}

async fn download_stream(
    url: &str,
    dest: &Path,
    size_hint: u64,
    on_progress: &(impl Fn(f64, String) + Send + Sync),
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        // 安装包可能几十 MB，别用检查用的 12 秒超时
        .timeout(Duration::from_secs(30 * 60))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("创建请求失败：{e}"))?;

    let resp = client.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            "下载超时".to_string()
        } else {
            format!("无法连接下载服务器：{e}")
        }
    })?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("下载服务器返回 HTTP {}", status.as_u16()));
    }

    // content-length 优先，没有就用 Release 里登记的大小
    let total = resp
        .content_length()
        .filter(|n| *n > 0)
        .unwrap_or(size_hint);

    if let Some(parent) = dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("无法创建文件：{e}"))?;

    let mut stream = resp.bytes_stream();
    let mut got: u64 = 0;
    let mut last_pct: i32 = -1;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断：{e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入失败：{e}"))?;
        got += chunk.len() as u64;

        if total > 0 {
            let pct = ((got as f64 / total as f64) * 100.0).round() as i32;
            if pct != last_pct {
                last_pct = pct;
                on_progress(
                    got as f64 / total as f64,
                    format!("{} / {}", human_size(got), human_size(total)),
                );
            }
        }
    }

    file.flush().await.map_err(|e| format!("写入失败：{e}"))?;

    // 服务器给了长度却收得少，说明连接中途断了 —— 别把半个包当成功
    if total > 0 && got < total {
        return Err(format!(
            "下载不完整（{} / {}）",
            human_size(got),
            human_size(total)
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------- 完整性校验

/// 发布时由 CI 生成的校验文件（`sha256sum` 的输出格式）。
///
/// 信任模型与引擎下载一致（见 `crate::engines::integrity` 的模块注释）：
/// **哈希必须来自与文件不同的通道**。安装包可以走加速镜像，校验文件则优先直连
/// GitHub —— 镜像能改安装包，改不了 GitHub 上的那份清单，对不上就中止。
///
/// 能挡住什么、挡不住什么，这里先说清楚，免得以后有人高估它：
///
/// - 挡得住：下载截断 / 损坏、镜像串包（把别的版本或架构的包配给你）、
///   以及「API 直连而下载走镜像」时镜像对安装包的替换
/// - 挡不住：恶意镜像。前缀式加速把 API 响应也一起代理了，它可以干脆不返回这个
///   附件，让应用退回「未校验」状态。真正的根治手段是代码签名，不是哈希
///
/// 也就是说这层校验降低的是出错概率，不是针对主动攻击的保证 —— 文案上不许说成
/// 「已验证，绝对安全」。
const SUMS_ASSET: &str = "SHA256SUMS";

/// `Sums::source` 的取值：官方直连。其余值是镜像名。
const SOURCE_GITHUB: &str = "github";

/// `VerifyInfo::note` 的取值。界面拿它去查对应语言的文案，
/// 后端不返回成品句子 —— 措辞得跟着界面语言走。
pub mod note {
    /// 这个版本压根没发布校验文件（1.2.1 及更早）
    pub const NO_SUMS: &str = "no-sums";
    /// 校验值是从镜像取到的，只能防损坏，防不了恶意镜像
    pub const VIA_MIRROR: &str = "via-mirror";
}

/// 一次校验的结论，直接序列化给界面。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyInfo {
    /// 是否真的比对过（false = 没有可信哈希，不代表文件有问题）
    pub verified: bool,
    /// 实际算出来的哈希，排错时有用
    pub actual: String,
    pub expected: Option<String>,
    /// 见 [`note`]，`verified = true` 时也可能有值（走了镜像）
    pub note: Option<String>,
}

/// 一份校验清单 + 它是从哪条通道取到的。
#[derive(Debug, Clone)]
pub struct Sums {
    /// (哈希, 文件名)
    pub list: Vec<(String, String)>,
    /// `github` = 官方直连；其余是镜像名
    pub source: String,
}

/// 解析 `sha256sum` 输出。
///
/// 兼容三种写法：`<hash>  <name>`、`<hash> *<name>`（二进制模式）、
/// 以及带 `./` 前缀的路径。无法识别的行直接跳过 —— 宁可少一条，
/// 也不要把半截内容当成哈希。
pub fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut parts = line.splitn(2, char::is_whitespace);
            let hash = parts.next()?.trim();
            if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let name = parts
                .next()?
                .trim()
                .trim_start_matches('*')
                .trim_start_matches("./");
            if name.is_empty() {
                return None;
            }
            Some((hash.to_ascii_lowercase(), name.to_string()))
        })
        .collect()
}

/// 从清单里找某个文件名的哈希。文件名大小写不敏感（macOS 上两者可能不一致）。
fn find_expected<'a>(list: &'a [(String, String)], name: &str) -> Option<&'a str> {
    list.iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|(h, _)| h.as_str())
}

fn sums_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n == "sha256sums" || n == "sha256sums.txt"
    })
}

/// 取校验清单。返回 `None` 表示这个版本没有发布校验文件。
///
/// 先直连，再按镜像顺序兜底：直连能通时哈希与安装包走的是两条通道，
/// 恶意镜像无从下手；直连不通时（国内常见）才退到镜像，并在结果里标明。
pub async fn fetch_sums(
    assets: &[ReleaseAsset],
    pref_mirror: &str,
) -> Option<Result<Sums, String>> {
    let asset = sums_asset(assets)?;

    // 直连优先。注意这里**不套**镜像前缀，套了就失去「不同通道」这个前提。
    let mut last_err = match read_sums(&asset.url).await {
        Ok(list) => {
            return Some(Ok(Sums {
                list,
                source: SOURCE_GITHUB.into(),
            }))
        }
        Err(e) => format!("GitHub：{e}"),
    };

    for m in mirror_order(pref_mirror).await {
        match read_sums(&apply_prefix(&m.prefix, &asset.url)).await {
            Ok(list) => {
                return Some(Ok(Sums {
                    list,
                    source: m.name.clone(),
                }))
            }
            Err(e) => last_err = format!("{}：{e}", m.name),
        }
    }
    Some(Err(last_err))
}

/// 取回并解析校验文件。解析不出任何条目也算失败 —— 有些镜像会把 404
/// 渲染成一张 HTML 错误页，HTTP 200 但内容不是清单，不能当「空清单」放过。
async fn read_sums(url: &str) -> Result<Vec<(String, String)>, String> {
    let text = fetch_text(url).await?;
    let list = parse_sums(&text);
    if list.is_empty() {
        return Err(format!("{SUMS_ASSET} 内容为空或格式无法识别"));
    }
    Ok(list)
}

/// 比对实际哈希与清单。不一致返回 `Err`，调用方负责删掉文件再报错。
///
/// 「清单里没有这个文件」也算失败：发版流程会断言清单与产物一一对应，
/// 出现缺项说明发布本身有问题，这时候装下去比不装更危险。
pub fn check_against_sums(
    actual: &str,
    sums: Option<&Sums>,
    asset_name: &str,
) -> Result<VerifyInfo, String> {
    let Some(sums) = sums else {
        return Ok(VerifyInfo {
            verified: false,
            actual: actual.to_string(),
            expected: None,
            note: Some(note::NO_SUMS.to_string()),
        });
    };

    let expected = find_expected(&sums.list, asset_name).ok_or_else(|| {
        format!("{asset_name} 不在 {SUMS_ASSET} 清单里，已中止安装。请到发布页手动下载。")
    })?;

    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "完整性校验失败：安装包与官方哈希不一致，已删除下载的文件。\n\
             期望 {expected}\n实际 {actual}\n\
             可能是下载过程中被篡改或文件损坏，请换一个更新源重试。"
        ));
    }

    Ok(VerifyInfo {
        verified: true,
        actual: actual.to_string(),
        expected: Some(expected.to_string()),
        // 走了镜像就如实标注：能防损坏与镜像串包，防不了蓄意投毒
        note: if sums.source == SOURCE_GITHUB {
            None
        } else {
            Some(note::VIA_MIRROR.to_string())
        },
    })
}

/// 算哈希并比对。
///
/// 安装包最大有 78 MB（AppImage），读盘 + 哈希是同步阻塞操作，
/// 丢到 `spawn_blocking` 里算，别卡住 tokio 的 worker 线程。
pub async fn verify_file(
    path: &Path,
    asset_name: &str,
    sums: Option<Sums>,
) -> Result<VerifyInfo, String> {
    let p = path.to_path_buf();
    let actual = tokio::task::spawn_blocking(move || crate::engines::integrity::sha256_file(&p))
        .await
        .map_err(|e| format!("校验任务失败：{e}"))??;
    check_against_sums(&actual, sums.as_ref(), asset_name)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

// ---------------------------------------------------------------- 安装

/// 安装动作的结论，前端据此决定提示什么。
///
/// 只回一个 `mode`，不回文案：文案要跟着界面语言走，而语言是前端才知道的事。
/// 早先这里带过一句中文 `message`，界面切成英/日/西之后它仍然是中文，
/// 于是把这句话的措辞权交还给前端。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    /// silent = 已在后台静默安装（应用需要退出）；opened = 已交给系统默认程序
    pub mode: String,
}

/// 启动安装。
///
/// 平台差异是硬约束，不假装统一：
/// - Windows：NSIS 用 `/S` 静默装，MSI 走 `msiexec /qn`，装完自己重启；
/// - macOS：`.dmg` 只能挂载后由用户拖进「应用程序」，没有免签名的静默通道；
/// - Linux：`.deb` / `.rpm` 需要 root，交给系统包管理器弹窗。
///
/// 所以返回值里带 `mode`，前端对 silent 和 opened 的文案是两套。
pub fn install(path: &Path) -> Result<InstallOutcome, String> {
    if !path.exists() {
        return Err("安装包不存在，请重新下载".to_string());
    }

    #[cfg(windows)]
    {
        let lower = path.to_string_lossy().to_ascii_lowercase();
        if lower.ends_with(".msi") {
            let mut cmd = std::process::Command::new("msiexec");
            cmd.arg("/i").arg(path).args(["/qn", "/norestart"]);
            crate::spawn::silent(cmd)
                .spawn()
                .map_err(|e| format!("无法启动安装程序：{e}"))?;
        } else if lower.ends_with(".exe") {
            // Tauri NSIS 安装器支持 /S 静默；装完会自己拉起新版本
            let mut cmd = std::process::Command::new(path);
            cmd.arg("/S");
            crate::spawn::silent(cmd)
                .spawn()
                .map_err(|e| format!("无法启动安装程序：{e}"))?;
        } else {
            return open_with_system(path).map(|_| InstallOutcome {
                mode: "opened".into(),
            });
        }
        Ok(InstallOutcome {
            mode: "silent".into(),
        })
    }

    #[cfg(not(windows))]
    {
        open_with_system(path)?;
        Ok(InstallOutcome {
            mode: "opened".into(),
        })
    }
}

#[cfg(not(windows))]
fn open_with_system(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let prog = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let prog = "xdg-open";

    std::process::Command::new(prog)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开安装包：{e}"))
}

#[cfg(windows)]
fn open_with_system(path: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(path);
    crate::spawn::silent(cmd)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开安装包：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_is_numeric_not_string() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        // 空值不能被当成「有更新」
        assert!(!is_newer("", "0.1.0"));
        assert!(!is_newer("0.2.0", ""));
    }

    #[test]
    fn interval_mapping_is_monotonic() {
        assert_eq!(interval_ms("never"), 0);
        assert_eq!(interval_ms("daily"), 24 * 60 * 60 * 1000);
        assert!(interval_ms("weekly") > interval_ms("daily"));
        assert!(interval_ms("monthly") > interval_ms("weekly"));
    }

    #[test]
    fn asset_kind_detection() {
        assert_eq!(asset_kind("Convy_0.1.0_x64-setup.exe"), "nsis");
        assert_eq!(asset_kind("Convy_0.1.0_x64_en-US.MSI"), "msi");
        assert_eq!(asset_kind("convy.zip"), "zip");
        assert_eq!(asset_kind("latest.json"), "other");
    }

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            url: format!("https://example.invalid/{name}"),
            size: 1,
            kind: asset_kind(name).to_string(),
        }
    }

    /// v1.2.0 的真实产物名，直接拿真数据验选包
    fn v120_assets() -> Vec<ReleaseAsset> {
        [
            "Convy-1.2.0-1.x86_64.rpm",
            "Convy-linux-x86_64-bin.tar.gz",
            "Convy-portable-win64.zip",
            "Convy_1.2.0_aarch64.dmg",
            "Convy_1.2.0_amd64.AppImage",
            "Convy_1.2.0_amd64.deb",
            "Convy_1.2.0_x64-setup.exe",
            "Convy_1.2.0_x64.dmg",
            "Convy_1.2.0_x64_en-US.msi",
        ]
        .iter()
        .map(|n| asset(n))
        .collect()
    }

    #[test]
    fn picked_asset_matches_current_arch() {
        let assets = v120_assets();
        let picked = pick_asset(&assets).expect("真实产物里应当能挑到一个包");
        let name = picked.name.to_ascii_lowercase();
        for token in other_arch_tokens() {
            assert!(
                !name.contains(token),
                "挑到了别的架构的包：{}（本机架构 {:?}）",
                picked.name,
                arch_tokens()
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_prefers_nsis_installer() {
        let assets = v120_assets();
        assert_eq!(
            pick_asset(&assets).unwrap().name,
            "Convy_1.2.0_x64-setup.exe"
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn foreign_arch_only_yields_none() {
        // 只有 arm64 包时，宁可报「没有适用包」，也不能让 Intel 机器下回来一个装不上的
        let only_arm = vec![asset("Convy-1.2.0-aarch64-portable.zip")];
        assert!(pick_asset(&only_arm).is_none());
    }

    #[test]
    fn arch_less_asset_is_still_picked() {
        // 名字里不带架构的通用包，不能被架构筛选误杀
        let mut generic = asset("Convy_1.2.0_universal.bin");
        generic.kind = preferred_kinds()[0].to_string();
        assert!(pick_asset(&[generic]).is_some());
    }

    #[test]
    fn mirror_prefix_applied_only_when_present() {
        let api = "https://api.github.com/repos/a/b/releases/latest";
        assert_eq!(apply_prefix("", api), api);
        assert_eq!(
            apply_prefix("https://ghfast.top/", api),
            "https://ghfast.top/https://api.github.com/repos/a/b/releases/latest"
        );
    }

    #[test]
    fn notes_trimmed_and_normalized() {
        assert_eq!(
            clean_notes("\n\n  hello  \nworld\r\n\n\n"),
            "  hello\nworld"
        );
        assert_eq!(clean_notes(""), "");
    }

    #[test]
    fn builtin_mirror_ids_are_unique() {
        let ids: Vec<_> = list_mirrors().into_iter().map(|m| m.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn never_interval_is_never_due() {
        assert!(!is_due(0, "never"));
        // 频率已到：上次检查是很久以前
        assert!(is_due(1, "daily"));
    }

    // ------------------------------------------------ 完整性校验

    const H: &str = "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba";
    const H2: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sums_parses_all_three_line_shapes() {
        // `sha256sum` 在 Linux 上是两个空格，二进制模式带 `*`，
        // 而有人手工生成时可能写成 `./名字`
        let text = format!(
            "{H}  Convy_1.2.2_x64-setup.exe\n\
             {H2} *Convy_1.2.2_aarch64.dmg\n\
             {H}  ./Convy_1.2.2_amd64.deb\n"
        );
        let list = parse_sums(&text);
        assert_eq!(list.len(), 3);
        assert_eq!(find_expected(&list, "Convy_1.2.2_x64-setup.exe"), Some(H));
        assert_eq!(find_expected(&list, "Convy_1.2.2_aarch64.dmg"), Some(H2));
        // `./` 前缀要被剥掉，否则名字对不上、每次更新都会报「不在清单里」
        assert_eq!(find_expected(&list, "Convy_1.2.2_amd64.deb"), Some(H));
    }

    #[test]
    fn sums_ignores_garbage_lines() {
        // 镜像把 404 渲染成 HTML 时，不能从里面「猜」出一个哈希
        let text = format!("<html>404</html>\n\n# 注释\nzzz  x\n{H}\n");
        let list = parse_sums(&text);
        // 只剩最后一行：只有哈希、没有文件名 —— 没有名字就没法对应到安装包
        assert!(list.is_empty(), "不该从垃圾内容里解析出条目：{list:?}");
    }

    #[test]
    fn sums_name_match_is_case_insensitive() {
        let list = vec![(H.to_string(), "CONVY_1.2.2_X64-SETUP.EXE".to_string())];
        assert_eq!(find_expected(&list, "Convy_1.2.2_x64-setup.exe"), Some(H));
    }

    #[test]
    fn sums_asset_found_among_real_assets() {
        let mut assets = v120_assets();
        assert!(sums_asset(&assets).is_none(), "老版本没有校验文件");
        assets.push(asset(SUMS_ASSET));
        assert_eq!(sums_asset(&assets).unwrap().name, SUMS_ASSET);
        // 大小写不同也要认
        assets.pop();
        assets.push(asset("sha256sums"));
        assert!(sums_asset(&assets).is_some());
    }

    #[test]
    fn missing_sums_is_reported_not_silently_accepted() {
        // 1.2.1 及更早没发校验文件：可以继续装，但必须让界面能说明白
        let v = check_against_sums(H, None, "Convy_1.2.2_x64-setup.exe").unwrap();
        assert!(!v.verified);
        assert_eq!(v.note.as_deref(), Some(note::NO_SUMS));
        assert!(v.expected.is_none());
    }

    #[test]
    fn matching_hash_is_verified() {
        let sums = Sums {
            list: vec![(H.to_string(), "Convy_1.2.2_x64-setup.exe".to_string())],
            source: SOURCE_GITHUB.into(),
        };
        let v = check_against_sums(H, Some(&sums), "Convy_1.2.2_x64-setup.exe").unwrap();
        assert!(v.verified);
        assert_eq!(v.note, None, "直连取到的校验值不该带「保护有限」的标注");
    }

    #[test]
    fn mismatch_is_fatal_and_mentions_both_hashes() {
        let sums = Sums {
            list: vec![(H2.to_string(), "Convy_1.2.2_x64-setup.exe".to_string())],
            source: SOURCE_GITHUB.into(),
        };
        let err = check_against_sums(H, Some(&sums), "Convy_1.2.2_x64-setup.exe").unwrap_err();
        assert!(err.contains("完整性校验失败"));
        assert!(err.contains(H2), "要给出期望值，用户才能核对");
        assert!(err.contains(H), "要给出实际值，否则没法判断是不是下载坏了");
    }

    #[test]
    fn asset_missing_from_sums_is_fatal() {
        // 清单里没有这个包 = 发版流程出了问题，这时候装下去比不装更危险
        let sums = Sums {
            list: vec![(H.to_string(), "Convy_1.2.2_x64.dmg".to_string())],
            source: SOURCE_GITHUB.into(),
        };
        let err = check_against_sums(H, Some(&sums), "Convy_1.2.2_x64-setup.exe").unwrap_err();
        assert!(err.contains("不在"));
    }

    #[test]
    fn mirror_sourced_sums_are_marked() {
        // 直连不通时退到镜像：仍然比对，但要标明保护强度有限，不能假装等价
        let sums = Sums {
            list: vec![(H.to_string(), "Convy_1.2.2_x64-setup.exe".to_string())],
            source: "GhProxy".into(),
        };
        let v = check_against_sums(H, Some(&sums), "Convy_1.2.2_x64-setup.exe").unwrap();
        assert!(v.verified);
        assert_eq!(v.note.as_deref(), Some(note::VIA_MIRROR));
    }

    #[tokio::test]
    async fn verify_file_hashes_real_bytes() {
        let dir = std::env::temp_dir().join(format!("convy-upd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Convy_1.2.2_x64-setup.exe");
        std::fs::write(&p, b"abc").unwrap();

        // "abc" 的 SHA-256 是固定值，正好验证「真的读了文件」而不是走了别的分支
        let sums = Sums {
            list: vec![(H2.to_string(), "Convy_1.2.2_x64-setup.exe".to_string())],
            source: SOURCE_GITHUB.into(),
        };
        let v = verify_file(&p, "Convy_1.2.2_x64-setup.exe", Some(sums))
            .await
            .unwrap();
        assert!(v.verified);
        assert_eq!(v.actual, H2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn verify_file_rejects_tampered_bytes() {
        let dir = std::env::temp_dir().join(format!("convy-upd-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Convy_1.2.2_x64-setup.exe");
        std::fs::write(&p, b"not what CI built").unwrap();

        let sums = Sums {
            list: vec![(H2.to_string(), "Convy_1.2.2_x64-setup.exe".to_string())],
            source: SOURCE_GITHUB.into(),
        };
        assert!(verify_file(&p, "Convy_1.2.2_x64-setup.exe", Some(sums))
            .await
            .is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
