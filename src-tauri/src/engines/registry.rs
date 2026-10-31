//! 引擎管理器：下载、解压、启用、回退。
//!
//! 目录约定（位于应用数据目录下的 `engines/`）：
//!   <root>/<id>/active.txt          记录当前启用的版本目录名
//!   <root>/<id>/<version>/...       某次下载解压后的内容，含 `spec.bin`
//!   <root>/<id>/downloads/          安装器类（installer/external 直链）的下载落点
//!   <root>/manifest.json            可选的用户覆盖清单
//!
//! 进度通过 `ProgressCb` 回调上抛，命令层再转成前端事件，便于做下载进度动画。

use super::layout;
use super::manifest::{platform_key, DownloadSpec, EngineDef, EngineManifest};
use crate::probe::Engines;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

/// 版本检测请求的 UA：GitHub API 对没有 UA 的请求会直接拒绝，
/// 官方也建议带上能识别应用的标识。
const USER_AGENT: &str = concat!("Convy/", env!("CARGO_PKG_VERSION"));

/// 版本检测响应最多读这么多字节。
/// 清单里的地址理论上都是版本文件 / JSON / 目录页，
/// 万一被误配成压缩包直链，也不能把整个安装包拉进内存。
const MAX_VERSION_BODY: usize = 512 * 1024;

pub type ProgressCb = Arc<dyn Fn(&str, f32, &str) + Send + Sync>;

/// 一个下载源：内置镜像或用户自添加镜像。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorDef {
    pub name: String,
    pub url: String,
    /// true = 清单内置；false = 用户添加（可删除）
    pub builtin: bool,
}

/// 镜像测速结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorTest {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub message: String,
}

pub struct EngineManager {
    pub root: PathBuf,
    pub manifest: EngineManifest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub id: String,
    pub label: String,
    pub license: String,
    pub description: String,
    /// 本平台是否有可自动托管的下载源（zip/tar 类）
    pub managed: bool,
    /// PATH / 系统安装里能找到（无论是否我们托管）
    pub installed: bool,
    /// 来自系统 PATH 而非本程序托管目录
    pub system: bool,
    pub active_version: Option<String>,
    pub available_versions: Vec<String>,
    pub latest_remote: Option<String>,
    pub update_available: bool,
    pub kind: String,
    pub url: Option<String>,
    /// 全部可选下载源：内置镜像 + 用户添加的镜像
    pub mirrors: Vec<MirrorDef>,
    pub homepage: Option<String>,
    pub bin_path: Option<String>,
    /// 安装包体积与必需组件说明
    pub size_hint: Option<String>,
}

/// 一次版本检查的结果（当前版本 / 远程最新 / 是否有更新 / 失败原因）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCheck {
    pub id: String,
    /// 本机当前版本（托管读 active 目录探测，系统安装直接跑二进制）
    pub current: Option<String>,
    /// 远程最新版本
    pub latest: Option<String>,
    pub update_available: bool,
    /// 检查失败时的原因，便于用户在界面上直接看到为什么会失败
    pub error: Option<String>,
    /// 本次检查使用的版本地址，排障时很有用
    pub source: Option<String>,
    pub checked_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineUpdateResult {
    pub id: String,
    pub version: Option<String>,
    /// 仅安装器/外链类：文件被存到的路径
    pub saved_path: Option<String>,
    /// 仅外链类（非直链）：应打开的官网页面
    pub homepage: Option<String>,
    /// 是否已被托管目录自动启用（zip/tar 类）
    pub installed: bool,
}

impl EngineManager {
    pub fn new(root: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&root);
        let manifest = EngineManifest::load(&root);
        EngineManager { root, manifest }
    }

    pub fn engine_root(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// 当前启用的版本目录名（读 active.txt）。
    pub fn read_active(&self, id: &str) -> Option<String> {
        let p = self.engine_root(id).join("active.txt");
        std::fs::read_to_string(p)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn write_active(&self, id: &str, version: &str) -> Result<(), String> {
        let p = self.engine_root(id).join("active.txt");
        std::fs::write(p, version).map_err(|e| e.to_string())
    }

    /// 托管目录下某版本的可执行文件绝对路径。
    ///
    /// 不直接拼 `<version>/<bin>`：发行包解压后可能多一层包装目录，
    /// 统一交给 layout::resolve_bin 处理（清单路径 → .convy-bin → 浅层搜索）。
    pub fn managed_bin(&self, id: &str, version: &str, bin: &str) -> Option<PathBuf> {
        layout::resolve_bin(&self.engine_root(id).join(version), bin)
    }

    pub fn list(&self, engines: &Engines) -> Vec<EngineStatus> {
        let key = platform_key();
        let user_mirrors = self.load_user_mirrors();
        let mut out = Vec::new();
        for def in &self.manifest.engines {
            let spec = def.downloads.get(&key);
            let managed = spec
                .map(|s| matches!(s.kind.as_str(), "zip" | "tar-gz" | "tar-xz"))
                .unwrap_or(false);
            // 内置镜像 + 用户镜像
            let mut mirrors: Vec<MirrorDef> = spec
                .and_then(|s| s.mirror.as_ref())
                .map(|m| {
                    vec![MirrorDef {
                        name: "内置镜像".into(),
                        url: m.clone(),
                        builtin: true,
                    }]
                })
                .unwrap_or_default();
            if let Some(extra) = user_mirrors.get(&def.id) {
                mirrors.extend(extra.iter().cloned());
            }

            if managed {
                let root = self.engine_root(&def.id);
                let versions = list_versions(&root);
                let active = self.read_active(&def.id);
                let (active_version, bin_path) = if let Some(v) = &active {
                    if let Some(bin) = spec.and_then(|s| self.managed_bin(&def.id, v, &s.bin)) {
                        let ver = probe_version(
                            &bin,
                            &spec.as_ref().unwrap().version.args,
                            &spec.as_ref().unwrap().version.regex,
                        );
                        (
                            ver.or_else(|| Some(v.clone())),
                            Some(bin.to_string_lossy().to_string()),
                        )
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                out.push(EngineStatus {
                    id: def.id.clone(),
                    label: def.label.clone(),
                    license: def.license.clone(),
                    description: def.description.clone(),
                    managed: true,
                    installed: bin_path.is_some() || !versions.is_empty(),
                    system: false,
                    active_version,
                    available_versions: versions,
                    latest_remote: None,
                    update_available: false,
                    kind: spec.unwrap().kind.clone(),
                    url: Some(spec.unwrap().url.clone()),
                    mirrors: mirrors.clone(),
                    homepage: spec.unwrap().homepage.clone(),
                    bin_path,
                    size_hint: spec.and_then(|s| s.size_hint.clone()),
                });
            } else if let Some(spec) = spec {
                // external / installer：仅展示系统是否安装 + 官网入口
                let system = system_present(&def.id, engines);
                out.push(EngineStatus {
                    id: def.id.clone(),
                    label: def.label.clone(),
                    license: def.license.clone(),
                    description: def.description.clone(),
                    managed: false,
                    installed: system,
                    system: true,
                    active_version: None,
                    available_versions: vec![],
                    latest_remote: None,
                    update_available: false,
                    kind: spec.kind.clone(),
                    url: Some(spec.url.clone()),
                    mirrors: mirrors.clone(),
                    homepage: spec.homepage.clone(),
                    bin_path: None,
                    size_hint: spec.size_hint.clone(),
                });
            }
        }
        out
    }

    /// 远程最新版本（用于判断是否有更新）。
    ///
    /// 返回 `Ok(None)` 表示该引擎没有配置版本检测源；
    /// `Err` 表示检查过程本身失败（网络 / 限流 / 正则不匹配），
    /// 上层会把原因透到界面上，而不是静默显示「已是最新」。
    pub async fn latest_version(&self, id: &str) -> Result<Option<String>, String> {
        let Some((url, check)) = self.latest_source(id) else {
            return Ok(None);
        };
        let text = fetch_text(&url).await?;
        Ok(pick_version(&text, &check.regex, check.pick_max()))
    }

    /// 该引擎版本检测实际使用的地址与规则。
    pub fn latest_source(&self, id: &str) -> Option<(String, super::manifest::LatestCheck)> {
        let key = platform_key();
        let def = self.manifest.engines.iter().find(|e| e.id == id)?;
        let spec = def.downloads.get(&key)?;
        let check = spec.latest.clone()?;
        let url = if check.use_mirror {
            spec.mirror.clone().unwrap_or_else(|| check.url.clone())
        } else {
            check.url.clone()
        };
        Some((url, check))
    }

    /// 本机当前版本：托管引擎读 active 目录里的二进制并探测，
    /// 系统安装的引擎直接跑 PATH 里的那个二进制。
    pub fn current_version(&self, engines: &Engines, id: &str) -> Option<String> {
        let key = platform_key();
        let def = self.manifest.engines.iter().find(|e| e.id == id)?;
        let spec = def.downloads.get(&key)?;

        if let Some(active) = self.read_active(id) {
            if let Some(bin) = self.managed_bin(id, &active, &spec.bin) {
                if let Some(v) = probe_version(&bin, &spec.version.args, &spec.version.regex) {
                    return Some(v);
                }
            }
            // 探测不出版本时退回目录名（下载时就是用版本号命名的）
            return Some(active);
        }

        let bin = system_bin(id, engines)?;
        probe_version(&bin, &spec.version.args, &spec.version.regex)
    }

    /// 完整检查一个引擎：当前版本 + 远程最新 + 是否可更新。
    pub async fn check(&self, engines: &Engines, id: &str) -> EngineCheck {
        let current = self.current_version(engines, id);
        let source = self.latest_source(id).map(|(url, _)| url);
        let (latest, error) = match self.latest_version(id).await {
            Ok(v) => (v, None),
            Err(e) => (None, Some(e)),
        };
        let update_available = match (&latest, &current) {
            (Some(l), Some(c)) => version_cmp(l, c) == Ordering::Greater,
            _ => false,
        };
        EngineCheck {
            id: id.to_string(),
            current,
            latest,
            update_available,
            error,
            source,
            checked_at: crate::job::now_ms(),
        }
    }

    /// 下载并启用（或下载安装包到目录）。`mirror_url` 指定镜像源；None 用官方原地址。
    pub async fn update(
        &self,
        app: &AppHandle,
        id: &str,
        mirror_url: Option<&str>,
        cb: ProgressCb,
    ) -> Result<EngineUpdateResult, String> {
        let key = platform_key();
        let def = self
            .manifest
            .engines
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("未知引擎：{}", id))?;
        let spec = def
            .downloads
            .get(&key)
            .ok_or_else(|| "本平台暂不支持自动下载该引擎".to_string())?;

        let url = mirror_url
            .filter(|u| !u.trim().is_empty())
            .unwrap_or(&spec.url)
            .to_string();

        // external / installer：处理「下载到正确文件夹」或「打开官网」
        if matches!(spec.kind.as_str(), "external" | "installer") {
            return self.download_external(app, def, spec, &url, cb).await;
        }

        // zip / tar-gz / tar-xz：完整托管流程
        let root = self.engine_root(id);
        let _ = std::fs::create_dir_all(&root);
        let staging = root.join("_staging");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

        let tmp_archive = root.join("_download.bin");
        cb("download", 0.0, "正在连接下载服务器…");
        download_to_file(app, &url, &tmp_archive, &cb).await?;

        cb("extract", 0.85, "正在解压…");
        extract(&tmp_archive, &staging, &spec.kind).map_err(|e| format!("解压失败：{}", e))?;
        let _ = std::fs::remove_file(&tmp_archive);

        // 发行包普遍多一层包装目录（如 ffmpeg-9.0.2-essentials_build/bin/…），
        // 先展平再定位可执行文件，否则清单里的相对路径永远对不上。
        let flattened = layout::normalize_layout(&staging);
        if flattened > 0 {
            cb("extract", 0.9, "正在整理目录结构…");
        }

        // 探测版本号，用于命名版本目录（必须在裁剪前完成，探测依赖二进制）
        let Some(bin) = layout::resolve_bin(&staging, &spec.bin) else {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!(
                "解压后未找到可执行文件 {}，清单中的 bin 路径可能不正确",
                spec.bin
            ));
        };
        let version = probe_version(&bin, &spec.version.args, &spec.version.regex)
            .unwrap_or_else(|| format!("build-{}", crate::job::now_ms() / 1000));
        let bin_rel = bin
            .strip_prefix(&staging)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| spec.bin.clone());

        // 删除转换用不到的组件（ffplay / doc / presets 等），省几十 MB 磁盘
        if !spec.prune.is_empty() {
            let removed = layout::prune(&staging, &spec.prune);
            if !removed.is_empty() {
                cb(
                    "extract",
                    0.93,
                    &format!("已清理无用组件：{}", removed.join("、")),
                );
            }
        }

        let target = root.join(safe_name(&version));
        if target.exists() {
            let _ = std::fs::remove_dir_all(&target);
        }
        std::fs::rename(&staging, &target).map_err(|e| e.to_string())?;
        // 记录真实可执行文件的相对路径，后续探测/回退不必再猜
        let _ = layout::write_bin_hint(&target, &bin_rel);

        // 写 active：若首次安装或当前 active 失效，则指向新版本（保留旧版本以支持回退）
        let need_activate = match self.read_active(id) {
            Some(active) => !self
                .managed_bin(id, &active, &spec.bin)
                .map(|p| p.is_file())
                .unwrap_or(false),
            None => true,
        };
        if need_activate {
            self.write_active(id, &safe_name(&version))?;
        }

        cb("done", 1.0, &format!("已启用 {}", version));
        Ok(EngineUpdateResult {
            id: id.to_string(),
            version: Some(version),
            saved_path: None,
            homepage: None,
            installed: true,
        })
    }

    async fn download_external(
        &self,
        _app: &AppHandle,
        def: &EngineDef,
        spec: &DownloadSpec,
        url: &str,
        cb: ProgressCb,
    ) -> Result<EngineUpdateResult, String> {
        // 直链（以常见二进制扩展名结尾）→ 下载到 downloads/
        let is_file = url
            .rsplit('/')
            .next()
            .map(|f| f.contains('.'))
            .unwrap_or(false)
            && (url.ends_with(".exe")
                || url.ends_with(".msi")
                || url.ends_with(".dmg")
                || url.ends_with(".zip")
                || url.ends_with(".tar.gz")
                || url.ends_with(".tar.xz")
                || url.ends_with(".AppImage")
                || url.ends_with(".deb")
                || url.ends_with(".rpm"));

        if is_file {
            let root = self.engine_root(&def.id);
            let dl = root.join("downloads");
            let _ = std::fs::create_dir_all(&dl);
            let fname = url.rsplit('/').next().unwrap_or("package.bin").to_string();
            let dest = dl.join(fname);
            cb("download", 0.0, "正在下载安装包…");
            // 复用下载逻辑但无需进度回调复杂化
            download_to_file(_app, url, &dest, &cb).await?;
            cb("done", 1.0, "安装包已保存到引擎目录");
            Ok(EngineUpdateResult {
                id: def.id.clone(),
                version: None,
                saved_path: Some(dest.to_string_lossy().to_string()),
                homepage: None,
                installed: false,
            })
        } else {
            // 仅提供官网页面，由前端打开
            cb("done", 1.0, "请在官网下载并安装");
            Ok(EngineUpdateResult {
                id: def.id.clone(),
                version: None,
                saved_path: None,
                homepage: spec.homepage.clone(),
                installed: false,
            })
        }
    }

    /// 回退 / 启用指定版本。
    pub fn set_active(&self, id: &str, version: &str) -> Result<(), String> {
        let root = self.engine_root(id);
        if !root.join(version).is_dir() {
            return Err(format!("版本目录不存在：{}", version));
        }
        self.write_active(id, version)
    }

    // ------------------------------------------------------------ 用户镜像

    fn mirrors_path(&self) -> PathBuf {
        self.root.join("mirrors.json")
    }

    /// 读取用户自添加镜像：<root>/mirrors.json，形如 { "ffmpeg": [ {name,url}, … ] }。
    pub fn load_user_mirrors(&self) -> BTreeMap<String, Vec<MirrorDef>> {
        std::fs::read_to_string(self.mirrors_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_user_mirrors(&self, map: &BTreeMap<String, Vec<MirrorDef>>) -> Result<(), String> {
        let _ = std::fs::create_dir_all(&self.root);
        let text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        std::fs::write(self.mirrors_path(), text).map_err(|e| e.to_string())
    }

    /// 添加用户镜像（同 URL 去重）。
    pub fn add_mirror(&self, id: &str, name: &str, url: &str) -> Result<(), String> {
        let url = url.trim().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("镜像地址必须以 http:// 或 https:// 开头".into());
        }
        let mut map = self.load_user_mirrors();
        let entry = map.entry(id.to_string()).or_default();
        if entry.iter().any(|m| m.url == url) {
            return Err("该镜像地址已存在".into());
        }
        let name = if name.trim().is_empty() {
            // 默认用主机名当显示名
            url.split("//")
                .nth(1)
                .and_then(|h| h.split('/').next())
                .unwrap_or("自定义镜像")
                .to_string()
        } else {
            name.trim().to_string()
        };
        entry.push(MirrorDef {
            name,
            url,
            builtin: false,
        });
        self.save_user_mirrors(&map)
    }

    /// 删除用户镜像（内置镜像不可删）。
    pub fn remove_mirror(&self, id: &str, url: &str) -> Result<(), String> {
        let mut map = self.load_user_mirrors();
        let Some(entry) = map.get_mut(id) else {
            return Err("未找到该镜像".into());
        };
        let before = entry.len();
        entry.retain(|m| m.url != url);
        if entry.len() == before {
            return Err("未找到该镜像".into());
        }
        if entry.is_empty() {
            map.remove(id);
        }
        self.save_user_mirrors(&map)
    }

    /// 镜像测速：发起 GET 请求，收到响应头即计时结束（不下载正文）。
    pub async fn test_mirror(url: &str) -> MirrorTest {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .unwrap_or_default();
        let start = Instant::now();
        match client.get(url).send().await {
            Ok(resp) => {
                let ms = start.elapsed().as_millis() as u64;
                let status = resp.status();
                MirrorTest {
                    ok: status.is_success(),
                    latency_ms: Some(ms),
                    message: format!("HTTP {} · {} ms", status.as_u16(), ms),
                }
            }
            Err(e) => MirrorTest {
                ok: false,
                latency_ms: None,
                message: if e.is_timeout() {
                    "超时（8 秒无响应）".into()
                } else {
                    format!("连接失败：{}", e)
                },
            },
        }
    }
}

// ---------------------------------------------------------------- 辅助

/// 列出 <root>/<id>/ 下的版本目录（排除临时目录），按版本号从新到旧。
fn list_versions(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('_') || name == "downloads" {
                continue;
            }
            out.push(name);
        }
    }
    // 目录名就是版本号，按数值排序（字符串排序会把 9.0 排到 10.0 后面）
    out.sort_by(|a, b| version_cmp(b, a));
    out
}

/// 引擎 id → 系统里那个二进制（用于读系统安装的版本号）。
fn system_bin(id: &str, engines: &Engines) -> Option<PathBuf> {
    match id {
        "ffmpeg" => engines.ffmpeg.clone().or_else(|| engines.ffprobe.clone()),
        "imagemagick" => engines.magick.clone().or_else(|| engines.vips.clone()),
        "calibre" => engines.ebook_convert.clone(),
        "libreoffice" => engines.soffice.clone(),
        "pandoc" => engines.pandoc.clone(),
        _ => None,
    }
}

fn system_present(id: &str, engines: &Engines) -> bool {
    match id {
        "ffmpeg" => engines.ffmpeg.is_some(),
        "imagemagick" => engines.magick.is_some() || engines.vips.is_some(),
        "calibre" => engines.ebook_convert.is_some(),
        "libreoffice" => engines.soffice.is_some(),
        "pandoc" => engines.pandoc.is_some(),
        _ => false,
    }
}

fn probe_version(bin: &Path, args: &[String], regex: &str) -> Option<String> {
    let out = crate::spawn::silent(std::process::Command::new(bin))
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let re = regex::Regex::new(regex).ok()?;
    re.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

// ---------------------------------------------------------------- 版本比较

/// 把版本串拆成数字段：`v9.17.0` → [9, 17, 0]，`7.1.2-34` → [7, 1, 2, 34]。
fn version_segments(s: &str) -> Vec<u64> {
    s.trim()
        .trim_start_matches(['v', 'V', 'n', 'N'])
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// 数值化比较版本号（逐段比较，缺失段按 0 处理）。
///
/// 不比较字符串：`"9.0.2" > "10.0"` 这种字符串排序会给出错误结论。
pub fn version_cmp(a: &str, b: &str) -> Ordering {
    let (pa, pb) = (version_segments(a), version_segments(b));
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// 从响应文本里按正则抓版本号；`pick_max` 时取最大者（用于会列出多个分支的目录页）。
pub fn pick_version(text: &str, regex: &str, pick_max: bool) -> Option<String> {
    let re = regex::Regex::new(regex).ok()?;
    let mut found: Vec<String> = re
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    if found.is_empty() {
        return None;
    }
    if pick_max {
        found.sort_by(|a, b| version_cmp(b, a));
    }
    Some(found.remove(0))
}

/// 抓取版本信息文本：带 UA、超时、只读前 512KB。
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
    } else {
        req = req.header("Accept", "text/html,text/plain;q=0.9,*/*;q=0.8");
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "检查超时（12 秒无响应），请检查网络或代理".to_string()
        } else {
            format!("无法连接版本服务器：{e}")
        }
    })?;

    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            403 | 429 => "版本接口限流或拒绝访问，请稍后再试".to_string(),
            404 => "版本接口地址不存在（可能已改版）".to_string(),
            c => format!("版本服务器返回 HTTP {c}"),
        });
    }

    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取版本信息失败：{e}"))?;
        let remain = MAX_VERSION_BODY.saturating_sub(buf.len());
        if remain == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..chunk.len().min(remain)]);
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

async fn download_to_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    cb: &ProgressCb,
) -> Result<(), String> {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败：{}", e))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败，HTTP 状态 {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断：{}", e))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let p = (downloaded as f32 / total as f32).clamp(0.0, 0.99);
            if downloaded - last_emit > total / 20 || p > 0.99 {
                last_emit = downloaded;
                let _ = app.emit(
                    "engine:progress",
                    serde_json::json!({ "id": dest, "phase": "download", "progress": p }),
                );
                cb("download", p, &format!("下载中 {:.0}%", p * 100.0));
            }
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

fn extract(archive: &Path, dest: &Path, kind: &str) -> Result<(), String> {
    match kind {
        "zip" => extract_zip(archive, dest),
        "tar-gz" => extract_tar(archive, dest, "gz"),
        "tar-xz" => extract_tar(archive, dest, "xz"),
        other => Err(format!("不支持的压缩类型：{}", other)),
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = match entry.enclosed_name() {
            Some(n) => n.to_path_buf(),
            None => continue,
        };
        let out_path = dest.join(&name);
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&out_path);
            continue;
        }
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut f = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract_tar(archive: &Path, dest: &Path, comp: &str) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    if comp == "xz" {
        // xz 没有现成的 Read 包装，先整体解压到临时文件再解 tar
        let tmp = archive.with_extension("dec");
        {
            let inf = std::fs::File::open(archive).map_err(|e| e.to_string())?;
            let outf = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            lzma_rs::xz_decompress(
                &mut std::io::BufReader::new(inf),
                &mut std::io::BufWriter::new(outf),
            )
            .map_err(|e| format!("xz 解压失败：{}", e))?;
        }
        let dec = std::fs::File::open(&tmp).map_err(|e| e.to_string())?;
        let mut ar = tar::Archive::new(dec);
        ar.unpack(dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
    } else {
        let dec = flate2::read::GzDecoder::new(file);
        let mut ar = tar::Archive::new(dec);
        ar.unpack(dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}
