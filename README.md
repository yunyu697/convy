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
```

## CI / 发版

两个 workflow，日常提交只跑快的那个：

| 文件 | 触发 | 内容 |
|---|---|---|
| `.github/workflows/ci.yml` | 每次 push / PR | `rustfmt` 检查、`clippy -D warnings`、`cargo test`、前端 `tsc` + `vite build` |
| `.github/workflows/release.yml` | 推 `v*` tag | 在 Windows 上编译 NSIS + MSI，上传 artifact 并发布到 GitHub Release |

两个 Rust 相关的 job 都跑在 `windows-latest`：项目实际只发布 NSIS / MSI，而且 `src-tauri` 里有
`#[cfg(windows)]` 分支（`import.rs` 的符号链接防护、`lib.rs` 的路径处理），跑 Linux 覆盖不到，
顺带还省掉了 Tauri 在 Linux 上那一堆 webkit / gtk 系统依赖。

发版只需要：

```bash
git tag v1.0.0 && git push origin v1.0.0
```

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

### 引擎管理

- **托管型引擎**（FFmpeg）：一键下载官方压缩包 → 自动解压到应用数据目录 → 自动探测启用，全程无需手动操作
- **安装器型引擎**（ImageMagick / Calibre / LibreOffice / Pandoc）：提供官网入口与安装包存放目录
- **原地址 / 加速镜像**：每个引擎可勾选「使用加速镜像」，两个地址都可一键在浏览器打开
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

## 已知限制

- `md → pdf` 依赖 Pandoc + WeasyPrint，未安装时会失败并给出提示
- `pdf → jpg/png` 依赖 Ghostscript（ImageMagick 的 PDF 渲染后端）
- Windows 上 LibreOffice 首次转换会有 2-3 秒冷启动
- 暂不支持文件夹递归批量，只处理拖入的文件
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
cargo test           # 14 个测试，含 3 个真实 FFmpeg 端到端转换
```

测试覆盖：格式矩阵路由、域间设置隔离（0 值语义、硬件模式透传、旋转/翻转/音量滤镜落参、
loudnorm 与增益互斥、图片旋转与透明拍平）、内置预设校验、
部分 JSON 容错、输出目录解析、引擎误判防护（Windows convert.exe 陷阱）、
以及 mp4→webm / mp4→mp3 / png→jpg 三条真实转换链路。
测试里的转换走与 App 完全一致的编码器降级链（`compute_enc_chain`）。

## 目录结构

```
convy/
├── src/                       前端
│   ├── App.tsx                主容器 + 事件订阅 + 拖拽 + 主题注入
│   ├── api.ts                 invoke / event 封装
│   ├── types.ts               与 Rust 侧对应的类型 + 主题定义
│   ├── components/
│   │   ├── Sidebar.tsx        左侧导航（含引擎健康角标）
│   │   ├── TopBar.tsx         顶栏 + 主题选择器
│   │   ├── ui.tsx             表单控件（分段/滑块/开关/步进器/折叠组）
│   │   ├── PresetBar.tsx      预设芯片栏
│   │   ├── DropZone.tsx       拖放区
│   │   └── JobRow.tsx         任务行
│   ├── modules/
│   │   ├── ModuleShell.tsx    转换模块共用骨架（左参数右队列）
│   │   ├── panels.tsx         五个模块的精细化参数面板
│   │   ├── EngineModule.tsx   引擎管理
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
│   │   │   └── system.rs      打开文件 / 网址、界面就绪
│   │   ├── formats.rs         格式矩阵（能力真相来源）
│   │   ├── settings.rs        设置模型 / 预设 / ConvOptions 转换
│   │   ├── job.rs             任务模型
│   │   ├── history.rs         转换历史（JSONL 追加 / 裁剪）
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
└── package.json
```

## 许可证

本项目代码采用 [MIT](LICENSE)。

随应用分发的第三方工具各自遵循其原始许可证，详见 `src-tauri/engines/README.md`。
应用本身不包含任何引擎二进制 —— 所有引擎均由用户在「引擎管理」页按需获取，
或直接复用系统上已安装的版本。
