# Convy · 万象转换

跨平台桌面文件格式转换工具，支持**视频 / 音频 / 图片 / 电子书 / 文档**五类格式互转。

免费开源，基于 Tauri 2 + Rust，安装包体积小、启动快。

[![CI](../../actions/workflows/ci.yml/badge.svg)](../../actions/workflows/ci.yml)
[![Release](../../actions/workflows/release.yml/badge.svg)](../../actions/workflows/release.yml)

---

## 首次打开提示

从 [Releases](../../releases/latest) 下载对应平台的安装包后，**第一次打开时系统会拦一下**。
原因是安装包没有购买代码签名证书（Windows 的 EV 证书、Apple Developer 年费对个人项目都是实打实的成本），
不是文件损坏，也不是病毒 —— 源码全部公开，你也可以自己 `npm run app:build` 构建一个。

### Windows

双击安装包，如果出现蓝色窗口「**Windows 已保护你的电脑**」：

1. 点 **更多信息**
2. 点 **仍要运行**

便携版（zip）解压后首次运行 `convy.exe` 同样会被拦一次，处理方式一样。

### macOS

从 dmg 把 Convy 拖进「应用程序」后，双击可能提示：

> 「Convy」已损坏，无法打开。您应该将它移到废纸篓。

这是 Gatekeeper 对**未公证应用**的固定说辞，文件本身是完整的。打开终端执行一次即可：

```bash
xattr -dr com.apple.quarantine /Applications/Convy.app
```

之后正常双击打开。也可以走图形界面：**系统设置 → 隐私与安全性**，在底部找到被拦截的 Convy，点 **仍要打开**。

> macOS 15 (Sequoia) 起，以前"右键 → 打开"的绕过方式已经失效，只能用上面两种之一。

### Linux

- **deb / rpm**：`sudo dpkg -i Convy_*_amd64.deb` 或 `sudo rpm -i Convy-*-1.x86_64.rpm`，装完从应用菜单启动
- **AppImage**：需要先加执行权限

  ```bash
  chmod +x Convy_*_amd64.AppImage
  ./Convy_*_amd64.AppImage
  ```

  如果报 `error while loading shared libraries: libfuse.so.2`，装一下 FUSE
  （`sudo apt install libfuse2`），或改用 `--appimage-extract-and-run` 参数启动

- **裸二进制（tar.gz）**：解压后直接运行 `./convy`，需要系统已装 `libwebkit2gtk-4.1`

## 技术栈

| 层 | 选型 |
|---|---|
| 应用外壳 | Tauri 2（系统 WebView，安装包 ~10MB 不含引擎） |
| 前端 | React 18 + TypeScript + Vite |
| 核心逻辑 | Rust（任务队列 / 引擎路由 / 进程管理 / 进度聚合 / 引擎托管） |
| 转换引擎 | FFmpeg、ImageMagick、Calibre、LibreOffice、Pandoc |

所有转换引擎都以**独立子进程**方式调用，通过命令行交互，不做任何链接或嵌入。这带来两个好处：

1. 引擎崩溃不会带崩主程序
2. 规避了 GPL 工具（Calibre / Pandoc）的传染性问题 —— 进程边界属于 aggregation

## 架构

```
┌─ Tauri 主进程 ────────────────────────────┐
│  WebView（React）                          │
│       ↕ invoke / event                     │
│  Rust Core                                 │
│    任务队列 · 引擎路由 · 引擎托管 · 进度聚合 │
└───────────────┬───────────────────────────┘
                │ spawn + pipe
┌───────────────┴───────────────────────────┐
│  sidecar 引擎（独立进程）                   │
│  ffmpeg · magick · ebook-convert           │
│  soffice · pandoc                          │
└───────────────────────────────────────────┘
```

## 开发

```bash
npm install
npm run app:dev      # 开发模式（热更新）
npm run app:build    # 打包安装程序
```

需要本机已安装 Rust 工具链和 Node 18+。

提交前建议本地跑一遍 CI 的等价命令，避免推上去才变红：

```bash
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all

cd ..
npm run build        # tsc --noEmit && vite build
npm test             # 前端单元测试（vitest）
```

## CI / 发版

两个 workflow，日常提交只跑快的那个：

| 文件 | 触发 | 内容 |
|---|---|---|
| `.github/workflows/ci.yml` | 每次 push / PR | `rustfmt`、`clippy -D warnings`、`cargo test`、前端 `tsc` + `vite build`，外加 macOS / Linux 上的 `cargo check` |
| `.github/workflows/release.yml` | 推 `v*` tag | 四台机器并行打包，收齐产物后发一个 GitHub Release |

测试那个 job 跑在 `windows-latest`：`src-tauri` 里有 `#[cfg(windows)]` 分支（`import.rs` 的符号链接防护、
`lib.rs` 的路径处理），在 Linux 上跑覆盖不到。跨平台能不能编译由单独的 `cargo check` 矩阵守着。

打包矩阵：

| 平台 | 产物 |
|---|---|
| Windows | `.msi` + `.exe`（NSIS）+ `Convy-portable-win64.zip` 便携版 |
| macOS | `.dmg` ×2（arm64 原生 + x86_64 交叉编译） |
| Linux | `.deb` + `.rpm` + `.AppImage` + 裸二进制 `tar.gz` |

发版三步：

```bash
# 1. 四处版本号同步（缺一处会导致安装包内版本与应用自报版本不一致）
#    package.json / src-tauri/tauri.conf.json / src-tauri/Cargo.toml / src-tauri/Cargo.lock
# 2. 在 CHANGELOG.md 顶部加一节 "## x.y.z — 日期"
# 3. 提交后打 tag
git tag v1.2.1 && git push origin v1.2.1
```

> Release 正文取自 `CHANGELOG.md` 里与 tag 对应的那一节 —— 应用里「设置 → 更新日志」显示的就是它，
> 所以这段是给用户读的，不是给开发者读的。`CHANGELOG.md` 里找不到对应版本时才退回 GitHub 自动生成的说明，
> 而这个仓库直接往 master 提交、没有 PR，自动生成的结果往往只有一行 Full Changelog 链接。

> 端到端测试在系统缺少 ffmpeg / ImageMagick 等引擎时会自动跳过，所以 CI 环境不装这些工具也是绿的。
> 想手动验证真实转换链路用 `cargo test --test convert -- --ignored --nocapture live_selfcheck`。

**开发期不需要预先配置引擎** —— 程序会自动从系统 PATH 里找 FFmpeg / ImageMagick / Calibre / LibreOffice。只要装了其中之一，对应类别的转换就能用。也可以直接在「引擎管理」页一键下载托管引擎。

想先看界面再跑起来？打开 `ui-preview.html`（浏览器直接打开，右下角可切换主题）。

## 功能模块

界面按操作逻辑分为八个模块，左侧边栏切换：

| 模块 | 说明 |
|---|---|
| 视频转换 | 目标格式、分辨率、编码器、CRF/CBR 码率、编码速度、帧率、旋转、镜像翻转、去隔行、硬件加速；音轨的编码器/码率/采样率/声道/响度归一/音量增益；faststart |
| 音频转换 | 目标格式、编码器、码率、采样率、声道、响度归一、音量增益 |
| 图片转换 | 目标格式、缩放、等比、旋转、镜像翻转、按格式的压缩质量（JPG/PNG/WebP/AVIF）、EXIF 清除、透明通道拍平背景色 |
| 电子书转换 | 目标格式、设备画像（Kindle 等）、字号、行距、页边距、嵌入字体、两端对齐、书名/作者元数据、目录、图片质量 |
| 文档转换 | 目标格式、Pandoc / LibreOffice 通道选择、PDF 排版引擎、自动生成目录 |
| 引擎管理 | 引擎下载、更新、镜像加速、版本回退、**引擎自检** |
| 转换历史 | 最近 500 条转换记录：体积变化、耗时、失败原因，支持「再转一次」 |
| 全局设置 | 主题、强调色、并发数、输出目录、覆盖策略、保持目录结构、更新策略 |

### 引擎自检

「装了」不等于「能用」——版本不兼容、缺运行库、编码器没编进去都会表现为转换失败。
引擎页的「引擎自检」会**在本地生成极小素材**（不联网、不需要你提供文件），
走与正式转换完全相同的 `plan_for` + `runner::execute` 链路跑一遍：

| 类别 | 素材来源 | 试跑内容 |
|---|---|---|
| 视频 | ffmpeg lavfi 生成 1 秒 160×120 | mp4 → mkv |
| 音频 | 直接写 0.3 秒 WAV（44 字节头 + 静音） | wav → mp3 |
| 图片 | 手写 16×16 24 位 BMP | bmp → jpg（magick → vips → ffmpeg 回退） |
| 电子书 | 极小 HTML | html → epub |
| 文档 | 极小 Markdown（只有 LibreOffice 时用 HTML） | md → docx |

通过项显示耗时；失败项直接列出引擎 stderr 的尾部，缺引擎的类别标记为「跳过」而不是失败。

### 失败重试与转换历史

- 失败 / 取消的任务行上有「重试」，原地重新入队，不用删了重加
- 每次任务结束（成功 / 失败 / 取消）都会追加一条到 `history.jsonl`（JSONL 追加是 O(1)，
  断电最多损失最后一行；超过 500 条自动裁剪最旧的）
- 历史页可按全部 / 成功 / 失败筛选，看到体积变化、耗时、失败原因，支持「再转一次」

#### 失败原因会翻译成人话

直接把 ffmpeg 的 stderr 甩给用户没有意义 —— `Exactly one MP3 audio stream is required`
这种话，看的人既不知道发生了什么，也不知道下一步点哪里。所以失败时会先过一层映射
（`src-tauri/src/errors.rs`），把常见错误换成中文说明 + 一句可执行的建议：

| 引擎原文 | 界面上看到的 |
|---|---|
| `Exactly one MP3 audio stream is required` | 目标格式只支持单条音轨，但源文件里有多条。请指定保留哪一条音轨，或改选支持多音轨的容器（如 mkv） |
| `moov atom not found` | MP4 缺少索引（moov），通常是录制或下载中断造成的。这个文件需要先修复才能转换 |
| `No space left on device` | 磁盘空间不足。清理输出目录所在的磁盘，或者换一个输出位置 |
| `Could not find tag for codec …` | 目标容器不支持所选的编码器。换一个容器（例如 mp4 → mkv），或者把视频编码改回自动 |

**原文一律附在说明后面**（`引擎原文：…`），既让普通用户看懂，也不让开发者失去线索。
未命中任何规则的错误保留原文，不做猜测。

### 稳定性

- **单实例**：重复启动不会开第二个窗口抢同一份 `settings.json`，而是把已有窗口叫到前面来
- **崩溃兜底**：界面渲染异常时显示一张能读懂的错误页（而不是白屏），提供「重新加载界面」
  和「打开日志」两个出口；任务队列在后端，重新加载不会丢任务
- **状态锁容错**：后台线程 panic 后不会让整个应用「点什么都没反应」——
  互斥锁中毒时恢复使用而不是继续崩

### 批量导入

文件和文件夹都能拖进窗口任意位置，也可以点「文件夹」按钮挑选目录：

- **递归扫描**：文件夹里所有受支持格式的文件一次性入队，子目录层级会被记录
- **保持目录结构**：开启后（设置页可切换）输出会在输出目录下还原原有子目录，避免几百个同名文件挤在一层；未设输出目录时按原目录就地输出
- **去重与防护**：同一文件重复拖入不会重复入队；隐藏 / 系统项与符号链接跳过；单次最多 3000 个文件、20 层目录，超出会在界面提示已截断
- 每个任务行会标注它来自哪个子目录，方便核对来源

### 预设系统

每个转换模块顶部有一排预设芯片：

- **内置预设**：覆盖常见场景（如"网页高清 1080p"、"Kindle 适配"、"WebP 高压缩"）
- **自定义预设**：调好参数后点「＋ 保存当前」命名保存，可随时删除
- 点击芯片即整体套用该域的参数快照

预设与设置都持久化在应用数据目录（`settings.json` / `presets.json`），字段带默认值兜底，旧配置缺字段不会崩。

### 主题系统

8 套主题（晨曦 / 暗夜 / 深海 / 暖阳 / 松林 / 胭脂 / 藤紫 / 素沙）× 8 种强调色，右上角下拉即点即换，全局设置页也可以换。主题用 CSS 变量实现，跟随 `data-theme` 属性切换。

### 应用更新

设置页「更新」一块，三个按钮对应三种心态：

| 按钮 | 做什么 | 什么时候用 |
|---|---|---|
| 更新 | 下载 + 立刻安装 | 想直接升上去 |
| 仅下载 | 只落盘，把路径给出来，留一个「安装」按钮 | 想先看清装的是什么 |
| 打开下载页面 | 只开浏览器到 Release 页 | 不想让应用碰安装包 |

- **检查频率**：每次启动 / 每天 / 每周 / 每月 / 从不，默认每天。是否到期由后端判定
  （`updater::is_due`），界面上的开关只改偏好 —— 反复进出设置页不会每次都联网
- **更新日志**：设置页直接渲染最近一次 Release 的正文。渲染器是手写的极简 Markdown
  （标题 / 列表 / 行内码 / 加粗），**不注入 HTML**：Release 正文是外部内容，
  不值得为它开一个 XSS 口子
- **跳过此版本**：记在 `skippedVersion` 里，之后启动不再提示，但「更新」按钮仍在
- **镜像**：下载同样走 `apply_prefix`，GitHub 直连不通时可换镜像加速，测速与引擎管理页共用一套逻辑

平台差异不假装统一，后端只回 `mode`，措辞交给前端：

| 平台 | 安装包 | 做法 | mode |
|---|---|---|---|
| Windows | `.exe`（NSIS） | `/S` 静默装 | `silent` |
| Windows | `.msi` | `msiexec /qn /norestart` | `silent` |
| macOS | `.dmg` | 挂载后交给 Finder | `opened` |
| Linux | `.deb` / `.rpm` | 交给系统包管理器（需要 root） | `opened` |

静默安装时安装器要替换正在运行的 exe，所以应用必须让位：延迟 1.5 秒（够前端把
「即将退出」渲染出来）再 `app.exit(0)`。提示文案在前端生成而不是后端 —— 后端只回
`mode`，不回中文句子，否则界面切成英 / 日 / 西之后这句提示还是中文。

### 多语言

6 个选项：系统语言 / 简体中文 / 繁體中文 / English / 日本語 / Español，默认「系统语言」。
系统语言不在支持范围内时落到**英语**，而不是留在中文。

- 归一化规则：`zh-TW` / `zh-HK` / `zh-MO` → 繁中，其余 `zh-*` → 简中，`en-*` / `ja-*` / `es-*` 按主干匹配
- **key 就是中文原文**（gettext 的做法），不是 `settings.update.title` 这种命名 key。
  四百多条文案不用逐个起名；改文案时旧 key 失效会自动回退成新文案本身，
  漏翻的地方显示中文而不是显示 key 或者空白 —— 坏也坏得看得懂
- 组件里用 `useT()`（订阅语言变化，切换后立即重渲染），非组件代码直接 `t()`
- 编解码器、分辨率、采样率、格式名与品牌名（`H.264`、`Full HD · 1920×1080`、`LibreOffice`）
  故意不进字典，查不到就回退成自己
- 四份字典由测试守着：key 集合必须一致、占位符必须逐字对应
  （漏一个 `{n}` 界面上就会露出字面量）

### 引擎管理

- **托管型引擎**（FFmpeg）：一键下载官方压缩包 → 自动解压到应用数据目录 → 自动探测启用，全程无需手动操作
- **安装器型引擎**（ImageMagick / Calibre / LibreOffice / Pandoc）：提供官网入口与安装包存放目录
- **原地址 / 加速镜像**：每个引擎可勾选「使用加速镜像」，两个地址都可一键在浏览器打开
- **断点续传**：下载中断后再点一次会从断点继续，而不是从 0 重来。官方源与镜像都支持 `Range`；
  换了下载源（官方 ↔ 镜像）则自动重下 —— 两个源放的往往不是同一个包，拼接只会得到校验不过的垃圾
- **版本回退**：托管引擎保留历史版本，「版本回退 ▾」菜单里可切回任意已安装版本
- **更新检查**：走各引擎**真正的官方版本接口**，逐项给出「当前版本 / 远程最新 / 是否有更新」，
  失败会直接显示原因（超时、限流、地址变更），不会静默显示「已是最新」

#### 下载完整性校验

引擎包下载完、**解压之前**会做一次 SHA-256 校验（`src-tauri/src/engines/integrity.rs`）。
这不是可选项：包解压后立刻要跑里面的 `ffmpeg.exe` 探测版本，那一步已经是执行外部代码了。

信任模型只有一条：**哈希必须来自与文件不同的通道**。

| 通道 | 可信度 | 用途 |
|---|---|---|
| 加速镜像 | 不可信 | 只用来传**文件**（快） |
| 官方直连 | 可信 | 只用来取**哈希**（慢，但只有几十字节） |

如果哈希也从镜像取，攻击者同时替换文件和哈希就能通过校验，等于没校验。
所以清单里的 `sha256Url` 一律直连，绝不套镜像前缀。

校验结果会如实显示在界面上：**已校验 SHA-256** / **未校验：原因**。其中两种情况会标注未校验：

- 下载源是镜像。清单里的 `mirror` 往往指向**另一个构建**（FFmpeg 官方是 gyan.dev，
  镜像是 BtbN 的 GPL 构建），哈希天然不同，拿官方哈希去比只会永远失败
- 清单没提供校验值，或官方哈希地址暂时取不到

标注未校验不会阻止安装（否则国内用户可能完全装不上），但用户能看见这件事。
哈希**不一致**则是硬失败：删除临时文件、中止安装，并同时给出期望值与实际值。

自己维护清单时可以在 `manifest.json` 里用 `sha256` 锁定某个固定版本的包：

```json
{ "sha256": "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba" }
```

引擎清单定义在内置 manifest 中，也可用应用数据目录下的 `manifest.json` 覆盖（自定义镜像或新增引擎不用改代码）。

#### 版本检测端点

| 引擎 | 检测地址 | 解析方式 |
|---|---|---|
| FFmpeg (Windows) | `gyan.dev/ffmpeg/builds/release-version` | 纯文本版本号 |
| FFmpeg (mac/Linux) | `ffmpeg.org/download.html` | 取 `ffmpeg-x.y.z.tar.xz` 里最大的版本 |
| ImageMagick | GitHub Releases API | `tag_name`（如 `7.1.2-34`） |
| Calibre | GitHub Releases API | `tag_name`，去掉 `v` 前缀 |
| LibreOffice | `download.documentfoundation.org/libreoffice/stable/` | 目录页里取最大版本 |
| Pandoc | GitHub Releases API | `tag_name` |

版本比较是**数值逐段比较**（`10.0 > 9.0.2`），不是字符串比较；
抓取带 UA、12 秒超时，且最多只读 512 KB 响应（防止误配成压缩包直链时把整个包拉下来）。

## 引擎体积与必需组件

**单个可执行文件就能用的只有 FFmpeg 和 Pandoc**，其余引擎都自带运行时，无法「只取一个 exe」。
各引擎真实体积与必需组件如下（体积取自官方发行包）：

| 引擎 | 下载体积 | 真正需要的组件 | 能否精简 |
|---|---|---|---|
| FFmpeg | ≈115 MB（Windows essentials） | `bin/ffmpeg.exe` + `bin/ffprobe.exe` | ✅ 解压后自动删掉 `ffplay`、`doc/`、`presets/` |
| Pandoc | ≈43 MB | 一个静态 `pandoc.exe` | — 已是最小形态 |
| ImageMagick | 便携包 / 安装器 | `magick.exe` + 一组 `CORE_RL_*.dll` | ❌ 必须整目录 |
| Calibre | ≈240 MB | `ebook-convert` + 整套 Qt 运行库 | ❌ 无法拆分 |
| LibreOffice | ≈375 MB | `soffice` + 整个 `program/` 目录 | ⚠️ 仅 Linux 可按包拆分（core + writer） |

所以「只装需要的那一类」正确做法是：**按需安装引擎**，而不是拆包 ——
只转图片就只装 ImageMagick，只转文档就只装 Pandoc（43 MB）而不用装整个 LibreOffice（375 MB）。
应用本身不含任何引擎，安装包只有 2–3 MB，缺哪个引擎时对应模块会显示「!」角标。

托管下载的目录结构也在这一版修正：发行包普遍多一层包装目录
（如 `ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe`），解压后会先展平，
再把真实相对路径记进 `.convy-bin`，避免「装完了但探测不到」。

## 引擎部署

发布时把二进制放进 `src-tauri/engines/`，应用会优先使用它（详见该目录的 README）。

> 该目录下除 `README.md` 外的内容都在 `.gitignore` 里：单个引擎动辄上百 MB，
> 一旦提交就再也删不干净。需要随包分发请走 release 流水线，不要直接 `git add`。

最小可用组合：

- **视频/音频**：`ffmpeg` + `ffprobe`
- **图片**：`magick`（ImageMagick 7）或 `vips`
- **电子书**：`ebook-convert`（Calibre）
- **文档**：`soffice`（LibreOffice）或 `pandoc`

## 编码器策略

编码器链按**目标容器**划分，硬件优先、失败自动降级：

| 目标容器 | 编码链（按优先级） |
|---|---|
| mp4 / mov / mkv 等 | VideoToolbox (macOS) / NVENC → QSV → AMF → libx264 |
| webm | libvpx-vp9 → libvpx（VP8） |
| avi | mpeg4 |

硬件编码器不仅快 5-20 倍，还让 LGPL 构建的 FFmpeg（不含 x264）依然能输出 H.264 —— 这是闭源分发时的合规路径。当前使用的编码器会显示在侧边栏底部。

硬件编码失败（如未装 CUDA 运行时）会自动换链上的下一个候选重试，对用户透明。

## 已实现

- [x] 七模块界面布局：左侧导航 + 每模块独立参数面板与文件队列
- [x] 精细化参数：视频 18 项 / 音频 8 项 / 图片 13 项 / 电子书 12 项 / 文档 4 项
- [x] 每模块可指定默认目标格式，新添加的任务自动套用
- [x] 任务完成后显示输出文件大小及与源文件的百分比对比（变小绿色 / 变大橙色）
- [x] 预设系统：内置 15+ 预设 + 用户自定义预设（保存 / 删除 / 套用）
- [x] 8 主题 × 8 强调色，CSS 变量驱动，即点即换
- [x] 界面动效：视图切换、进度流光、任务行滑入、导航指示条、Toast 通知（尊重 prefers-reduced-motion）
- [x] 引擎管理：一键下载 / 自动启用 / 更新检查 / 镜像加速 / 版本回退
- [x] 硬件加速三档模式（自动 / 强制指定编码器 / 关闭）+ 失败自动降级
- [x] 容器感知的编码器选择（webm 强制 VPx，避免 H.264 塞进 webm 被拒）
- [x] 批量任务队列，并发数可调（1-8）
- [x] 实时进度：FFmpeg 走 `-progress pipe:1`，Calibre 解析百分比
- [x] 任务取消（杀整个进程树，不留孤儿进程）
- [x] 输出目录可自定义，重名自动加序号或覆盖
- [x] LibreOffice 并发隔离（每任务独立 profile 目录）
- [x] 首次启动三步向导：选用途 → 装对应引擎 → 转第一个文件（可从设置里重新打开）
- [x] 大队列增量渲染：一次导入 3000 个文件也只挂 80 行，滚动到底自动追加
- [x] 崩溃兜底：渲染异常显示可恢复页面，并可一键打开日志
- [x] 应用内更新：检查频率可调，更新 / 仅下载 / 打开下载页面三种动作，Windows 静默安装
- [x] 更新日志直接渲染 Release 正文（极简 Markdown，不注入 HTML）
- [x] 六语种界面（简中 / 繁中 / 英 / 日 / 西 + 跟随系统），系统语言不支持时落到英语

## 已知限制

- `md → pdf` 依赖 Pandoc + WeasyPrint，未安装时会失败并给出提示
- `pdf → jpg/png` 依赖 Ghostscript（ImageMagick 的 PDF 渲染后端）
- Windows 上 LibreOffice 首次转换会有 2-3 秒冷启动
- 后端返回的错误文本（引擎下载失败、安装失败等）仍是中文，没有跟着界面语言切换
- 镜像地址以引擎清单为准，部分镜像可能随上游变更失效（可在 manifest.json 自行覆盖）

## 排障

**窗口打开但内容全黑 / 全白**

WebView2 的用户数据目录损坏时会出现，删掉它让程序重建即可：

```
%LOCALAPPDATA%\app.convy.desktop\EBWebView
```

**构建产物跑起来一直连 devUrl**

`Cargo.toml` 里的 `custom-protocol` feature 是 Tauri 判断"生产模式"的唯一依据。
手动 `cargo build --release` 时必须显式启用，否则程序会以为自己在开发模式：

```bash
cargo build --release --features custom-protocol
```

用 `npm run app:build`（即 `tauri build`）则不需要关心，CLI 会自动加上。

**开发服务器连不上**

`vite.config.ts` 里显式绑定了 `127.0.0.1`。Windows 上 Node 有时只监听 IPv6 的 `::1`，
而 WebView2 走 IPv4，会导致页面一直空白。`devUrl` 也相应写成 `http://127.0.0.1:1420`。

**某个格式转不了**

侧边栏对应模块会显示「!」警告角标，鼠标悬停看缺哪个引擎；到「引擎管理」页下载启用即可。

**启动日志**

程序会把引擎探测结果写到 `%TEMP%\convy.log`，排查"某个格式转不了"时先看这个文件。

## 测试

```bash
cd src-tauri
cargo test           # 58 个通过（3 个联网用例默认忽略），含 4 条真实 FFmpeg 端到端转换

cd ..
npm test             # 36 个前端测试（vitest + jsdom）
npm run test:watch   # 开发时监听
```

测试覆盖：格式矩阵路由、域间设置隔离（0 值语义、硬件模式透传、旋转/翻转/音量滤镜落参、
loudnorm 与增益互斥、图片旋转与透明拍平）、内置预设校验、
部分 JSON 容错、输出目录解析、引擎误判防护（Windows convert.exe 陷阱）、
以及 mp4→webm / mp4→mp3 / png→jpg 三条真实转换链路。
测试里的转换走与 App 完全一致的编码器降级链（`compute_enc_chain`）。

前端测试盯的是三类容易悄悄回退的行为：

- **队列增量渲染**：3000 条任务首屏只挂 80 行，滚动或点按钮再追加一页。
  以前是 `list.map(...)` 全量挂载，导入刚结束时界面会明显卡一下。
- **目标格式同步**：改模块的「目标格式」只影响待处理任务，
  正在跑和已完成的不动（改了会让「已转换到 mp4」和实际产物对不上）；
  未命中的行复用同一对象引用，避免 memo 的 `JobRow` 整表重渲染。
- **设置兜底合并**：老版本 `settings.json` 缺新字段、字段被手工改坏、
  或后端比前端旧时，入口处补默认值，不让 `undefined` 漏进 `<select>` 和输入框。
- **多语言**：系统语言归一化（繁简分流、不认识的语言落到英语）、占位符替换、
  以及四份词典的一致性 —— key 集合必须相同、每条译文的 `{…}` 必须与原文逐字对应。
  漏翻一个语种、或翻译时吃掉一个 `{n}`，测试会直接报出是哪一条。

测试文件会把界面语言钉在简体中文（`src/__tests__/setup.ts`）：应用默认跟随系统语言，
而 jsdom 的 `navigator.language` 固定是 `en-US`，不钉的话断言的文案会随宿主环境漂，
断言的内容也和组件源码里那句中文对不上号。

```bash
npm test
# ✓ src/__tests__/logic.test.ts        (11 tests)
# ✓ src/__tests__/i18n.test.tsx        (20 tests)
# ✓ src/__tests__/ModuleShell.test.tsx  (5 tests)
```

> 用 vitest 3.x。2.x 的模块缓存在两个 worker 并发时会把同一个缓存文件写崩（Windows 上直接
> `EPERM`），然后**静默丢掉**受影响的测试文件 —— 表现是「只跑了一个文件，还显示通过」。
> 3.x 修掉了这个问题，所以这里不需要任何绕行配置。

## 目录结构

```
convy/
├── src/                       前端
│   ├── App.tsx                主容器 + 事件订阅 + 拖拽 + 主题注入
│   ├── api.ts                 invoke / event 封装
│   ├── types.ts               与 Rust 侧对应的类型 + 主题定义
│   ├── logic.ts               纯逻辑：目标格式同步 / 设置兜底合并（可单测）
│   ├── i18n/                  多语言：index.ts 运行层 + 四份词典（中文原文即 key）
│   ├── __tests__/             前端单元测试（vitest + jsdom）
│   ├── components/
│   │   ├── Sidebar.tsx        左侧导航（含引擎健康角标）
│   │   ├── TopBar.tsx         顶栏 + 主题选择器
│   │   ├── ui.tsx             表单控件（分段/滑块/开关/步进器/折叠组）
│   │   ├── PresetBar.tsx      预设芯片栏
│   │   ├── DropZone.tsx       拖放区
│   │   ├── JobRow.tsx         任务行
│   │   ├── Onboarding.tsx     首次启动三步向导
│   │   └── ErrorBoundary.tsx  渲染崩溃兜底页
│   ├── modules/
│   │   ├── ModuleShell.tsx    转换模块共用骨架（左参数右队列）
│   │   ├── panels.tsx         五个模块的精细化参数面板
│   │   ├── EngineModule.tsx   引擎管理
│   │   ├── UpdateSection.tsx  设置页的「更新」与「更新日志」
│   │   └── SettingsModule.tsx 全局设置
│   └── styles.css             设计系统：8 主题变量 + 布局 + 动效
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs             应用装配：模块声明、AppState、启动
│   │   ├── queue.rs           队列执行：并发调度 / 编码器降级 / 落历史
│   │   ├── commands/          IPC 命令层（每条命令只做校验与状态读写）
│   │   │   ├── queue.rs       导入 / 改目标 / 重试 / 取消 / 开始
│   │   │   ├── engine.rs      引擎报告 / 版本检查 / 自检 / 更新 / 镜像
│   │   │   ├── settings.rs    全局设置 / 预设 / 设置落盘
│   │   │   ├── history.rs     历史读取 / 清空
│   │   │   ├── update.rs      应用更新：检查 / 下载 / 安装 / 镜像测速
│   │   │   └── system.rs      打开文件 / 网址、界面就绪
│   │   ├── formats.rs         格式矩阵（能力真相来源）
│   │   ├── settings.rs        设置模型 / 预设 / ConvOptions 转换
│   │   ├── job.rs             任务模型
│   │   ├── history.rs         转换历史（JSONL 追加 / 裁剪）
│   │   ├── updater.rs         应用自身更新：Release 解析 / 资产挑选 / 安装
│   │   ├── selfcheck.rs       引擎自检：本地素材 + 真实转换链路
│   │   ├── import.rs          导入扫描：文件夹递归 / 去重 / 层级记录
│   │   ├── probe.rs           引擎探测 + 编码器链构建
│   │   ├── runner.rs          进程执行、进度解析、进程树取消
│   │   └── engines/
│   │       ├── manifest.rs    引擎清单（内置 + 用户覆盖）
│   │       ├── registry.rs    引擎托管：下载 / 解压 / 启用 / 回退
│   │       ├── integrity.rs   下载完整性校验（SHA-256 / 信任通道分离）
│   │       ├── layout.rs      发行包展平 / 可执行文件定位 / 裁剪
│   │       ├── ffmpeg.rs      视频 / 音频参数构建
│   │       ├── image.rs       图片三级回退（magick → vips → ffmpeg）
│   │       ├── ebook.rs       Calibre
│   │       └── document.rs    LibreOffice / Pandoc
│   ├── engines/               随包分发的二进制
│   └── tauri.conf.json
├── ui-preview.html            静态界面预览（含主题切换）
├── CHANGELOG.md               更新日志（发版时作为 Release 正文）
└── package.json
```

## 许可证

本项目代码采用 [MIT](LICENSE)。

随应用分发的第三方工具各自遵循其原始许可证，详见 `src-tauri/engines/README.md`。
应用本身不包含任何引擎二进制 —— 所有引擎均由用户在「引擎管理」页按需获取，
或直接复用系统上已安装的版本。
