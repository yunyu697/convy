# 引擎目录

把要随应用分发的转换工具放到这里，应用启动时会**优先**使用本目录下的可执行文件，
找不到才去系统 PATH 和各平台常见安装路径查找。

## 目录结构

```
engines/
├── ffmpeg.exe          (Windows)
├── ffprobe.exe
├── magick.exe
├── ebook-convert.exe
└── soffice.exe
```

macOS / Linux 直接放同名文件（无扩展名）。

## 许可证提醒

| 工具 | 许可证 | 能否随应用分发 |
|---|---|---|
| FFmpeg（LGPL 构建） | LGPL-2.1+ | 可以，需保留版权声明并提供源码获取方式 |
| FFmpeg（GPL 构建，含 x264/x265） | GPL-2.0+ | **仅当你的应用也以 GPL 兼容方式开源时** |
| ImageMagick | Apache-2.0 | 可以 |
| libvips | LGPL-2.1 | 可以 |
| Calibre | GPL-3.0 | 仅作为独立进程调用；闭源分发建议改为引导用户自行安装 |
| LibreOffice | MPL-2.0 | 可以 |
| Pandoc | GPL-2.0 | 同上，独立进程调用 |

**本项目是免费开源工具，因此可以直接捆绑 GPL 版 FFmpeg。**
如果你之后想闭源分发，请改用 LGPL 构建的 FFmpeg —— 此时 `libx264` 不可用，
应用会自动回退到硬件编码器（NVENC / QSV / AMF / VideoToolbox），这是设计上已经预留的路径。

## 各平台 FFmpeg 下载

- Windows: https://www.gyan.dev/ffmpeg/builds/ （选 `ffmpeg-release-essentials`）
- macOS: `brew install ffmpeg`
- Linux: `apt install ffmpeg` / `dnf install ffmpeg`
