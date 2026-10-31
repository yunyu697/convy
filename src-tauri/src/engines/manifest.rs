//! 引擎清单：描述每个第三方工具在本平台如何下载、解压、探测版本、回退。
//!
//! 设计要点：
//! - 清单可以是代码内置的 `BUILTIN`，也可以被 `<engines_root>/manifest.json` 覆盖，
//!   方便用户自行维护镜像或新增引擎，无需改代码。
//! - 每个平台（os-arch）一套下载源：`url` 为官方原地址，`mirror` 为加速镜像。
//! - `kind` 决定解压方式：`zip` / `tar-gz` / `tar-xz` 会自动下载、解压、启用；
//!   `installer` / `external` 仅提供下载页或把安装包存到目录，由用户手动安装。
//! - `latest` 必须是**真正能返回版本号**的地址（纯文本版本文件 / API JSON / 目录列表）。
//!   早期版本这里全填 null，导致「检查更新」永远返回空、界面上永远显示「已是最新」，
//!   现已逐个改为可用的官方端点，并支持 `pick: max`（响应里有多个版本时取最大）。
//! - `prune` 列出解压后可以安全删除的组件（如 ffplay / doc / presets），
//!   FFmpeg 这类「一包多程序」的发行包因此能省下几十 MB 磁盘。
//! - `sizeHint` 说明安装包体积与「哪些组件才是真正需要的」，直接展示在引擎管理页，
//!   避免用户误以为必须装完整套件。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineManifest {
    pub engines: Vec<EngineDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDef {
    /// 内部标识：ffmpeg / imagemagick / calibre / libreoffice / pandoc
    pub id: String,
    /// 显示名
    pub label: String,
    pub license: String,
    pub description: String,
    /// 首次启动是否尝试自动下载（仅对体积小、直链稳定的核心引擎默认 true）
    pub auto: bool,
    /// 按 "os-arch" 选择下载源，例如 "windows-x86_64"
    pub downloads: HashMap<String, DownloadSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSpec {
    /// 官方原地址
    pub url: String,
    /// 加速镜像（可空）
    pub mirror: Option<String>,
    /// zip | tar-gz | tar-xz | installer | external
    pub kind: String,
    /// 解压后相对可执行文件路径，例如 "bin/ffmpeg.exe"
    pub bin: String,
    /// 版本探测：运行 `bin` + args，用 regex 的第一个捕获组取出版本号
    pub version: VersionProbe,
    /// 可选的「远程最新版本」检测，用于判断是否有更新
    pub latest: Option<LatestCheck>,
    /// installer / external 类型给出官网下载页
    pub homepage: Option<String>,
    /// 解压后要删除的、转换用不到的组件（相对路径），如 ffplay / doc / presets
    #[serde(default)]
    pub prune: Vec<String>,
    /// 安装包体积与必需组件说明，展示在引擎管理页
    #[serde(default)]
    pub size_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionProbe {
    pub args: Vec<String>,
    pub regex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestCheck {
    pub url: String,
    pub regex: String,
    /// 是否允许改用镜像地址检测（镜像通常也提供同样结构的版本文件）
    pub use_mirror: bool,
    /// 一个响应里有多个版本号时的取法：`first`（默认）或 `max`（取最大）。
    /// LibreOffice 的 stable 目录会同时列出多个分支，必须取最大。
    #[serde(default)]
    pub pick: Option<String>,
    /// 该地址只提供说明文字（如滚动构建），不做版本比较
    #[serde(default)]
    pub rolling: bool,
}

impl LatestCheck {
    pub fn pick_max(&self) -> bool {
        self.pick.as_deref() == Some("max")
    }
}

impl EngineManifest {
    /// 优先用用户覆盖的 manifest.json，否则用内置清单。
    pub fn load(root: &Path) -> Self {
        let override_path = root.join("manifest.json");
        if let Ok(text) = std::fs::read_to_string(&override_path) {
            if let Ok(m) = serde_json::from_str::<EngineManifest>(&text) {
                return m;
            }
        }
        serde_json::from_str::<EngineManifest>(BUILTIN).expect("内置清单解析失败")
    }
}

/// 当前平台的 "os-arch" 键。
pub fn platform_key() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "x86"
    };
    format!("{}-{}", os, arch)
}

const BUILTIN: &str = r#"
{
  "engines": [
    {
      "id": "ffmpeg",
      "label": "FFmpeg",
      "license": "LGPL-2.1+ (essentials) / GPL (镜像 full)",
      "description": "音视频转换与硬件加速核心引擎。Windows 用 LGPL essentials 构建以确保合规。",
      "auto": true,
      "downloads": {
        "windows-x86_64": {
          "url": "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
          "mirror": "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
          "kind": "zip",
          "bin": "bin/ffmpeg.exe",
          "version": { "args": ["-version"], "regex": "ffmpeg version ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://www.gyan.dev/ffmpeg/builds/release-version",
            "regex": "([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)",
            "useMirror": false
          },
          "homepage": "https://www.gyan.dev/ffmpeg/builds/",
          "prune": ["bin/ffplay.exe", "doc", "presets"],
          "sizeHint": "下载约 115 MB；转换只需要 bin 下的 ffmpeg.exe + ffprobe.exe，ffplay、doc、presets 解压后会自动清理"
        },
        "darwin-x86_64": {
          "url": "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-macos64-gpl.zip",
          "mirror": null,
          "kind": "zip",
          "bin": "bin/ffmpeg",
          "version": { "args": ["-version"], "regex": "ffmpeg version ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://ffmpeg.org/download.html",
            "regex": "ffmpeg-([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)\\.tar\\.xz",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://ffmpeg.org/download.html",
          "prune": ["bin/ffplay", "doc", "presets"],
          "sizeHint": "下载约 90–200 MB（视构建）；只需要 bin 下的 ffmpeg + ffprobe"
        },
        "darwin-aarch64": {
          "url": "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-macos64-gpl.zip",
          "mirror": null,
          "kind": "zip",
          "bin": "bin/ffmpeg",
          "version": { "args": ["-version"], "regex": "ffmpeg version ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://ffmpeg.org/download.html",
            "regex": "ffmpeg-([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)\\.tar\\.xz",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://ffmpeg.org/download.html",
          "prune": ["bin/ffplay", "doc", "presets"],
          "sizeHint": "下载约 90–200 MB（视构建）；只需要 bin 下的 ffmpeg + ffprobe"
        },
        "linux-x86_64": {
          "url": "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz",
          "mirror": null,
          "kind": "tar-xz",
          "bin": "bin/ffmpeg",
          "version": { "args": ["-version"], "regex": "ffmpeg version ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://ffmpeg.org/download.html",
            "regex": "ffmpeg-([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)\\.tar\\.xz",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://ffmpeg.org/download.html",
          "prune": ["bin/ffplay", "doc", "presets"],
          "sizeHint": "下载约 90–200 MB（视构建）；只需要 bin 下的 ffmpeg + ffprobe"
        },
        "linux-aarch64": {
          "url": "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linuxarm64-gpl.tar.xz",
          "mirror": null,
          "kind": "tar-xz",
          "bin": "bin/ffmpeg",
          "version": { "args": ["-version"], "regex": "ffmpeg version ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://ffmpeg.org/download.html",
            "regex": "ffmpeg-([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)\\.tar\\.xz",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://ffmpeg.org/download.html",
          "prune": ["bin/ffplay", "doc", "presets"],
          "sizeHint": "下载约 90–200 MB（视构建）；只需要 bin 下的 ffmpeg + ffprobe"
        }
      }
    },
    {
      "id": "imagemagick",
      "label": "ImageMagick",
      "license": "ImageMagick (Apache-2.0 类，含特殊条款)",
      "description": "图片转换覆盖最广的引擎（PSD / RAW / 多尺寸 ICO / HEIC）。各平台多为安装器或便携包。",
      "auto": false,
      "downloads": {
        "windows-x86_64": {
          "url": "https://imagemagick.org/archive/binaries/",
          "mirror": null,
          "kind": "external",
          "bin": "magick.exe",
          "version": { "args": ["-version"], "regex": "Version: ImageMagick ([0-9]+(?:\\.[0-9]+)*(?:-[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"([0-9][0-9.]*(?:-[0-9]+)?)\"",
            "useMirror": false
          },
          "homepage": "https://imagemagick.org/script/download.php",
          "sizeHint": "需要用便携包里的完整程序目录（magick.exe + 一组 CORE_RL_*.dll），不能只取单个 exe"
        },
        "darwin-x86_64": {
          "url": "https://imagemagick.org/script/download.php",
          "mirror": null,
          "kind": "external",
          "bin": "magick",
          "version": { "args": ["-version"], "regex": "Version: ImageMagick ([0-9]+(?:\\.[0-9]+)*(?:-[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"([0-9][0-9.]*(?:-[0-9]+)?)\"",
            "useMirror": false
          },
          "homepage": "https://imagemagick.org/script/download.php",
          "sizeHint": "建议用 Homebrew 安装（brew install imagemagick），会一并装好依赖库"
        },
        "darwin-aarch64": {
          "url": "https://imagemagick.org/script/download.php",
          "mirror": null,
          "kind": "external",
          "bin": "magick",
          "version": { "args": ["-version"], "regex": "Version: ImageMagick ([0-9]+(?:\\.[0-9]+)*(?:-[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"([0-9][0-9.]*(?:-[0-9]+)?)\"",
            "useMirror": false
          },
          "homepage": "https://imagemagick.org/script/download.php",
          "sizeHint": "建议用 Homebrew 安装（brew install imagemagick），会一并装好依赖库"
        },
        "linux-x86_64": {
          "url": "https://imagemagick.org/script/download.php",
          "mirror": null,
          "kind": "external",
          "bin": "magick",
          "version": { "args": ["-version"], "regex": "Version: ImageMagick ([0-9]+(?:\\.[0-9]+)*(?:-[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"([0-9][0-9.]*(?:-[0-9]+)?)\"",
            "useMirror": false
          },
          "homepage": "https://imagemagick.org/script/download.php",
          "sizeHint": "用发行版包管理器安装（apt/dnf/pacman install imagemagick）最省事"
        },
        "linux-aarch64": {
          "url": "https://imagemagick.org/script/download.php",
          "mirror": null,
          "kind": "external",
          "bin": "magick",
          "version": { "args": ["-version"], "regex": "Version: ImageMagick ([0-9]+(?:\\.[0-9]+)*(?:-[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/ImageMagick/ImageMagick/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"([0-9][0-9.]*(?:-[0-9]+)?)\"",
            "useMirror": false
          },
          "homepage": "https://imagemagick.org/script/download.php",
          "sizeHint": "用发行版包管理器安装（apt/dnf/pacman install imagemagick）最省事"
        }
      }
    },
    {
      "id": "calibre",
      "label": "Calibre",
      "license": "GPL-3.0",
      "description": "电子书转换引擎（ebook-convert）。以独立进程调用，仅做格式转换，不涉及链接。",
      "auto": false,
      "downloads": {
        "windows-x86_64": {
          "url": "https://calibre-ebook.com/download_windows",
          "mirror": null,
          "kind": "installer",
          "bin": "ebook-convert.exe",
          "version": { "args": ["--version"], "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)" },
          "latest": {
            "url": "https://api.github.com/repos/kovidgoyal/calibre/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://calibre-ebook.com/download",
          "sizeHint": "安装包约 240 MB，内含 Qt 运行库；ebook-convert 离不开整套运行库，无法只取一个 exe"
        },
        "darwin-x86_64": {
          "url": "https://calibre-ebook.com/download_osx",
          "mirror": null,
          "kind": "installer",
          "bin": "ebook-convert",
          "version": { "args": ["--version"], "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)" },
          "latest": {
            "url": "https://api.github.com/repos/kovidgoyal/calibre/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://calibre-ebook.com/download",
          "sizeHint": "dmg 约 200 MB 级，含完整运行库"
        },
        "darwin-aarch64": {
          "url": "https://calibre-ebook.com/download_osx",
          "mirror": null,
          "kind": "installer",
          "bin": "ebook-convert",
          "version": { "args": ["--version"], "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)" },
          "latest": {
            "url": "https://api.github.com/repos/kovidgoyal/calibre/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://calibre-ebook.com/download",
          "sizeHint": "dmg 约 200 MB 级，含完整运行库"
        },
        "linux-x86_64": {
          "url": "https://calibre-ebook.com/download_linux",
          "mirror": null,
          "kind": "installer",
          "bin": "ebook-convert",
          "version": { "args": ["--version"], "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)" },
          "latest": {
            "url": "https://api.github.com/repos/kovidgoyal/calibre/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://calibre-ebook.com/download_linux",
          "sizeHint": "官方脚本会装完整运行库；也可用发行版仓库的 calibre 包"
        },
        "linux-aarch64": {
          "url": "https://calibre-ebook.com/download_linux",
          "mirror": null,
          "kind": "installer",
          "bin": "ebook-convert",
          "version": { "args": ["--version"], "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)" },
          "latest": {
            "url": "https://api.github.com/repos/kovidgoyal/calibre/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://calibre-ebook.com/download_linux",
          "sizeHint": "官方脚本会装完整运行库；也可用发行版仓库的 calibre 包"
        }
      }
    },
    {
      "id": "libreoffice",
      "label": "LibreOffice",
      "license": "MPL-2.0",
      "description": "文档 / 表格 / 演示转换引擎（headless）。每个任务使用独立 profile 避免冲突。",
      "auto": false,
      "downloads": {
        "windows-x86_64": {
          "url": "https://www.libreoffice.org/download/download-libreoffice/",
          "mirror": null,
          "kind": "installer",
          "bin": "soffice.exe",
          "version": { "args": ["--version"], "regex": "LibreOffice ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://download.documentfoundation.org/libreoffice/stable/",
            "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)/",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://www.libreoffice.org/download/download-libreoffice/",
          "sizeHint": "安装包约 375 MB（完整套件，含 Writer/Calc/Impress）；soffice 依赖整个 program 目录，不能只取一个 exe"
        },
        "darwin-x86_64": {
          "url": "https://www.libreoffice.org/download/download-libreoffice/",
          "mirror": null,
          "kind": "installer",
          "bin": "soffice",
          "version": { "args": ["--version"], "regex": "LibreOffice ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://download.documentfoundation.org/libreoffice/stable/",
            "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)/",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://www.libreoffice.org/download/download-libreoffice/",
          "sizeHint": "dmg 约 300 MB 级，含完整套件"
        },
        "darwin-aarch64": {
          "url": "https://www.libreoffice.org/download/download-libreoffice/",
          "mirror": null,
          "kind": "installer",
          "bin": "soffice",
          "version": { "args": ["--version"], "regex": "LibreOffice ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://download.documentfoundation.org/libreoffice/stable/",
            "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)/",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://www.libreoffice.org/download/download-libreoffice/",
          "sizeHint": "dmg 约 300 MB 级，含完整套件"
        },
        "linux-x86_64": {
          "url": "https://www.libreoffice.org/download/download-libreoffice/",
          "mirror": null,
          "kind": "installer",
          "bin": "soffice",
          "version": { "args": ["--version"], "regex": "LibreOffice ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://download.documentfoundation.org/libreoffice/stable/",
            "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)/",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://www.libreoffice.org/download/download-libreoffice/",
          "sizeHint": "deb/rpm 约 250 MB；建议只装 libreoffice-core + libreoffice-writer 以精简体积"
        },
        "linux-aarch64": {
          "url": "https://www.libreoffice.org/download/download-libreoffice/",
          "mirror": null,
          "kind": "installer",
          "bin": "soffice",
          "version": { "args": ["--version"], "regex": "LibreOffice ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://download.documentfoundation.org/libreoffice/stable/",
            "regex": "([0-9]+\\.[0-9]+\\.[0-9]+)/",
            "useMirror": false,
            "pick": "max"
          },
          "homepage": "https://www.libreoffice.org/download/download-libreoffice/",
          "sizeHint": "deb/rpm 约 250 MB；建议只装 libreoffice-core + libreoffice-writer 以精简体积"
        }
      }
    },
    {
      "id": "pandoc",
      "label": "Pandoc",
      "license": "GPL-2.0+",
      "description": "Markdown / HTML 转 Word / EPUB / PDF 的高保真排版引擎。",
      "auto": false,
      "downloads": {
        "windows-x86_64": {
          "url": "https://pandoc.org/installing.html",
          "mirror": null,
          "kind": "installer",
          "bin": "pandoc.exe",
          "version": { "args": ["--version"], "regex": "pandoc(?:[a-z ]*)? ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/jgm/pandoc/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://pandoc.org/installing.html",
          "sizeHint": "安装包约 43 MB，里面就是一个静态 exe，已经是最小形态"
        },
        "darwin-x86_64": {
          "url": "https://pandoc.org/installing.html",
          "mirror": null,
          "kind": "installer",
          "bin": "pandoc",
          "version": { "args": ["--version"], "regex": "pandoc(?:[a-z ]*)? ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/jgm/pandoc/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://pandoc.org/installing.html",
          "sizeHint": "单文件静态可执行，pkg 约 40 MB 级"
        },
        "darwin-aarch64": {
          "url": "https://pandoc.org/installing.html",
          "mirror": null,
          "kind": "installer",
          "bin": "pandoc",
          "version": { "args": ["--version"], "regex": "pandoc(?:[a-z ]*)? ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/jgm/pandoc/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://pandoc.org/installing.html",
          "sizeHint": "单文件静态可执行，pkg 约 40 MB 级"
        },
        "linux-x86_64": {
          "url": "https://pandoc.org/installing.html",
          "mirror": null,
          "kind": "installer",
          "bin": "pandoc",
          "version": { "args": ["--version"], "regex": "pandoc(?:[a-z ]*)? ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/jgm/pandoc/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://pandoc.org/installing.html",
          "sizeHint": "单文件静态可执行，deb/tar 约 30 MB 级"
        },
        "linux-aarch64": {
          "url": "https://pandoc.org/installing.html",
          "mirror": null,
          "kind": "installer",
          "bin": "pandoc",
          "version": { "args": ["--version"], "regex": "pandoc(?:[a-z ]*)? ([0-9]+\\.[0-9]+(?:\\.[0-9]+)?)" },
          "latest": {
            "url": "https://api.github.com/repos/jgm/pandoc/releases/latest",
            "regex": "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"",
            "useMirror": false
          },
          "homepage": "https://pandoc.org/installing.html",
          "sizeHint": "单文件静态可执行，deb/tar 约 30 MB 级"
        }
      }
    }
  ]
}
"#;
