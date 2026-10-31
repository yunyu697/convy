// 与 Rust 后端一一对应的类型定义。
// 后端用 serde 的 camelCase / kebab-case / lowercase 序列化，这里严格对齐。

export type JobStatus = "pending" | "running" | "done" | "failed" | "canceled";
export type Domain = "video" | "audio" | "image" | "ebook" | "document";
export type Quality = "high" | "balanced" | "small";
export type ResolutionMode =
  | "original"
  | "r2160p"
  | "r1080p"
  | "r720p"
  | "r480p"
  | "custom";
export type HwMode = "auto" | "force" | "off";

export interface Job {
  id: string;
  source: string;
  fileName: string;
  size: number;
  domain: Domain | null;
  sourceExt: string;
  targetFormat: string;
  /** 导入时相对根目录的子目录（如 "photos/2024"），仅文件夹批量导入才有 */
  relDir: string | null;
  status: JobStatus;
  progress: number;
  detail: string | null;
  outputPath: string | null;
  /** 转换完成后的文件大小（字节） */
  outputSize: number | null;
  error: string | null;
  startedAt: number | null;
  finishedAt: number | null;
}

export interface EngineReport {
  ffmpeg: boolean;
  ffprobe: boolean;
  image: string | null;
  ebook: boolean;
  document: boolean;
  pandoc: boolean;
  h264Encoder: string;
  hardwareAccel: boolean;
  warnings: string[];
}

// ------------------------------------------------------------ 设置模型

export interface VideoSettings {
  quality: Quality;
  resolution: ResolutionMode;
  customWidth: number;
  customHeight: number;
  videoCodec: string;
  videoBitrate: number;
  crf: number;
  /** 软件编码器速度档：ultrafast…veryslow */
  speed: string;
  /** 旋转：none / 90 / 180 / 270（顺时针） */
  rotation: string;
  flipH: boolean;
  flipV: boolean;
  audioCodec: string;
  audioBitrate: number;
  /** 0 = 保持源采样率 */
  audioSampleRate: number;
  /** 0 = 保持源声道数 */
  audioChannels: number;
  /** 音轨响度归一（与音量增益互斥，归一优先） */
  audioNormalize: boolean;
  /** 音轨音量增益（dB），0 = 不调整 */
  volumeDb: number;
  fps: string;
  hardware: HwMode;
  encoder: string;
  faststart: boolean;
  deinterlace: boolean;
  /**
   * 保持源参数：为 true 时只应用编码器/格式/画面变换，
   * 分辨率、帧率、码率、采样率、声道一律沿用源文件（而非面板默认值）。
   */
  keepSource: boolean;
  /** 新任务默认目标格式；空 = 跟随推荐 */
  defaultTarget: string;
}

export interface AudioSettings {
  quality: Quality;
  codec: string;
  bitrate: number;
  sampleRate: number;
  channels: number;
  normalize: boolean;
  volumeDb: number;
  /**
   * 保持源参数：为 true 时只应用编码器/格式，
   * 码率、采样率、声道一律沿用源文件（而非面板默认值）。
   */
  keepSource: boolean;
  defaultTarget: string;
}

export interface ImageSettings {
  quality: Quality;
  width: number;
  height: number;
  keepAspect: boolean;
  strip: boolean;
  rotation: string;
  flipH: boolean;
  flipV: boolean;
  /** 输出 JPG 时拍平透明通道的背景色，空 = white */
  background: string;
  jpgQuality: number;
  pngLevel: number;
  webpQuality: number;
  avifQuality: number;
  defaultTarget: string;
}

export interface EbookSettings {
  quality: Quality;
  profile: string;
  baseFontSize: number;
  lineHeight: number;
  jpegQuality: number;
  insertToc: boolean;
  margin: number;
  title: string;
  author: string;
  fontFamily: string;
  justify: boolean;
  defaultTarget: string;
}

export interface DocumentSettings {
  via: string;
  pdfEngine: string;
  toc: boolean;
  defaultTarget: string;
}

export interface GlobalSettings {
  theme: string;
  accent: string;
  concurrentJobs: number;
  outputFolder: string;
  autoCheckUpdate: boolean;
  overwrite: boolean;
  /** 批量导入文件夹时，在输出目录下还原原有子目录层级 */
  preserveTree: boolean;
}

export interface Settings {
  video: VideoSettings;
  audio: AudioSettings;
  image: ImageSettings;
  ebook: EbookSettings;
  document: DocumentSettings;
  global: GlobalSettings;
}

/**
 * 前端内置的默认设置，与后端 Rust `Settings::default()` 逐字段对齐。
 * 打包后的 exe 若 IPC / 设置加载失败（例如 WebView2 初始化异常），
 * 用它兜底渲染界面，避免永远停在启动 spinner（表现为“白屏”）。
 */
export const DEFAULT_SETTINGS: Settings = {
  video: {
    quality: "balanced",
    resolution: "original",
    customWidth: 1920,
    customHeight: 1080,
    videoCodec: "auto",
    videoBitrate: 0,
    crf: 23,
    speed: "medium",
    rotation: "none",
    flipH: false,
    flipV: false,
    audioCodec: "auto",
    audioBitrate: 128,
    audioSampleRate: 0,
    audioChannels: 0,
    audioNormalize: false,
    volumeDb: 0,
    fps: "source",
    hardware: "auto",
    encoder: "auto",
    faststart: true,
    deinterlace: false,
    keepSource: true,
    defaultTarget: "",
  },
  audio: {
    quality: "balanced",
    codec: "auto",
    bitrate: 192,
    sampleRate: 0,
    channels: 0,
    normalize: false,
    volumeDb: 0,
    keepSource: true,
    defaultTarget: "",
  },
  image: {
    quality: "balanced",
    width: 0,
    height: 0,
    keepAspect: true,
    strip: true,
    rotation: "none",
    flipH: false,
    flipV: false,
    background: "",
    jpgQuality: 85,
    pngLevel: 9,
    webpQuality: 82,
    avifQuality: 55,
    defaultTarget: "",
  },
  ebook: {
    quality: "balanced",
    profile: "generic",
    baseFontSize: 12,
    lineHeight: 1.4,
    jpegQuality: 80,
    insertToc: true,
    margin: 15,
    title: "",
    author: "",
    fontFamily: "",
    justify: true,
    defaultTarget: "",
  },
  document: {
    via: "auto",
    pdfEngine: "weasyprint",
    toc: false,
    defaultTarget: "",
  },
  global: {
    theme: "light",
    accent: "blue",
    concurrentJobs: 2,
    outputFolder: "",
    autoCheckUpdate: true,
    overwrite: false,
    preserveTree: true,
  },
};

/** add_files 的返回：新任务 + 文件夹扫描统计 */
export interface AddResult {
  jobs: Job[];
  files: number;
  dirs: number;
  unsupported: number;
  hidden: number;
  truncated: boolean;
}

export interface Preset {
  name: string;
  domain: Domain;
  settings: Record<string, unknown>;
  /** 内置预设不可删除 */
  builtin: boolean;
}

// ------------------------------------------------------------ 引擎管理

export interface MirrorDef {
  name: string;
  url: string;
  /** true = 清单内置；false = 用户添加（可删除） */
  builtin: boolean;
}

export interface MirrorTest {
  ok: boolean;
  latencyMs: number | null;
  message: string;
}

export interface EngineStatus {
  id: string;
  label: string;
  license: string;
  description: string;
  /** 本平台有可自动托管的下载源（zip/tar 类） */
  managed: boolean;
  /** PATH / 系统安装里能找到 */
  installed: boolean;
  /** 来自系统 PATH 而非本程序托管目录 */
  system: boolean;
  activeVersion: string | null;
  availableVersions: string[];
  latestRemote: string | null;
  updateAvailable: boolean;
  kind: string;
  url: string | null;
  /** 全部可选下载源：内置镜像 + 用户添加的镜像 */
  mirrors: MirrorDef[];
  homepage: string | null;
  binPath: string | null;
  /** 安装包体积与必需组件说明 */
  sizeHint: string | null;
}

/** 一次版本检查的结果 */
export interface EngineCheck {
  id: string;
  /** 本机当前版本 */
  current: string | null;
  /** 远程最新版本 */
  latest: string | null;
  updateAvailable: boolean;
  /** 检查失败原因（网络 / 限流 / 地址变更） */
  error: string | null;
  /** 本次使用的版本地址 */
  source: string | null;
  checkedAt: number;
}

/** 引擎自检里某一项的结论 */
export interface SelfCheckItem {
  domain: Domain;
  label: string;
  ok: boolean;
  message: string;
  detail: string | null;
  elapsedMs: number;
  /** 缺少引擎而跳过（不算失败） */
  skipped: boolean;
}

/** 一条转换历史 */
export interface HistoryEntry {
  id: string;
  source: string;
  fileName: string;
  domain: Domain | null;
  targetFormat: string;
  status: JobStatus;
  inputSize: number;
  outputSize: number | null;
  outputPath: string | null;
  durationMs: number;
  error: string | null;
  finishedAt: number;
}

export interface EngineUpdateResult {
  id: string;
  version: string | null;
  savedPath: string | null;
  homepage: string | null;
  installed: boolean;
}

export interface EngineProgress {
  id: string;
  phase: string;
  progress: number;
  message: string;
}

// ------------------------------------------------------------ 主题

export interface ThemeDef {
  id: string;
  label: string;
  /** 深色主题下文字反白 */
  dark: boolean;
  swatch: [string, string, string];
}

export const THEMES: ThemeDef[] = [
  { id: "light", label: "晨曦", dark: false, swatch: ["#ffffff", "#f4f5f7", "#185fa5"] },
  { id: "dark", label: "暗夜", dark: true, swatch: ["#1a1b1e", "#242629", "#78a9ff"] },
  { id: "midnight", label: "深海", dark: true, swatch: ["#0f1420", "#161d2e", "#6ee7c8"] },
  { id: "solar", label: "暖阳", dark: false, swatch: ["#fffbf3", "#f6efe3", "#c2410c"] },
  { id: "forest", label: "松林", dark: false, swatch: ["#f6faf7", "#e9f2ec", "#15803d"] },
  { id: "rose", label: "胭脂", dark: false, swatch: ["#fff7f8", "#fbeaec", "#be123c"] },
  { id: "grape", label: "藤紫", dark: false, swatch: ["#faf7ff", "#f0eafb", "#6d28d9"] },
  { id: "sand", label: "素沙", dark: false, swatch: ["#faf9f7", "#f0eeea", "#57534e"] },
];

export interface AccentDef {
  id: string;
  label: string;
  color: string;
}

export const ACCENTS: AccentDef[] = [
  { id: "blue", label: "靛蓝", color: "#185fa5" },
  { id: "teal", label: "青碧", color: "#0f766e" },
  { id: "violet", label: "藤紫", color: "#6d28d9" },
  { id: "amber", label: "琥珀", color: "#b45309" },
  { id: "rose", label: "胭脂", color: "#be123c" },
  { id: "emerald", label: "翠绿", color: "#15803d" },
  { id: "cyan", label: "湖蓝", color: "#0e7490" },
  { id: "slate", label: "岩灰", color: "#475569" },
];

// ------------------------------------------------------------ 静态元数据

export const DOMAIN_META: Record<Domain, { label: string; icon: string; desc: string }> = {
  video: { label: "视频", icon: "▶", desc: "转码、压缩、改分辨率与封装格式" },
  audio: { label: "音频", icon: "♪", desc: "提取音轨、转换编码、统一响度" },
  image: { label: "图片", icon: "◈", desc: "格式转换、缩放、压缩与去元数据" },
  ebook: { label: "电子书", icon: "❦", desc: "EPUB / MOBI / AZW3 / PDF 互转与排版" },
  document: { label: "文档", icon: "▤", desc: "Office / Markdown / PDF 跨格式转换" },
};

export const STATUS_META: Record<JobStatus, { label: string; color: string }> = {
  pending: { label: "等待中", color: "#888780" },
  running: { label: "转换中", color: "#185fa5" },
  done: { label: "已完成", color: "#3b6d11" },
  failed: { label: "失败", color: "#a32d2d" },
  canceled: { label: "已取消", color: "#888780" },
};

export const QUALITY_LABEL: Record<Quality, string> = {
  high: "高质量",
  balanced: "均衡",
  small: "小体积",
};

export function formatSize(bytes: number): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function baseName(p: string): string {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

/** 各模块可指定的默认目标格式（供「目标格式」下拉用；空 = 自动跟随推荐） */
export const DOMAIN_TARGETS: Record<Domain, string[]> = {
  video: ["mp4", "mkv", "webm", "mov", "avi", "gif"],
  audio: ["mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma"],
  image: ["jpg", "png", "webp", "avif", "tiff", "bmp", "ico", "gif"],
  ebook: ["epub", "mobi", "azw3", "pdf", "fb2", "txt", "htmlz"],
  document: ["pdf", "docx", "odt", "rtf", "txt", "html", "md", "epub", "csv"],
};

/** 把引擎 id 映射为它能服务的域，用于「引擎缺失」提示。 */
export const ENGINE_DOMAINS: Record<string, Domain[]> = {
  ffmpeg: ["video", "audio"],
  imagemagick: ["image"],
  calibre: ["ebook"],
  libreoffice: ["document"],
  pandoc: ["document"],
};
