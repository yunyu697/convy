import { Field, Group, NumberInput, Segmented, Select, Slider, TextInput, Toggle } from "../components/ui";
import {
  DOMAIN_TARGETS,
  QUALITY_LABEL,
  type AudioSettings,
  type Domain,
  type DocumentSettings,
  type EbookSettings,
  type HwMode,
  type ImageSettings,
  type Quality,
  type ResolutionMode,
  type VideoSettings,
} from "../types";

// ---------------------------------------------------------------- 公共选项

const ROTATIONS: { value: string; label: string }[] = [
  { value: "none", label: "不旋转" },
  { value: "90", label: "顺时针 90°" },
  { value: "180", label: "旋转 180°" },
  { value: "270", label: "顺时针 270°（即逆时针 90°）" },
];

const SAMPLE_RATES: { value: string; label: string }[] = [
  { value: "0", label: "保持源采样率" },
  { value: "8000", label: "8 kHz · 电话" },
  { value: "22050", label: "22.05 kHz" },
  { value: "32000", label: "32 kHz" },
  { value: "44100", label: "44.1 kHz · CD" },
  { value: "48000", label: "48 kHz · 视频标准" },
  { value: "96000", label: "96 kHz · Hi-Res" },
];

const SPEEDS: { value: string; label: string }[] = [
  { value: "ultrafast", label: "极快（文件更大）" },
  { value: "superfast", label: "超快" },
  { value: "veryfast", label: "很快" },
  { value: "faster", label: "较快" },
  { value: "fast", label: "快" },
  { value: "medium", label: "中等（推荐）" },
  { value: "slow", label: "慢" },
  { value: "slower", label: "较慢" },
  { value: "veryslow", label: "极慢（文件更小）" },
];

const KNOWN_FPS: { value: string; label: string }[] = [
  { value: "source", label: "保持源帧率" },
  { value: "23.976", label: "23.976 · 电影" },
  { value: "24", label: "24" },
  { value: "25", label: "25 · PAL" },
  { value: "29.97", label: "29.97 · NTSC" },
  { value: "30", label: "30" },
  { value: "50", label: "50" },
  { value: "60", label: "60" },
];

/** 音频码率选项：0 = 保持原码率（不显式指定，由编码器就近输出） */
const AUDIO_BITRATES: { value: string; label: string }[] = [
  { value: "0", label: "保持原码率" },
  { value: "64", label: "64 kbps" },
  { value: "96", label: "96 kbps" },
  { value: "128", label: "128 kbps" },
  { value: "160", label: "160 kbps" },
  { value: "192", label: "192 kbps" },
  { value: "256", label: "256 kbps" },
  { value: "320", label: "320 kbps" },
  { value: "448", label: "448 kbps" },
  { value: "512", label: "512 kbps" },
];

// ---------------------------------------------------------------- 快速调整

/**
 * 「快速调整」条：把每个模块最高频的单项设置做成一键分段按钮，
 * 覆盖“只想改编码/格式/质量，其它都不动”的场景。
 * 改的就是下方完整面板绑定的同一份设置，两侧实时同步。
 */
function QuickBar({ children }: { children: React.ReactNode }) {
  return (
    <div className="quickbar">
      <span className="quickbar__title" aria-hidden>
        ⚡ 快速调整
      </span>
      {children}
    </div>
  );
}

function QuickItem({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <span className="quickbar__item">
      <span className="quickbar__label">{label}</span>
      {children}
    </span>
  );
}

/** 「目标格式」字段：新添加任务默认输出到这个格式 */
function TargetField({
  domain,
  value,
  onChange,
}: {
  domain: Domain;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <Field label="目标格式" hint="新添加任务的默认输出格式">
      <Select
        value={value}
        onChange={onChange}
        options={[
          { value: "", label: "自动（跟随格式推荐）" },
          ...DOMAIN_TARGETS[domain].map((t) => ({ value: t, label: t.toUpperCase() })),
        ]}
      />
    </Field>
  );
}

/** 音量增益滑块，0 dB 显示为「不调整」 */
function VolumeField({
  value,
  onChange,
}: {
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <Field label="音量增益" hint="与响度归一互斥，归一优先">
      <Slider
        value={value}
        onChange={onChange}
        min={-30}
        max={12}
        format={(v) => (v === 0 ? "不调整" : `${v > 0 ? "+" : ""}${v} dB`)}
      />
    </Field>
  );
}

// ---------------------------------------------------------------- 视频

const RESOLUTIONS: { value: ResolutionMode; label: string }[] = [
  { value: "original", label: "保持原始分辨率" },
  { value: "r2160p", label: "4K UHD · 3840×2160" },
  { value: "r1080p", label: "Full HD · 1920×1080" },
  { value: "r720p", label: "HD · 1280×720" },
  { value: "r480p", label: "标清 · 854×480" },
  { value: "custom", label: "自定义…" },
];

const VIDEO_CODECS: { value: string; label: string }[] = [
  { value: "auto", label: "自动匹配容器" },
  { value: "h264", label: "H.264 / AVC" },
  { value: "hevc", label: "H.265 / HEVC" },
  { value: "vp9", label: "VP9" },
  { value: "av1", label: "AV1" },
  { value: "mpeg4", label: "MPEG-4 Part 2" },
];

const AUDIO_CODECS: { value: string; label: string }[] = [
  { value: "auto", label: "自动匹配容器" },
  { value: "aac", label: "AAC" },
  { value: "mp3", label: "MP3" },
  { value: "opus", label: "Opus" },
  { value: "ogg", label: "Vorbis / OGG" },
  { value: "flac", label: "FLAC（无损）" },
  { value: "wav", label: "WAV（PCM）" },
  { value: "wma", label: "WMA" },
  { value: "copy", label: "直接复制源音轨" },
  { value: "none", label: "去掉音轨（无声视频）" },
];

const HW_ENCODERS = [
  { value: "h264_nvenc", label: "NVIDIA NVENC" },
  { value: "h264_qsv", label: "Intel QuickSync" },
  { value: "h264_amf", label: "AMD AMF" },
  { value: "hevc_nvenc", label: "NVIDIA NVENC (HEVC)" },
  { value: "hevc_qsv", label: "Intel QuickSync (HEVC)" },
  { value: "hevc_amf", label: "AMD AMF (HEVC)" },
  { value: "h264_videotoolbox", label: "Apple VideoToolbox" },
  { value: "h264_vaapi", label: "VAAPI（Linux）" },
];

export function VideoPanel({
  value,
  onChange,
}: {
  value: VideoSettings;
  onChange: (v: VideoSettings) => void;
}) {
  const set = <K extends keyof VideoSettings>(k: K, v: VideoSettings[K]) =>
    onChange({ ...value, [k]: v });
  const cbr = value.videoBitrate > 0;
  const knownFps = KNOWN_FPS.some((o) => o.value === value.fps);

  return (
    <>
      <QuickBar>
        <QuickItem label="其余参数">
          <Segmented
            compact
            value={value.keepSource ? "source" : "custom"}
            onChange={(v) => set("keepSource", v === "source")}
            options={[
              { value: "source", label: "跟随源文件" },
              { value: "custom", label: "自定义" },
            ]}
          />
        </QuickItem>
        <QuickItem label="视频编码">
          <Segmented
            compact
            value={value.videoCodec}
            onChange={(v) => set("videoCodec", v)}
            options={[
              { value: "auto", label: "自动" },
              { value: "h264", label: "H.264" },
              { value: "hevc", label: "H.265" },
              { value: "vp9", label: "VP9" },
              { value: "av1", label: "AV1" },
            ]}
          />
        </QuickItem>
        <QuickItem label="音轨">
          <Segmented
            compact
            value={value.audioCodec}
            onChange={(v) => set("audioCodec", v)}
            options={[
              { value: "auto", label: "自动" },
              { value: "aac", label: "AAC" },
              { value: "mp3", label: "MP3" },
              { value: "copy", label: "复制" },
              { value: "none", label: "无声" },
            ]}
          />
        </QuickItem>
      </QuickBar>

      {value.keepSource && (
        <p className="quickbar__hint">
          分辨率、帧率、码率、采样率、声道均与源文件一致，只应用你选择的编码与格式。
        </p>
      )}

      <Group title="基础" desc="画质、分辨率与目标格式">
        <Field label="质量档位" locked={value.keepSource}>
          <Segmented<Quality>
            value={value.quality}
            onChange={(q) => set("quality", q)}
            options={[
              { value: "high", label: QUALITY_LABEL.high },
              { value: "balanced", label: QUALITY_LABEL.balanced },
              { value: "small", label: QUALITY_LABEL.small },
            ]}
          />
        </Field>

        <TargetField domain="video" value={value.defaultTarget} onChange={(v) => set("defaultTarget", v)} />

        <Field label="分辨率" hint="长边优先，保持画面比例" locked={value.keepSource}>
          <Select
            value={value.resolution}
            onChange={(v) => set("resolution", v)}
            options={RESOLUTIONS}
          />
        </Field>

        {value.resolution === "custom" && (
          <Field label="自定义尺寸" wide locked={value.keepSource}>
            <div className="field__row">
              <NumberInput
                value={value.customWidth}
                onChange={(v) => set("customWidth", v)}
                min={16}
                max={7680}
                step={2}
                suffix="px"
              />
              <span className="field__x">×</span>
              <NumberInput
                value={value.customHeight}
                onChange={(v) => set("customHeight", v)}
                min={16}
                max={4320}
                step={2}
                suffix="px"
              />
            </div>
          </Field>
        )}

        <Field label="视频编码器" hint="决定 H.264 / H.265 / VP9 / AV1">
          <Select
            value={value.videoCodec}
            onChange={(v) => set("videoCodec", v)}
            options={VIDEO_CODECS}
          />
        </Field>
      </Group>

      <Group title="码率与速度" desc="CRF / CBR、编码速度与帧率">
        <Field label="码率模式" locked={value.keepSource}>
          <Segmented
            value={cbr ? "cbr" : "crf"}
            onChange={(m) => set("videoBitrate", m === "cbr" ? 6000 : 0)}
            options={[
              { value: "crf", label: "CRF 恒定画质" },
              { value: "cbr", label: "CBR 固定码率" },
            ]}
          />
        </Field>

        {cbr ? (
          <Field label="视频码率" hint="单位 kbps" locked={value.keepSource}>
            <NumberInput
              value={value.videoBitrate}
              onChange={(v) => set("videoBitrate", v)}
              min={64}
              max={102400}
              step={64}
              suffix="kbps"
            />
          </Field>
        ) : (
          <Field label="CRF" hint="数值越小画质越好、文件越大" locked={value.keepSource}>
            <Slider
              value={value.crf}
              onChange={(v) => set("crf", v)}
              min={0}
              max={51}
              format={(v) => `${v}`}
            />
          </Field>
        )}

        <Field label="编码速度" hint="只对软件编码器（x264/x265）生效，越慢压缩率越高">
          <Select value={value.speed} onChange={(v) => set("speed", v)} options={SPEEDS} />
        </Field>

        <Field label="帧率" locked={value.keepSource}>
          {knownFps ? (
            <Select
              value={value.fps}
              onChange={(v) => set("fps", v === "__custom" ? "" : v)}
              options={[...KNOWN_FPS, { value: "__custom", label: "自定义…" }]}
            />
          ) : (
            <div className="field__row">
              <TextInput
                value={value.fps}
                onChange={(v) => set("fps", v)}
                placeholder="如 23.976"
              />
              <button className="btn btn--ghost" onClick={() => set("fps", "source")}>
                恢复
              </button>
            </div>
          )}
        </Field>
      </Group>

      <Group title="画面处理" desc="旋转、翻转与去隔行">
        <Field label="旋转" hint="顺时针角度">
          <Select value={value.rotation} onChange={(v) => set("rotation", v)} options={ROTATIONS} />
        </Field>

        <Field label="镜像翻转">
          <div className="field__row">
            <Toggle checked={value.flipH} onChange={(v) => set("flipH", v)} label="水平" />
            <Toggle checked={value.flipV} onChange={(v) => set("flipV", v)} label="垂直" />
          </div>
        </Field>

        <Field label="去隔行" hint="把隔行扫描转为逐行（老式片源）">
          <Toggle
            checked={value.deinterlace}
            onChange={(v) => set("deinterlace", v)}
            label={value.deinterlace ? "已启用" : "已关闭"}
          />
        </Field>
      </Group>

      <Group title="硬件加速" desc="优先使用显卡编码，失败自动回退软件">
        <Field label="加速模式">
          <Segmented<HwMode>
            value={value.hardware}
            onChange={(v) => set("hardware", v)}
            options={[
              { value: "auto", label: "自动" },
              { value: "force", label: "强制指定" },
              { value: "off", label: "关闭" },
            ]}
          />
        </Field>

        {value.hardware === "force" && (
          <Field label="指定编码器">
            <Select
              value={value.encoder}
              onChange={(v) => set("encoder", v)}
              options={[{ value: "auto", label: "自动选择" }, ...HW_ENCODERS]}
            />
          </Field>
        )}
      </Group>

      <Group title="音轨" desc="视频文件内音频流的处理">
        <Field label="音频编码器">
          <Select
            value={value.audioCodec}
            onChange={(v) => set("audioCodec", v)}
            options={AUDIO_CODECS}
          />
        </Field>
        <Field label="音频码率" hint="保持原码率 = 由编码器按源流就近输出" locked={value.keepSource}>
          <Select
            value={String(value.audioBitrate)}
            onChange={(v) => set("audioBitrate", Number(v))}
            options={AUDIO_BITRATES}
          />
        </Field>
        <Field label="采样率" locked={value.keepSource}>
          <Select
            value={String(value.audioSampleRate)}
            onChange={(v) => set("audioSampleRate", Number(v))}
            options={SAMPLE_RATES}
          />
        </Field>
        <Field label="声道数" locked={value.keepSource}>
          <Segmented
            value={String(value.audioChannels)}
            onChange={(v) => set("audioChannels", Number(v))}
            options={[
              { value: "0", label: "保持" },
              { value: "1", label: "单声道" },
              { value: "2", label: "立体声" },
            ]}
          />
        </Field>
        <Field label="响度归一" hint="统一到广播标准响度，与增益互斥">
          <Toggle
            checked={value.audioNormalize}
            onChange={(v) => set("audioNormalize", v)}
            label={value.audioNormalize ? "已启用" : "已关闭"}
          />
        </Field>
        <VolumeField value={value.volumeDb} onChange={(v) => set("volumeDb", v)} />
      </Group>

      <Group title="封装" defaultOpen={false}>
        <Field label="快速起播" hint="把 moov 原子前置，网页播放更快">
          <Toggle
            checked={value.faststart}
            onChange={(v) => set("faststart", v)}
            label={value.faststart ? "已启用" : "已关闭"}
          />
        </Field>
      </Group>
    </>
  );
}

// ---------------------------------------------------------------- 音频

const AUDIO_CODEC_OPTS: { value: string; label: string }[] = [
  { value: "auto", label: "自动匹配容器" },
  { value: "mp3", label: "MP3" },
  { value: "aac", label: "AAC" },
  { value: "flac", label: "FLAC（无损）" },
  { value: "wav", label: "WAV（PCM）" },
  { value: "opus", label: "Opus" },
  { value: "ogg", label: "Vorbis / OGG" },
  { value: "wma", label: "WMA" },
  { value: "copy", label: "直接复制源音轨" },
];

export function AudioPanel({
  value,
  onChange,
}: {
  value: AudioSettings;
  onChange: (v: AudioSettings) => void;
}) {
  const set = <K extends keyof AudioSettings>(k: K, v: AudioSettings[K]) =>
    onChange({ ...value, [k]: v });

  return (
    <>
      <QuickBar>
        <QuickItem label="其余参数">
          <Segmented
            compact
            value={value.keepSource ? "source" : "custom"}
            onChange={(v) => set("keepSource", v === "source")}
            options={[
              { value: "source", label: "跟随源文件" },
              { value: "custom", label: "自定义" },
            ]}
          />
        </QuickItem>
        <QuickItem label="编码">
          <Segmented
            compact
            value={value.codec}
            onChange={(v) => set("codec", v)}
            options={[
              { value: "auto", label: "自动" },
              { value: "mp3", label: "MP3" },
              { value: "aac", label: "AAC" },
              { value: "flac", label: "FLAC" },
              { value: "wav", label: "WAV" },
              { value: "opus", label: "Opus" },
            ]}
          />
        </QuickItem>
        <QuickItem label="码率">
          <Segmented
            compact
            value={String(value.bitrate)}
            onChange={(v) => set("bitrate", Number(v))}
            options={[
              { value: "0", label: "原码率" },
              { value: "128", label: "128k" },
              { value: "192", label: "192k" },
              { value: "320", label: "320k" },
            ]}
          />
        </QuickItem>
      </QuickBar>

      {value.keepSource && (
        <p className="quickbar__hint">
          码率、采样率、声道均与源文件一致，只应用你选择的编码与格式。
        </p>
      )}

      <Group title="编码" desc="输出格式与码率">
        <Field label="质量档位" locked={value.keepSource}>
          <Segmented<Quality>
            value={value.quality}
            onChange={(q) => set("quality", q)}
            options={[
              { value: "high", label: QUALITY_LABEL.high },
              { value: "balanced", label: QUALITY_LABEL.balanced },
              { value: "small", label: QUALITY_LABEL.small },
            ]}
          />
        </Field>

        <TargetField domain="audio" value={value.defaultTarget} onChange={(v) => set("defaultTarget", v)} />

        <Field label="音频编码器">
          <Select
            value={value.codec}
            onChange={(v) => set("codec", v)}
            options={AUDIO_CODEC_OPTS}
          />
        </Field>

        <Field label="码率" hint="保持原码率 = 由编码器按源流就近输出；无损/复制时忽略" locked={value.keepSource}>
          <Select
            value={String(value.bitrate)}
            onChange={(v) => set("bitrate", Number(v))}
            options={AUDIO_BITRATES}
          />
        </Field>
      </Group>

      <Group title="采样与声道">
        <Field label="采样率" locked={value.keepSource}>
          <Select
            value={String(value.sampleRate)}
            onChange={(v) => set("sampleRate", Number(v))}
            options={SAMPLE_RATES}
          />
        </Field>

        <Field label="声道数" locked={value.keepSource}>
          <Segmented
            value={String(value.channels)}
            onChange={(v) => set("channels", Number(v))}
            options={[
              { value: "0", label: "保持" },
              { value: "1", label: "单声道" },
              { value: "2", label: "立体声" },
            ]}
          />
        </Field>
      </Group>

      <Group title="音量" desc="归一化或手动增益">
        <Field label="响度归一" hint="统一各文件音量到广播标准响度">
          <Toggle
            checked={value.normalize}
            onChange={(v) => set("normalize", v)}
            label={value.normalize ? "已启用" : "已关闭"}
          />
        </Field>
        <VolumeField value={value.volumeDb} onChange={(v) => set("volumeDb", v)} />
      </Group>
    </>
  );
}

// ---------------------------------------------------------------- 图片

export function ImagePanel({
  value,
  onChange,
}: {
  value: ImageSettings;
  onChange: (v: ImageSettings) => void;
}) {
  const set = <K extends keyof ImageSettings>(k: K, v: ImageSettings[K]) =>
    onChange({ ...value, [k]: v });

  return (
    <>
      <QuickBar>
        <QuickItem label="输出格式">
          <Segmented
            compact
            value={value.defaultTarget}
            onChange={(v) => set("defaultTarget", v)}
            options={[
              { value: "", label: "自动" },
              { value: "jpg", label: "JPG" },
              { value: "png", label: "PNG" },
              { value: "webp", label: "WebP" },
              { value: "avif", label: "AVIF" },
              { value: "gif", label: "GIF" },
            ]}
          />
        </QuickItem>
        <QuickItem label="质量">
          <Segmented<Quality>
            compact
            value={value.quality}
            onChange={(q) => set("quality", q)}
            options={[
              { value: "high", label: "高" },
              { value: "balanced", label: "均衡" },
              { value: "small", label: "小" },
            ]}
          />
        </QuickItem>
      </QuickBar>

      <Group title="基础" desc="输出格式与质量档位">
        <Field label="质量档位">
          <Segmented<Quality>
            value={value.quality}
            onChange={(q) => set("quality", q)}
            options={[
              { value: "high", label: QUALITY_LABEL.high },
              { value: "balanced", label: QUALITY_LABEL.balanced },
              { value: "small", label: QUALITY_LABEL.small },
            ]}
          />
        </Field>

        <TargetField domain="image" value={value.defaultTarget} onChange={(v) => set("defaultTarget", v)} />
      </Group>

      <Group title="尺寸" desc="留 0 表示保持原始尺寸">
        <Field label="宽度 / 高度" wide>
          <div className="field__row">
            <NumberInput
              value={value.width}
              onChange={(v) => set("width", v)}
              min={0}
              max={20000}
              step={10}
              suffix="px"
            />
            <span className="field__x">×</span>
            <NumberInput
              value={value.height}
              onChange={(v) => set("height", v)}
              min={0}
              max={20000}
              step={10}
              suffix="px"
            />
          </div>
        </Field>

        <Field label="保持宽高比" hint="缩放时避免画面变形">
          <Toggle
            checked={value.keepAspect}
            onChange={(v) => set("keepAspect", v)}
            label={value.keepAspect ? "已启用" : "已关闭"}
          />
        </Field>
      </Group>

      <Group title="画面调整" desc="旋转与镜像">
        <Field label="旋转" hint="顺时针角度">
          <Select value={value.rotation} onChange={(v) => set("rotation", v)} options={ROTATIONS} />
        </Field>

        <Field label="镜像翻转">
          <div className="field__row">
            <Toggle checked={value.flipH} onChange={(v) => set("flipH", v)} label="水平" />
            <Toggle checked={value.flipV} onChange={(v) => set("flipV", v)} label="垂直" />
          </div>
        </Field>
      </Group>

      <Group title="压缩质量" desc="按目标格式分别设置">
        <Field label="JPEG 质量" hint="1–100，越高越清晰">
          <Slider
            value={value.jpgQuality}
            onChange={(v) => set("jpgQuality", v)}
            min={1}
            max={100}
            format={(v) => `${v}`}
          />
        </Field>
        <Field label="PNG 压缩级别" hint="0 最快 · 9 体积最小">
          <Slider
            value={value.pngLevel}
            onChange={(v) => set("pngLevel", v)}
            min={0}
            max={9}
            format={(v) => `${v}`}
          />
        </Field>
        <Field label="WebP 质量">
          <Slider
            value={value.webpQuality}
            onChange={(v) => set("webpQuality", v)}
            min={1}
            max={100}
            format={(v) => `${v}`}
          />
        </Field>
        <Field label="AVIF 质量" hint="压缩率极高，编码较慢">
          <Slider
            value={value.avifQuality}
            onChange={(v) => set("avifQuality", v)}
            min={1}
            max={100}
            format={(v) => `${v}`}
          />
        </Field>
      </Group>

      <Group title="元数据与透明" desc="EXIF 与透明通道处理">
        <Field label="清除 EXIF" hint="移除拍摄信息、GPS 等隐私数据">
          <Toggle
            checked={value.strip}
            onChange={(v) => set("strip", v)}
            label={value.strip ? "已启用" : "已关闭"}
          />
        </Field>
        <Field label="背景色" hint="输出 JPG 时拍平透明通道用，如 #ffffff">
          <TextInput
            value={value.background}
            onChange={(v) => set("background", v)}
            placeholder="white"
          />
        </Field>
      </Group>
    </>
  );
}

// ---------------------------------------------------------------- 电子书

const EBOOK_PROFILES = [
  { value: "generic", label: "通用阅读器" },
  { value: "kindle", label: "Kindle" },
  { value: "kindle_dx", label: "Kindle DX" },
  { value: "tablet", label: "平板" },
  { value: "phone", label: "手机" },
];

export function EbookPanel({
  value,
  onChange,
}: {
  value: EbookSettings;
  onChange: (v: EbookSettings) => void;
}) {
  const set = <K extends keyof EbookSettings>(k: K, v: EbookSettings[K]) =>
    onChange({ ...value, [k]: v });

  return (
    <>
      <QuickBar>
        <QuickItem label="设备">
          <Segmented
            compact
            value={value.profile}
            onChange={(v) => set("profile", v)}
            options={[
              { value: "generic", label: "通用" },
              { value: "kindle", label: "Kindle" },
              { value: "tablet", label: "平板" },
              { value: "phone", label: "手机" },
            ]}
          />
        </QuickItem>
        <QuickItem label="目录">
          <Segmented
            compact
            value={value.insertToc ? "on" : "off"}
            onChange={(v) => set("insertToc", v === "on")}
            options={[
              { value: "on", label: "生成" },
              { value: "off", label: "不生成" },
            ]}
          />
        </QuickItem>
      </QuickBar>

      <Group title="排版" desc="字体、间距与设备适配">
        <Field label="质量档位">
          <Segmented<Quality>
            value={value.quality}
            onChange={(q) => set("quality", q)}
            options={[
              { value: "high", label: QUALITY_LABEL.high },
              { value: "balanced", label: QUALITY_LABEL.balanced },
              { value: "small", label: QUALITY_LABEL.small },
            ]}
          />
        </Field>

        <TargetField domain="ebook" value={value.defaultTarget} onChange={(v) => set("defaultTarget", v)} />

        <Field label="设备画像" hint="决定屏幕尺寸与翻页方式">
          <Select
            value={value.profile}
            onChange={(v) => set("profile", v)}
            options={EBOOK_PROFILES}
          />
        </Field>

        <Field label="基础字号" hint="单位 pt">
          <NumberInput
            value={value.baseFontSize}
            onChange={(v) => set("baseFontSize", v)}
            min={8}
            max={32}
            suffix="pt"
          />
        </Field>

        <Field label="行距倍数">
          <Slider
            value={value.lineHeight}
            onChange={(v) => set("lineHeight", v)}
            min={1}
            max={2.5}
            step={0.05}
            format={(v) => v.toFixed(2)}
          />
        </Field>

        <Field label="页边距" hint="单位 pt">
          <NumberInput
            value={value.margin}
            onChange={(v) => set("margin", v)}
            min={0}
            max={120}
            suffix="pt"
          />
        </Field>

        <Field label="嵌入字体" hint="正文字体族名，空 = 设备默认">
          <TextInput
            value={value.fontFamily}
            onChange={(v) => set("fontFamily", v)}
            placeholder="如 思源宋体"
          />
        </Field>

        <Field label="两端对齐" hint="关闭后正文左对齐">
          <Toggle
            checked={value.justify}
            onChange={(v) => set("justify", v)}
            label={value.justify ? "已启用" : "左对齐"}
          />
        </Field>
      </Group>

      <Group title="元数据" desc="留空保留源文件的书名与作者">
        <Field label="书名">
          <TextInput value={value.title} onChange={(v) => set("title", v)} placeholder="保留源数据" />
        </Field>
        <Field label="作者">
          <TextInput value={value.author} onChange={(v) => set("author", v)} placeholder="保留源数据" />
        </Field>
      </Group>

      <Group title="图片与结构">
        <Field label="内嵌图片质量" hint="1–100">
          <Slider
            value={value.jpegQuality}
            onChange={(v) => set("jpegQuality", v)}
            min={1}
            max={100}
            format={(v) => `${v}`}
          />
        </Field>

        <Field label="插入目录" hint="按章节标题生成可跳转目录">
          <Toggle
            checked={value.insertToc}
            onChange={(v) => set("insertToc", v)}
            label={value.insertToc ? "已启用" : "已关闭"}
          />
        </Field>
      </Group>
    </>
  );
}

// ---------------------------------------------------------------- 文档

const DOC_VIA = [
  { value: "auto", label: "自动选择（推荐）" },
  { value: "pandoc", label: "Pandoc · 保真度高" },
  { value: "soffice", label: "LibreOffice · 兼容性最好" },
];

const PDF_ENGINES = [
  { value: "weasyprint", label: "WeasyPrint · 轻量" },
  { value: "latex", label: "LaTeX · 学术排版" },
  { value: "chromium", label: "Chromium · 还原网页" },
  { value: "wkhtmltopdf", label: "wkhtmltopdf" },
];

export function DocumentPanel({
  value,
  onChange,
}: {
  value: DocumentSettings;
  onChange: (v: DocumentSettings) => void;
}) {
  const set = <K extends keyof DocumentSettings>(k: K, v: DocumentSettings[K]) =>
    onChange({ ...value, [k]: v });

  return (
    <>
      <QuickBar>
        <QuickItem label="输出">
          <Segmented
            compact
            value={value.defaultTarget}
            onChange={(v) => set("defaultTarget", v)}
            options={[
              { value: "", label: "自动" },
              { value: "pdf", label: "PDF" },
              { value: "docx", label: "Word" },
              { value: "txt", label: "TXT" },
              { value: "html", label: "HTML" },
            ]}
          />
        </QuickItem>
        <QuickItem label="通道">
          <Segmented
            compact
            value={value.via}
            onChange={(v) => set("via", v)}
            options={[
              { value: "auto", label: "自动" },
              { value: "pandoc", label: "Pandoc" },
              { value: "soffice", label: "LibreOffice" },
            ]}
          />
        </QuickItem>
      </QuickBar>

      <Group title="转换通道" desc="不同引擎对同一格式的表现差异">
        <TargetField domain="document" value={value.defaultTarget} onChange={(v) => set("defaultTarget", v)} />

        <Field label="转换方式">
          <Select value={value.via} onChange={(v) => set("via", v)} options={DOC_VIA} />
        </Field>
        <Field label="PDF 排版引擎" hint="导出 PDF 时使用">
          <Select
            value={value.pdfEngine}
            onChange={(v) => set("pdfEngine", v)}
            options={PDF_ENGINES}
          />
        </Field>
      </Group>

      <Group title="结构" desc="目录生成">
        <Field label="自动生成目录" hint="Pandoc 输出 docx / pdf / epub 时在开头插入目录">
          <Toggle
            checked={value.toc}
            onChange={(v) => set("toc", v)}
            label={value.toc ? "已启用" : "已关闭"}
          />
        </Field>
      </Group>

      <Group title="说明" defaultOpen={false}>
        <div className="prose">
          <p>
            <strong>Pandoc</strong> 适合 Markdown / HTML 等结构化文本，保真度最高；
            <strong>LibreOffice</strong> 适合 Office 原生格式，兼容性好但对复杂排版可能失真。
          </p>
          <p>
            PDF 源文件转可编辑格式时，程序会自动改用 Calibre 提取，因为它的文本与版面还原比
            Office 引擎更可靠。
          </p>
        </div>
      </Group>
    </>
  );
}
