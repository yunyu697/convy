//! 用户设置：每个功能模块的精细化参数，以及常用预设。
//!
//! 这是“精细化设置 + 预设”需求的核心数据模型。前端按模块编辑这些字段，
//! 后端在转换时把对应域的设置转成 `ConvOptions` 交给引擎调度层。
//!
//! 持久化：整个 `Settings` 以 JSON 存到应用数据目录的 `settings.json`，
//! 字段用 `#[serde(default)]`，因此旧配置缺字段时会自动补齐默认值，不会崩。

use crate::formats::Domain;
use crate::job::Quality;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionMode {
    #[default]
    Original,
    R2160p,
    R1080p,
    R720p,
    R480p,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HwMode {
    /// 优先硬件，失败自动降级到软件
    #[default]
    Auto,
    /// 强制使用指定硬件编码器
    Force,
    /// 关闭硬件，只用软件
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoSettings {
    pub quality: Quality,
    pub resolution: ResolutionMode,
    pub custom_width: u32,
    pub custom_height: u32,
    /// 可选视频编码器：auto 时按目标格式自动选（h264/hevc/vp9/mpeg4）
    pub video_codec: String,
    /// 0 表示使用 CRF 模式；非 0 表示恒定码率（kbps）
    pub video_bitrate: u32,
    /// 仅当 video_bitrate==0 时生效
    pub crf: u8,
    /// 编码速度档（软件编码器的 preset）：ultrafast…veryslow
    pub speed: String,
    /// 旋转：none / 90 / 180 / 270（顺时针）
    pub rotation: String,
    pub flip_h: bool,
    pub flip_v: bool,
    pub audio_codec: String,
    pub audio_bitrate: u32,
    /// 0 表示保持源采样率
    pub audio_sample_rate: u32,
    /// 0 表示保持源声道数
    pub audio_channels: u8,
    /// 音轨响度归一（与音量增益互斥，归一优先）
    pub audio_normalize: bool,
    /// 音轨音量增益（dB），0 = 不调整
    pub volume_db: i8,
    /// "source" 或数字（如 "30"）
    pub fps: String,
    pub hardware: HwMode,
    /// 强制硬件编码器时的具体编码器名，如 "h264_nvenc"
    pub encoder: String,
    pub faststart: bool,
    pub deinterlace: bool,
    /// 保持源参数：为 true 时只应用编码器/格式/画面变换，
    /// 分辨率、帧率、码率、采样率、声道一律沿用源文件（而非面板默认值）。
    pub keep_source: bool,
    /// 新任务的默认目标格式；空 = 跟随格式矩阵推荐
    pub default_target: String,
}

impl Default for VideoSettings {
    fn default() -> Self {
        VideoSettings {
            quality: Quality::Balanced,
            resolution: ResolutionMode::Original,
            custom_width: 1920,
            custom_height: 1080,
            video_codec: "auto".into(),
            video_bitrate: 0,
            crf: 23,
            speed: "medium".into(),
            rotation: "none".into(),
            flip_h: false,
            flip_v: false,
            audio_codec: "auto".into(),
            audio_bitrate: 128,
            audio_sample_rate: 0,
            audio_channels: 0,
            audio_normalize: false,
            volume_db: 0,
            fps: "source".into(),
            hardware: HwMode::Auto,
            encoder: "auto".into(),
            faststart: true,
            deinterlace: false,
            keep_source: true,
            default_target: String::new(),
        }
    }
}

impl VideoSettings {
    /// 计算目标分辨率（宽, 高）。Original 或无法解析时返回 None。
    pub fn target_size(&self) -> Option<(u32, u32)> {
        let wh = match self.resolution {
            ResolutionMode::Original => return None,
            ResolutionMode::R2160p => (3840, 2160),
            ResolutionMode::R1080p => (1920, 1080),
            ResolutionMode::R720p => (1280, 720),
            ResolutionMode::R480p => (854, 480),
            ResolutionMode::Custom => {
                if self.custom_width > 0 && self.custom_height > 0 {
                    (self.custom_width, self.custom_height)
                } else {
                    return None;
                }
            }
        };
        Some(wh)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioSettings {
    pub quality: Quality,
    pub codec: String,
    pub bitrate: u32,
    /// 0 表示保持源采样率
    pub sample_rate: u32,
    /// 0 表示保持源声道数
    pub channels: u8,
    pub normalize: bool,
    /// 音量增益（dB），0 = 不调整；与响度归一二选一（归一优先）
    pub volume_db: i8,
    /// 保持源参数：为 true 时只应用编码器/格式，
    /// 码率、采样率、声道一律沿用源文件（而非面板默认值）。
    pub keep_source: bool,
    /// 新任务的默认目标格式
    pub default_target: String,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            quality: Quality::Balanced,
            codec: "auto".into(),
            bitrate: 192,
            sample_rate: 0,
            channels: 0,
            normalize: false,
            volume_db: 0,
            keep_source: true,
            default_target: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageSettings {
    pub quality: Quality,
    /// 0 表示保持原宽度
    pub width: u32,
    /// 0 表示保持原高度（与 width 配合做等比缩放）
    pub height: u32,
    pub keep_aspect: bool,
    pub strip: bool,
    /// 旋转：none / 90 / 180 / 270（顺时针）
    pub rotation: String,
    pub flip_h: bool,
    pub flip_v: bool,
    /// 输出不支持透明（JPG）时用于拍平透明通道的背景色，如 "#ffffff"；空 = white
    pub background: String,
    pub jpg_quality: u8,
    pub png_level: u8,
    pub webp_quality: u8,
    pub avif_quality: u8,
    /// 新任务的默认目标格式
    pub default_target: String,
}

impl Default for ImageSettings {
    fn default() -> Self {
        ImageSettings {
            quality: Quality::Balanced,
            width: 0,
            height: 0,
            keep_aspect: true,
            strip: true,
            rotation: "none".into(),
            flip_h: false,
            flip_v: false,
            background: String::new(),
            jpg_quality: 85,
            png_level: 9,
            webp_quality: 82,
            avif_quality: 55,
            default_target: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EbookSettings {
    pub quality: Quality,
    pub profile: String,
    pub base_font_size: u32,
    pub line_height: f32,
    pub jpeg_quality: u8,
    pub insert_toc: bool,
    pub margin: u32,
    /// 覆盖书名元数据；空 = 保留源数据
    pub title: String,
    /// 覆盖作者元数据；空 = 保留源数据
    pub author: String,
    /// 嵌入正文字体族；空 = 用输出设备默认字体
    pub font_family: String,
    /// 正文两端对齐（关闭后左对齐）
    pub justify: bool,
    /// 新任务的默认目标格式
    pub default_target: String,
}

impl Default for EbookSettings {
    fn default() -> Self {
        EbookSettings {
            quality: Quality::Balanced,
            profile: "generic".into(),
            base_font_size: 12,
            line_height: 1.4,
            jpeg_quality: 80,
            insert_toc: true,
            margin: 15,
            title: String::new(),
            author: String::new(),
            font_family: String::new(),
            justify: true,
            default_target: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DocumentSettings {
    /// auto / pandoc / soffice
    pub via: String,
    /// pandoc 导出 PDF 时使用的排版引擎
    pub pdf_engine: String,
    /// pandoc 输出（docx / pdf / epub）时自动生成目录
    pub toc: bool,
    /// 新任务的默认目标格式
    pub default_target: String,
}

impl Default for DocumentSettings {
    fn default() -> Self {
        DocumentSettings {
            via: "auto".into(),
            pdf_engine: "weasyprint".into(),
            toc: false,
            default_target: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalSettings {
    pub theme: String,
    pub accent: String,
    pub concurrent_jobs: u8,
    /// 输出目录；空字符串表示与源文件同目录
    pub output_folder: String,
    /// 启动时自动检查引擎更新
    pub auto_check_update: bool,
    /// 覆盖同名文件
    pub overwrite: bool,
    /// 批量导入文件夹时，在输出目录下还原原有子目录层级
    #[serde(default = "default_true")]
    pub preserve_tree: bool,
    /// 界面语言：system / zh-Hans / zh-Hant / en / ja / es
    #[serde(default = "default_language")]
    pub language: String,
    /// 应用更新检查频率：startup / daily / weekly / monthly / never
    #[serde(default = "default_update_freq")]
    pub update_check_freq: String,
    /// 上次检查应用更新的时间（毫秒）；0 = 从未检查
    #[serde(default)]
    pub last_update_check: u64,
    /// 用户点了「跳过此版本」的版本号；空 = 没跳过
    #[serde(default)]
    pub skipped_version: String,
    /// 更新源镜像 id；"auto" = 自动择优
    #[serde(default = "default_update_mirror")]
    pub update_mirror: String,
}

/// serde 的 default 需要无参函数，`bool::default()` 是 false，这里要 true。
fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "system".into()
}

fn default_update_freq() -> String {
    "startup".into()
}

fn default_update_mirror() -> String {
    "auto".into()
}

impl Default for GlobalSettings {
    fn default() -> Self {
        GlobalSettings {
            theme: "light".into(),
            accent: "blue".into(),
            concurrent_jobs: 2,
            output_folder: String::new(),
            auto_check_update: true,
            overwrite: false,
            preserve_tree: true,
            language: default_language(),
            update_check_freq: default_update_freq(),
            last_update_check: 0,
            skipped_version: String::new(),
            update_mirror: default_update_mirror(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub image: ImageSettings,
    pub ebook: EbookSettings,
    pub document: DocumentSettings,
    #[serde(rename = "global")]
    pub global: GlobalSettings,
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&text) {
                return s;
            }
        }
        Settings::default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// 该域设置里用户指定的默认目标格式（空 = 跟随格式矩阵推荐）。
    pub fn default_target(&self, domain: Domain) -> &str {
        match domain {
            Domain::Video => &self.video.default_target,
            Domain::Audio => &self.audio.default_target,
            Domain::Image => &self.image.default_target,
            Domain::Ebook => &self.ebook.default_target,
            Domain::Document => &self.document.default_target,
        }
    }

    /// 把某个域的设置落到转换参数。引擎层只读 `ConvOptions`，不关心域。
    ///
    /// 每个域只填自己相关的字段——视频域复用音频模块的码率/采样率参数来
    /// 处理视频文件里的音轨，其余域的字段一律保持默认，避免串味。
    pub fn to_options(&self, domain: Domain) -> ConvOptions {
        let v = &self.video;
        let a = &self.audio;
        let im = &self.image;
        let eb = &self.ebook;
        let doc = &self.document;

        let mut o = ConvOptions::default();

        match domain {
            Domain::Video => {
                o.quality = v.quality;
                o.keep_source = v.keep_source;
                o.scale = v.target_size();
                o.video_codec = opt_str(&v.video_codec);
                o.crf = if v.video_bitrate > 0 {
                    None
                } else {
                    Some(v.crf)
                };
                o.video_bitrate_kb = if v.video_bitrate > 0 {
                    Some(v.video_bitrate)
                } else {
                    None
                };
                o.hardware = v.hardware;
                o.encoder = opt_str(&v.encoder);
                o.speed = v.speed.clone();
                o.fps = parse_fps(&v.fps);
                o.faststart = v.faststart;
                o.deinterlace = v.deinterlace;
                o.rotation = v.rotation.clone();
                o.flip_h = v.flip_h;
                o.flip_v = v.flip_v;

                // 视频里的音轨用视频模块自己的参数（面板的「音轨」分组）
                o.audio_codec = opt_str(&v.audio_codec);
                // 0 = 保持原码率（不显式指定 -b:a）
                o.audio_bitrate_kb = if v.audio_bitrate > 0 {
                    Some(v.audio_bitrate)
                } else {
                    None
                };
                o.sample_rate = opt_u32(v.audio_sample_rate);
                o.channels = opt_u8(v.audio_channels);
                o.normalize = v.audio_normalize;
                o.volume_db = v.volume_db;
            }
            Domain::Audio => {
                o.quality = a.quality;
                o.keep_source = a.keep_source;
                o.audio_codec = opt_str(&a.codec);
                // 0 = 保持原码率（不显式指定 -b:a）
                o.audio_bitrate_kb = if a.bitrate > 0 { Some(a.bitrate) } else { None };
                o.sample_rate = opt_u32(a.sample_rate);
                o.channels = opt_u8(a.channels);
                o.normalize = a.normalize;
                o.volume_db = a.volume_db;
            }
            Domain::Image => {
                o.quality = im.quality;
                o.width = opt_u32(im.width);
                o.height = opt_u32(im.height);
                o.keep_aspect = im.keep_aspect;
                o.strip = im.strip;
                o.rotation = im.rotation.clone();
                o.flip_h = im.flip_h;
                o.flip_v = im.flip_v;
                o.img_background = im.background.clone();
                o.jpg_quality = Some(im.jpg_quality);
                o.png_level = Some(im.png_level);
                o.webp_quality = Some(im.webp_quality);
                o.avif_quality = Some(im.avif_quality);
            }
            Domain::Ebook => {
                o.quality = eb.quality;
                o.ebook_profile = eb.profile.clone();
                o.base_font_size = eb.base_font_size;
                o.line_height = eb.line_height;
                o.ebook_jpeg_quality = eb.jpeg_quality;
                o.insert_toc = eb.insert_toc;
                o.margin = eb.margin;
                o.ebook_title = eb.title.clone();
                o.ebook_author = eb.author.clone();
                o.ebook_font_family = eb.font_family.clone();
                o.ebook_justify = eb.justify;
            }
            Domain::Document => {
                o.doc_via = doc.via.clone();
                o.pdf_engine = doc.pdf_engine.clone();
                o.doc_toc = doc.toc;
            }
        }

        o
    }
}

/// 真正交给引擎调度层的扁平参数。它由 `Settings::to_options` 生成，
/// 与具体模块解耦——引擎只认这个结构。
#[derive(Debug, Clone)]
pub struct ConvOptions {
    pub quality: Quality,
    /// 保持源参数：为 true 时引擎调度层会用探测到的源流参数覆盖
    /// scale / fps / 码率 / 采样率 / 声道，只保留编码器与显式画面变换。
    pub keep_source: bool,
    pub scale: Option<(u32, u32)>,
    pub video_codec: Option<String>,
    pub crf: Option<u8>,
    pub video_bitrate_kb: Option<u32>,
    pub audio_codec: Option<String>,
    pub audio_bitrate_kb: Option<u32>,
    pub fps: Option<f32>,
    pub faststart: bool,
    pub deinterlace: bool,
    pub hardware: HwMode,
    pub encoder: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub normalize: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub keep_aspect: bool,
    pub strip: bool,
    pub jpg_quality: Option<u8>,
    pub png_level: Option<u8>,
    pub webp_quality: Option<u8>,
    pub avif_quality: Option<u8>,
    pub ebook_profile: String,
    pub base_font_size: u32,
    pub line_height: f32,
    pub ebook_jpeg_quality: u8,
    pub insert_toc: bool,
    pub margin: u32,
    pub ebook_title: String,
    pub ebook_author: String,
    pub ebook_font_family: String,
    pub ebook_justify: bool,
    pub doc_via: String,
    pub pdf_engine: String,
    pub doc_toc: bool,
    pub speed: String,
    pub rotation: String,
    pub flip_h: bool,
    pub flip_v: bool,
    pub volume_db: i8,
    pub img_background: String,
}

impl Default for ConvOptions {
    fn default() -> Self {
        ConvOptions {
            quality: Quality::Balanced,
            keep_source: false,
            scale: None,
            video_codec: None,
            crf: None,
            video_bitrate_kb: None,
            audio_codec: None,
            audio_bitrate_kb: None,
            fps: None,
            faststart: true,
            deinterlace: false,
            hardware: HwMode::Auto,
            encoder: None,
            sample_rate: None,
            channels: None,
            normalize: false,
            width: None,
            height: None,
            keep_aspect: true,
            strip: true,
            jpg_quality: None,
            png_level: None,
            webp_quality: None,
            avif_quality: None,
            ebook_profile: "generic".into(),
            base_font_size: 12,
            line_height: 1.4,
            ebook_jpeg_quality: 80,
            insert_toc: true,
            margin: 15,
            ebook_title: String::new(),
            ebook_author: String::new(),
            ebook_font_family: String::new(),
            ebook_justify: true,
            doc_via: "auto".into(),
            pdf_engine: "weasyprint".into(),
            doc_toc: false,
            speed: "medium".into(),
            rotation: "none".into(),
            flip_h: false,
            flip_v: false,
            volume_db: 0,
            img_background: String::new(),
        }
    }
}

/// 「保持源参数」：用探测到的源流参数覆盖面板里的分辨率、帧率、码率、
/// 采样率与声道，只保留编码器、输出格式与显式画面变换。
///
/// 这是「只改编码（如 H.264→H.265、MP3→AAC），其余与原文件一致」这一
/// 场景的落地点——输出参数来自**源文件**，而不是面板里的默认值。
pub fn apply_keep_source(o: &mut ConvOptions, s: &crate::probe::SourceStreams) {
    o.scale = None;
    o.fps = None;
    o.crf = None;
    o.video_bitrate_kb = s.video_bitrate_kbps;
    o.audio_bitrate_kb = s.audio_bitrate_kbps;
    o.sample_rate = s.sample_rate;
    o.channels = s.channels;
}

// ---------------------------------------------------------------- 预设

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub domain: Domain,
    /// 该域设置的完整快照（JSON 对象），应用即整体替换
    pub settings: serde_json::Value,
    /// 内置预设不可删除；用户自定义为 false
    #[serde(default)]
    pub builtin: bool,
}

/// 内置预设：覆盖各模块的常见使用场景。
pub fn builtin_presets() -> Vec<Preset> {
    let mut out = Vec::new();

    // 视频
    out.push(preset(
        "网页高清 1080p",
        Domain::Video,
        json!({
            "quality":"balanced","resolution":"r1080p","videoCodec":"auto","videoBitrate":0,
            "crf":23,"audioCodec":"auto","audioBitrate":128,"fps":"source",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "手机兼容 H.264",
        Domain::Video,
        json!({
            "quality":"balanced","resolution":"r720p","videoCodec":"h264","videoBitrate":0,
            "crf":24,"audioCodec":"aac","audioBitrate":128,"fps":"source",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":true
        }),
    ));
    out.push(preset(
        "极致压缩",
        Domain::Video,
        json!({
            "quality":"small","resolution":"r480p","videoCodec":"h264","videoBitrate":0,
            "crf":30,"audioCodec":"aac","audioBitrate":96,"fps":"24",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "无损存档",
        Domain::Video,
        json!({
            "quality":"high","resolution":"original","videoCodec":"auto","videoBitrate":0,
            "crf":18,"audioCodec":"copy","audioBitrate":0,"fps":"source",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "B 站投稿 1080p",
        Domain::Video,
        json!({
            "quality":"high","resolution":"r1080p","videoCodec":"h264","videoBitrate":0,
            "crf":20,"audioCodec":"aac","audioBitrate":192,"fps":"source",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "微信小视频",
        Domain::Video,
        json!({
            "quality":"small","resolution":"r720p","videoCodec":"h264","videoBitrate":0,
            "crf":30,"audioCodec":"aac","audioBitrate":96,"fps":"source",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "高帧率 60fps",
        Domain::Video,
        json!({
            "quality":"balanced","resolution":"r1080p","videoCodec":"auto","videoBitrate":0,
            "crf":21,"audioCodec":"aac","audioBitrate":160,"fps":"60",
            "hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "竖屏短视频",
        Domain::Video,
        json!({
            "quality":"balanced","resolution":"custom","customWidth":1080,"customHeight":1920,
            "videoCodec":"h264","videoBitrate":0,"crf":23,"audioCodec":"aac","audioBitrate":128,
            "fps":"source","hardware":"auto","encoder":"auto","faststart":true,"deinterlace":false
        }),
    ));
    out.push(preset(
        "动图 GIF",
        Domain::Video,
        json!({
            "quality":"small","resolution":"r480p","videoCodec":"auto","videoBitrate":0,
            "crf":28,"audioCodec":"none","audioBitrate":0,"fps":"15",
            "hardware":"off","encoder":"auto","faststart":false,"deinterlace":false,
            "defaultTarget":"gif"
        }),
    ));

    // 音频
    out.push(preset(
        "高保真 320k",
        Domain::Audio,
        json!({"quality":"high","codec":"auto","bitrate":320,"sampleRate":0,"channels":0,"normalize":false}),
    ));
    out.push(preset(
        "通用兼容",
        Domain::Audio,
        json!({"quality":"balanced","codec":"mp3","bitrate":192,"sampleRate":44100,"channels":2,"normalize":true}),
    ));
    out.push(preset(
        "无损 FLAC",
        Domain::Audio,
        json!({"quality":"high","codec":"flac","bitrate":0,"sampleRate":0,"channels":0,"normalize":false}),
    ));
    out.push(preset(
        "播客人声",
        Domain::Audio,
        json!({"quality":"balanced","codec":"aac","bitrate":96,"sampleRate":44100,"channels":1,"normalize":true}),
    ));
    out.push(preset(
        "CD 无损 WAV",
        Domain::Audio,
        json!({"quality":"high","codec":"wav","bitrate":0,"sampleRate":44100,"channels":2,"normalize":false}),
    ));
    out.push(preset(
        "微信/QQ 通用 MP3",
        Domain::Audio,
        json!({"quality":"balanced","codec":"mp3","bitrate":128,"sampleRate":44100,"channels":2,"normalize":false}),
    ));

    // 图片
    out.push(preset(
        "社交分享 JPG",
        Domain::Image,
        json!({"quality":"balanced","width":1080,"height":0,"keepAspect":true,"strip":true,"jpgQuality":85,"pngLevel":9,"webpQuality":82,"avifQuality":55}),
    ));
    out.push(preset(
        "透明无损 PNG",
        Domain::Image,
        json!({"quality":"high","width":0,"height":0,"keepAspect":true,"strip":true,"jpgQuality":95,"pngLevel":9,"webpQuality":90,"avifQuality":70}),
    ));
    out.push(preset(
        "WebP 高压缩",
        Domain::Image,
        json!({"quality":"small","width":0,"height":0,"keepAspect":true,"strip":true,"jpgQuality":75,"pngLevel":9,"webpQuality":70,"avifQuality":40}),
    ));
    out.push(preset(
        "微信聊天图",
        Domain::Image,
        json!({"quality":"small","width":1280,"height":0,"keepAspect":true,"strip":true,"jpgQuality":75,"pngLevel":9,"webpQuality":75,"avifQuality":45}),
    ));
    out.push(preset(
        "ICO 图标",
        Domain::Image,
        json!({"quality":"high","width":256,"height":256,"keepAspect":true,"strip":true,"jpgQuality":95,"pngLevel":9,"webpQuality":90,"avifQuality":70,"defaultTarget":"ico"}),
    ));
    out.push(preset(
        "印刷 TIFF",
        Domain::Image,
        json!({"quality":"high","width":0,"height":0,"keepAspect":true,"strip":false,"jpgQuality":100,"pngLevel":9,"webpQuality":100,"avifQuality":90,"defaultTarget":"tiff"}),
    ));
    out.push(preset(
        "表情包 GIF",
        Domain::Image,
        json!({"quality":"small","width":480,"height":0,"keepAspect":true,"strip":true,"jpgQuality":70,"pngLevel":9,"webpQuality":65,"avifQuality":40,"defaultTarget":"gif"}),
    ));

    // 电子书
    out.push(preset(
        "Kindle 适配",
        Domain::Ebook,
        json!({"quality":"balanced","profile":"kindle","baseFontSize":14,"lineHeight":1.5,"jpegQuality":60,"insertToc":true,"margin":20}),
    ));
    out.push(preset(
        "阅读器通用",
        Domain::Ebook,
        json!({"quality":"balanced","profile":"generic","baseFontSize":12,"lineHeight":1.4,"jpegQuality":80,"insertToc":true,"margin":15}),
    ));
    out.push(preset(
        "小体积",
        Domain::Ebook,
        json!({"quality":"small","profile":"generic","baseFontSize":12,"lineHeight":1.4,"jpegQuality":50,"insertToc":false,"margin":10}),
    ));
    out.push(preset(
        "大字护眼",
        Domain::Ebook,
        json!({"quality":"balanced","profile":"generic","baseFontSize":18,"lineHeight":1.8,"jpegQuality":80,"insertToc":true,"margin":20}),
    ));
    out.push(preset(
        "手机阅读",
        Domain::Ebook,
        json!({"quality":"balanced","profile":"phone","baseFontSize":13,"lineHeight":1.6,"jpegQuality":70,"insertToc":true,"margin":12}),
    ));

    // 文档
    out.push(preset(
        "排版精美 PDF",
        Domain::Document,
        json!({"via":"auto","pdfEngine":"weasyprint","defaultTarget":"pdf"}),
    ));
    out.push(preset(
        "标准 Word",
        Domain::Document,
        json!({"via":"auto","pdfEngine":"weasyprint","defaultTarget":"docx"}),
    ));
    out.push(preset(
        "Markdown 转 Word",
        Domain::Document,
        json!({"via":"pandoc","pdfEngine":"weasyprint","defaultTarget":"docx"}),
    ));
    out.push(preset(
        "网页转 PDF",
        Domain::Document,
        json!({"via":"auto","pdfEngine":"weasyprint","defaultTarget":"pdf"}),
    ));
    out.push(preset(
        "表格转 CSV",
        Domain::Document,
        json!({"via":"auto","pdfEngine":"weasyprint","defaultTarget":"csv"}),
    ));

    out
}

fn preset(name: &str, domain: Domain, settings: serde_json::Value) -> Preset {
    Preset {
        name: name.into(),
        domain,
        settings,
        builtin: true,
    }
}

fn opt_u32(v: u32) -> Option<u32> {
    if v == 0 {
        None
    } else {
        Some(v)
    }
}

fn opt_u8(v: u8) -> Option<u8> {
    if v == 0 {
        None
    } else {
        Some(v)
    }
}

/// "auto" / 空串表示交给引擎按容器自动决定。
fn opt_str(v: &str) -> Option<String> {
    if v.is_empty() || v == "auto" {
        None
    } else {
        Some(v.to_string())
    }
}

fn parse_fps(s: &str) -> Option<f32> {
    if s.trim().eq_ignore_ascii_case("source") || s.trim().is_empty() {
        None
    } else {
        s.trim().parse::<f32>().ok()
    }
}

/// 用户自定义预设的持久化位置（与设置放同一目录）。
pub fn presets_path(settings_path: &Path) -> PathBuf {
    let mut p = settings_path.to_path_buf();
    p.set_file_name("presets.json");
    p
}
