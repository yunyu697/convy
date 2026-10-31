//! 端到端测试：用真实的外部引擎跑完整转换链路。
//!
//! 这些测试依赖系统已安装的工具，缺少对应引擎时会自动跳过，
//! 因此可以在任意开发机上运行。

use convy_lib::engines;
use convy_lib::formats;
use convy_lib::formats::Domain;
use convy_lib::job::Job;
use convy_lib::probe::Engines;
use convy_lib::runner;
use convy_lib::settings::{HwMode, Settings};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("convy-test-{}", name));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("create temp dir");
    d
}

/// 托管引擎目录（模拟应用数据目录下的 engines/）
fn managed_root(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("convy-managed-{}", name));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("create managed dir");
    d
}

fn make_video(dir: &Path) -> Option<PathBuf> {
    let out = dir.join("input.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=15:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&out)
        .status()
        .ok()?;
    if status.success() && out.exists() {
        Some(out)
    } else {
        None
    }
}

fn make_png(dir: &Path) -> Option<PathBuf> {
    let out = dir.join("input.png");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x480:duration=1",
            "-frames:v",
            "1",
        ])
        .arg(&out)
        .status()
        .ok()?;
    if status.success() && out.exists() {
        Some(out)
    } else {
        None
    }
}

/// 复刻 App 的执行路径：按编码器降级链依次尝试，失败就换下一个。
///
/// 这一点很关键——硬件编码器（比如没装 CUDA 运行时的 NVENC）第一次会失败，
/// App 靠这条链自动回退到软件编码器。测试必须走同一条路径才真实。
async fn convert(engines: &Engines, job: &Job, output: &Path) -> runner::RunOutcome {
    let settings = Settings::default();
    let domain = job.domain.unwrap_or(Domain::Video);
    let options = settings.to_options(domain);
    let chain = convy_lib::compute_enc_chain(engines, &options, &job.target_format);

    let cb: runner::ProgressFn = Arc::new(|_, _| {});
    let mut last = runner::RunOutcome::default();

    for (attempt, forced) in chain
        .iter()
        .enumerate()
        .map(|(i, s)| (i, Some(s.as_str())))
        .chain(std::iter::once((chain.len(), None)))
    {
        let plan = match engines::plan_for(engines, job, &options, output, forced).await {
            Ok(p) => p,
            Err(e) => {
                last.stderr = e;
                last.success = false;
                break;
            }
        };
        let outcome =
            runner::execute(&plan, Arc::new(AtomicBool::new(false)), cb.clone(), None).await;
        if outcome.success {
            return outcome;
        }
        if outcome.canceled {
            return outcome;
        }
        last = outcome;
        // 还有下一个候选就继续尝试
        let _ = attempt;
    }

    last
}

// 这些是纯逻辑测试，不依赖任何外部工具

#[test]
fn format_matrix_routes_media_across_domains() {
    let video_targets = formats::targets_for("mp4");
    assert!(video_targets.contains(&"mkv"));
    assert!(video_targets.contains(&"mp3"), "视频应能直接抽音频");
    assert!(video_targets.contains(&"gif"));
    // 媒体域允许目标格式与源格式相同（只改编码/参数不改容器）
    assert!(video_targets.contains(&"mp4"), "视频应允许同格式转码");
    assert!(
        formats::targets_for("png").contains(&"png"),
        "图片应允许同格式重压缩"
    );
    // 文档/电子书同格式没有转换意义，仍排除自身
    assert!(!formats::targets_for("docx").contains(&"docx"));
    assert!(!formats::targets_for("epub").contains(&"epub"));
}

/// 同格式转换时，输出文件绝不能覆盖源文件。
#[test]
fn same_format_output_never_overwrites_source() {
    let dir = tmpdir("same-format");
    let src = dir.join("clip.mp4");
    std::fs::write(&src, b"x").unwrap();

    let job = Job::new(src.clone(), "mp4".into());
    let out1 = job.resolve_output(false, "", false);
    assert_ne!(out1, src, "同目录同格式输出不得等于源文件");

    // 再来一个任务也不应与源文件或上一个输出冲突
    std::fs::write(&out1, b"y").unwrap();
    let job2 = Job::new(src.clone(), "mp4".into());
    let out2 = job2.resolve_output(false, "", false);
    assert_ne!(out2, src);
    assert_ne!(out2, out1);
}

#[test]
fn format_matrix_handles_special_cases() {
    assert!(formats::targets_for("pdf").contains(&"epub"));
    assert!(formats::targets_for("xlsx").contains(&"csv"));
    assert!(formats::targets_for("md").contains(&"docx"));
    assert!(formats::targets_for("docx").contains(&"pdf"));
    assert!(formats::targets_for("unknown_ext").is_empty());
}

#[test]
fn domain_detection() {
    assert_eq!(formats::domain_of("mp4"), Some(Domain::Video));
    assert_eq!(
        formats::domain_of("FLAC"),
        Some(Domain::Audio),
        "应大小写不敏感"
    );
    assert_eq!(formats::domain_of("heic"), Some(Domain::Image));
    assert_eq!(formats::domain_of("azw3"), Some(Domain::Ebook));
    assert_eq!(formats::domain_of("xlsx"), Some(Domain::Document));
}

// ---------------------------------------------------------------- 设置层

/// 每个模块的设置必须只影响自己的域，不能串味。
#[test]
fn per_domain_settings_do_not_leak_across_domains() {
    let mut s = Settings::default();
    s.video.crf = 12;
    s.video.resolution = convy_lib::settings::ResolutionMode::R1080p;
    s.video.audio_bitrate = 160;
    s.audio.bitrate = 320;
    s.image.jpg_quality = 30;

    let v = s.to_options(Domain::Video);
    assert_eq!(v.crf, Some(12));
    assert_eq!(v.scale, Some((1920, 1080)));
    assert_eq!(
        v.audio_bitrate_kb,
        Some(160),
        "视频音轨用视频模块自己的码率"
    );
    assert_eq!(v.jpg_quality, None, "图片参数不应进入视频");

    let a = s.to_options(Domain::Audio);
    assert_eq!(a.audio_bitrate_kb, Some(320));
    assert_eq!(a.crf, None, "视频参数不应进入音频");
    assert_eq!(a.scale, None);

    let i = s.to_options(Domain::Image);
    assert_eq!(i.jpg_quality, Some(30));
    assert_eq!(i.crf, None);
    assert_eq!(i.width, None, "未设缩放时宽度应为 None（保持原尺寸）");
}

/// 0 值应表示「保持源设置」而不是「改成 0」。
#[test]
fn zero_valued_options_mean_keep_source() {
    let s = Settings::default();
    let a = s.to_options(Domain::Audio);
    assert_eq!(a.sample_rate, None);
    assert_eq!(a.channels, None);
    assert_eq!(a.audio_bitrate_kb, Some(192));
}

/// 硬件加速模式必须原样传递到引擎层。
#[test]
fn hardware_mode_is_propagated() {
    let mut s = Settings::default();
    s.video.hardware = HwMode::Force;
    s.video.encoder = "h264_nvenc".into();

    let v = s.to_options(Domain::Video);
    assert_eq!(v.hardware, HwMode::Force);
    assert_eq!(v.encoder.as_deref(), Some("h264_nvenc"));

    s.video.hardware = HwMode::Off;
    let v = s.to_options(Domain::Video);
    assert_eq!(v.hardware, HwMode::Off);
}

/// 旋转 / 翻转 / 音量增益 / 速度档必须透传到 FFmpeg 参数里。
#[tokio::test]
async fn video_plan_carries_picture_and_volume_filters() {
    use convy_lib::probe::Engines;

    let engines = Engines {
        ffmpeg: Some(PathBuf::from("ffmpeg")),
        ..Default::default()
    };
    let dir = tmpdir("plan-filters");
    let src = dir.join("in.mp4");
    std::fs::write(&src, b"x").unwrap();
    let job = Job::new(src, "mp4".into());

    let mut s = Settings::default();
    s.video.rotation = "90".into();
    s.video.flip_h = true;
    s.video.volume_db = -6;
    s.video.speed = "veryslow".into();
    s.video.audio_sample_rate = 48000;
    let opts = s.to_options(Domain::Video);

    let plan = engines::plan_for(&engines, &job, &opts, &dir.join("out.mp4"), None)
        .await
        .expect("build plan");
    let joined = plan
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        joined.contains("transpose=1"),
        "旋转 90° 应产生 transpose=1：{}",
        joined
    );
    assert!(joined.contains("hflip"), "水平翻转应产生 hflip：{}", joined);
    assert!(
        joined.contains("volume=-6dB"),
        "音量增益应产生 volume 滤镜：{}",
        joined
    );
    assert!(
        joined.contains("veryslow"),
        "速度档应传给 -preset：{}",
        joined
    );
    assert!(joined.contains("-ar"), "采样率应产生 -ar：{}", joined);
    assert!(joined.contains("48000"));
}

/// 音频域的音量增益同样生效；归一化开启时优先用 loudnorm。
#[tokio::test]
async fn audio_plan_prefers_loudnorm_over_gain() {
    use convy_lib::probe::Engines;

    let engines = Engines {
        ffmpeg: Some(PathBuf::from("ffmpeg")),
        ..Default::default()
    };
    let dir = tmpdir("plan-audio");
    let src = dir.join("in.mp3");
    std::fs::write(&src, b"x").unwrap();
    let job = Job::new(src, "mp3".into());

    let mut s = Settings::default();
    s.audio.volume_db = 4;
    s.audio.normalize = true;
    let opts = s.to_options(Domain::Audio);

    let plan = engines::plan_for(&engines, &job, &opts, &dir.join("out.mp3"), None)
        .await
        .expect("build plan");
    let joined = plan
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(joined.contains("loudnorm"), "归一化优先：{}", joined);
    assert!(
        !joined.contains("volume=4dB"),
        "开了归一化就不该再叠加增益：{}",
        joined
    );
}

/// 图片域的旋转 / 翻转要落到 ImageMagick 参数；JPG 输出要拍平透明通道。
#[tokio::test]
async fn image_plan_applies_rotation_and_flatten() {
    use convy_lib::probe::Engines;

    let engines = Engines {
        magick: Some(PathBuf::from("magick")),
        ..Default::default()
    };
    let dir = tmpdir("plan-image");
    let src = dir.join("in.png");
    std::fs::write(&src, b"x").unwrap();
    let job = Job::new(src, "jpg".into());

    let mut s = Settings::default();
    s.image.rotation = "270".into();
    s.image.flip_v = true;
    s.image.background = "#ffffff".into();
    let opts = s.to_options(Domain::Image);

    let plan = engines::plan_for(&engines, &job, &opts, &dir.join("out.jpg"), None)
        .await
        .expect("build plan");
    let joined = plan
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(joined.contains("-rotate"), "应有旋转参数：{}", joined);
    assert!(joined.contains("270"), "旋转角度应正确：{}", joined);
    assert!(joined.contains("-flip"), "垂直翻转应产生 -flip：{}", joined);
    assert!(joined.contains("#ffffff"), "背景色应传入：{}", joined);
    assert!(
        joined.contains("-alpha"),
        "JPG 输出应拍平透明通道：{}",
        joined
    );
}

/// 每个模块都能指定自己的默认目标格式，且互不影响。
#[test]
fn default_target_is_per_domain() {
    let mut s = Settings::default();
    s.video.default_target = "mkv".into();
    s.image.default_target = "webp".into();

    assert_eq!(s.default_target(Domain::Video), "mkv");
    assert_eq!(s.default_target(Domain::Image), "webp");
    assert_eq!(s.default_target(Domain::Audio), "", "未设置的域应为空");
}

/// 内置预设必须能被解析成对应域的设置，且字段名与前端 camelCase 一致。
#[test]
fn builtin_presets_are_valid_and_scoped() {
    let presets = convy_lib::settings::builtin_presets();
    assert!(presets.len() >= 12, "内置预设数量偏少：{}", presets.len());

    for p in &presets {
        assert!(p.builtin, "内置预设应标记 builtin");
        assert!(p.settings.is_object(), "预设内容必须是对象");
    }

    // 预设应覆盖全部五个域
    for d in [
        Domain::Video,
        Domain::Audio,
        Domain::Image,
        Domain::Ebook,
        Domain::Document,
    ] {
        assert!(
            presets.iter().any(|p| p.domain == d),
            "缺少 {:?} 域的预设",
            d
        );
    }

    // 视频预设的字段名必须与 VideoSettings 的 serde 名一致
    let v = presets
        .iter()
        .find(|p| p.domain == Domain::Video)
        .expect("有视频预设");
    let obj = v.settings.as_object().expect("对象");
    for key in [
        "quality",
        "resolution",
        "videoCodec",
        "crf",
        "audioCodec",
        "hardware",
        "faststart",
        "deinterlace",
    ] {
        assert!(obj.contains_key(key), "视频预设缺少字段 {}", key);
    }
}

/// 设置文件缺字段时应补齐默认值而不是解析失败。
#[test]
fn settings_survive_partial_json() {
    let partial = r#"{"video":{"crf":18}}"#;
    let s: Settings = serde_json::from_str(partial).expect("应能解析");
    assert_eq!(s.video.crf, 18);
    assert_eq!(s.video.audio_bitrate, 128, "缺失字段应回落默认值");
    assert_eq!(s.global.concurrent_jobs, 2);
}

/// 输出目录与覆盖策略必须落到实际路径解析上。
#[test]
fn output_path_respects_global_output_folder() {
    let dir = tmpdir("outdir");
    let src = dir.join("clip.mp4");
    std::fs::write(&src, b"x").unwrap();
    let out = dir.join("converted");
    std::fs::create_dir_all(&out).unwrap();

    let job = Job::new(src, "mp4".into());
    let resolved = job.resolve_output(false, out.to_string_lossy().as_ref(), false);
    assert_eq!(
        resolved.parent().unwrap(),
        out.as_path(),
        "应输出到全局目录"
    );
    assert_eq!(resolved.file_name().unwrap(), "clip.mp4");
}

// 以下测试需要真实引擎

#[tokio::test]
async fn video_mp4_to_webm_end_to_end() {
    let dir = tmpdir("mp4-webm");
    let Some(input) = make_video(&dir) else {
        eprintln!("跳过：ffmpeg 不可用");
        return;
    };
    let engines = Engines::detect(&dir, &managed_root("detect"));
    if engines.ffmpeg.is_none() {
        eprintln!("跳过：未探测到 ffmpeg");
        return;
    }

    let job = Job::new(input, "webm".into());
    let output = dir.join("output.webm");
    let outcome = convert(&engines, &job, &output).await;

    assert!(outcome.success, "转换失败：{}", outcome.stderr);
    let size = std::fs::metadata(&output).expect("输出文件应存在").len();
    assert!(size > 1000, "输出文件过小：{} 字节", size);
}

#[tokio::test]
async fn video_mp4_to_mp3_extracts_audio() {
    let dir = tmpdir("mp4-mp3");
    let Some(input) = make_video(&dir) else {
        eprintln!("跳过：ffmpeg 不可用");
        return;
    };
    let engines = Engines::detect(&dir, &managed_root("detect"));
    if engines.ffmpeg.is_none() {
        eprintln!("跳过：未探测到 ffmpeg");
        return;
    }

    let job = Job::new(input, "mp3".into());
    let output = dir.join("output.mp3");
    let outcome = convert(&engines, &job, &output).await;

    assert!(outcome.success, "转换失败：{}", outcome.stderr);
    assert!(std::fs::metadata(&output)
        .map(|m| m.len() > 1000)
        .unwrap_or(false));
}

#[tokio::test]
async fn image_png_to_jpg_end_to_end() {
    let dir = tmpdir("png-jpg");
    let Some(input) = make_png(&dir) else {
        eprintln!("跳过：ffmpeg 不可用");
        return;
    };
    let engines = Engines::detect(&dir, &managed_root("detect"));
    if engines.magick.is_none() && engines.vips.is_none() && engines.ffmpeg.is_none() {
        eprintln!("跳过：未探测到任何图片引擎");
        return;
    }

    let job = Job::new(input, "jpg".into());
    let output = dir.join("output.jpg");
    let outcome = convert(&engines, &job, &output).await;

    assert!(outcome.success, "转换失败：{}", outcome.stderr);
    assert!(std::fs::metadata(&output)
        .map(|m| m.len() > 500)
        .unwrap_or(false));
}

#[tokio::test]
async fn engine_report_has_no_false_positives() {
    let dir = tmpdir("report");
    let engines = Engines::detect(&dir, &managed_root("detect"));
    let report = engines.report();

    assert_eq!(report.ffmpeg, engines.ffmpeg.is_some());
    assert_eq!(report.ebook, engines.ebook_convert.is_some());
    assert_eq!(report.document, engines.soffice.is_some());

    if report.ffmpeg {
        assert!(!engines.h264_chain.is_empty(), "有 ffmpeg 就必须有编码器链");
    }
}

/// Windows 的 system32 里有个同名 convert.exe（卷转换工具），
/// 早期版本会把它误认成 ImageMagick，导致所有图片转换报"无效参数"。
#[tokio::test]
async fn detection_is_not_fooled_by_same_name_tools() {
    let dir = tmpdir("trap");
    let engines = Engines::detect(&dir, &managed_root("detect"));

    if let Some(m) = &engines.magick {
        let out = std::process::Command::new(m)
            .arg("--version")
            .output()
            .expect("run version");
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        assert!(
            text.contains("imagemagick"),
            "把 {:?} 误认成了 ImageMagick",
            m
        );
    }
}

// ---------------------------------------------------------------- 保持源参数

/// 「保持源参数」必须用源文件的参数覆盖面板默认值：
/// 这是「只改编码（H.264→H.265、MP3→AAC），其余与原文件一致」的核心保证。
#[test]
fn keep_source_overrides_panel_defaults_with_source_params() {
    use convy_lib::probe::SourceStreams;
    use convy_lib::settings::{apply_keep_source, ResolutionMode};

    let mut s = Settings::default();
    // 面板上全是「自定义」值，模拟用户之前调过压缩参数
    s.video.resolution = ResolutionMode::R1080p;
    s.video.crf = 18;
    s.video.fps = "30".into();
    s.video.audio_bitrate = 320;
    s.video.audio_sample_rate = 48000;
    s.video.audio_channels = 2;

    let mut o = s.to_options(Domain::Video);
    assert!(o.keep_source, "视频默认应为保持源参数");

    let src = SourceStreams {
        video_bitrate_kbps: Some(4500),
        audio_bitrate_kbps: Some(192),
        sample_rate: Some(44100),
        channels: Some(2),
    };
    apply_keep_source(&mut o, &src);

    assert_eq!(o.scale, None, "分辨率应回到源文件，而不是 1080p");
    assert_eq!(o.fps, None, "帧率应回到源文件，而不是 30");
    assert_eq!(o.crf, None, "不应再强制 CRF");
    assert_eq!(o.video_bitrate_kb, Some(4500), "视频码率应取源文件");
    assert_eq!(o.audio_bitrate_kb, Some(192), "音频码率应取源文件");
    assert_eq!(o.sample_rate, Some(44100), "采样率应取源文件");
    assert_eq!(o.channels, Some(2));
}

/// 关闭「保持源参数」时，面板参数必须原样生效（否则预设会失效）。
#[test]
fn custom_mode_keeps_panel_values() {
    use convy_lib::settings::ResolutionMode;

    let mut s = Settings::default();
    s.video.keep_source = false;
    s.video.resolution = ResolutionMode::R720p;
    s.video.crf = 30;

    let o = s.to_options(Domain::Video);
    assert!(!o.keep_source);
    assert_eq!(o.scale, Some((1280, 720)));
    assert_eq!(o.crf, Some(30));
}

/// 音频域的 keepSource 同样要透传到转换参数。
#[test]
fn keep_source_flag_propagates_for_audio() {
    let mut s = Settings::default();
    assert!(
        s.to_options(Domain::Audio).keep_source,
        "音频默认保持源参数"
    );

    s.audio.keep_source = false;
    assert!(!s.to_options(Domain::Audio).keep_source);
}

/// 端到端验证「保持源参数」：源文件 480x270 / 12fps / 22050Hz 单声道，
/// 面板故意设成 1080p / 30fps / 48000Hz / 立体声，只把编码换成 H.265 + AAC。
/// 输出必须完全沿用源文件的参数，而不是面板里的值。
#[tokio::test]
async fn keep_source_preserves_source_stream_params_end_to_end() {
    let dir = tmpdir("keep-source-e2e");

    let src = dir.join("src.mp4");
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=480x270:rate=12:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=22050:duration=1",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-ac",
            "1",
            "-ar",
            "22050",
            "-shortest",
        ])
        .arg(&src)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok || !src.exists() {
        eprintln!("跳过：ffmpeg 不可用");
        return;
    }

    let engines = Engines::detect(&dir, &managed_root("detect"));
    let (Some(_), Some(ffprobe)) = (engines.ffmpeg.as_ref(), engines.ffprobe.as_ref()) else {
        eprintln!("跳过：未探测到 ffmpeg/ffprobe");
        return;
    };

    let mut s = Settings::default();
    s.video.keep_source = true;
    s.video.video_codec = "hevc".into();
    s.video.audio_codec = "aac".into();
    // 面板上设成完全不同的值，验证会被源文件覆盖
    s.video.resolution = convy_lib::settings::ResolutionMode::R1080p;
    s.video.fps = "30".into();
    s.video.audio_sample_rate = 48000;
    s.video.audio_channels = 2;
    let options = s.to_options(Domain::Video);

    let job = Job::new(src.clone(), "mkv".into());
    let output = dir.join("out.mkv");

    let chain = convy_lib::compute_enc_chain(&engines, &options, "mkv");
    let cb: runner::ProgressFn = Arc::new(|_, _| {});
    let mut outcome = runner::RunOutcome::default();
    let attempts = chain
        .iter()
        .map(|c| Some(c.as_str()))
        .chain(std::iter::once(None));
    for forced in attempts {
        let plan = match engines::plan_for(&engines, &job, &options, &output, forced).await {
            Ok(p) => p,
            Err(e) => {
                outcome.stderr = e;
                break;
            }
        };
        outcome = runner::execute(&plan, Arc::new(AtomicBool::new(false)), cb.clone(), None).await;
        if outcome.success {
            break;
        }
    }
    assert!(outcome.success, "转换失败：{}", outcome.stderr);

    let probe = |entries: &str, stream: &str| -> String {
        let out = std::process::Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                stream,
                "-show_entries",
                entries,
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(&output)
            .output()
            .expect("run ffprobe");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let dims = probe("stream=width,height", "v:0");
    let fps = probe("stream=r_frame_rate", "v:0");
    let ar = probe("stream=sample_rate", "a:0");
    let ac = probe("stream=channels", "a:0");

    assert!(
        dims.contains("480") && dims.contains("270"),
        "分辨率应保持源文件 480x270，实际 {dims}"
    );
    assert!(fps.starts_with("12"), "帧率应保持源文件 12，实际 {fps}");
    assert_eq!(ar, "22050", "采样率应保持源文件");
    assert_eq!(ac, "1", "声道应保持源文件单声道");
}

// ---------------------------------------------------------------- 批量导入

/// 拖入文件夹时递归收集受支持的文件，并记下相对子目录；
/// 隐藏文件、不支持的文件、重复根目录都要正确处理。
#[test]
fn folder_scan_recurses_and_records_relative_dirs() {
    let dir = tmpdir("scan-tree");
    let root = dir.join("album");
    std::fs::create_dir_all(root.join("2024/asia")).unwrap();
    std::fs::create_dir_all(root.join("2024/europe")).unwrap();

    std::fs::write(root.join("cover.jpg"), b"x").unwrap();
    std::fs::write(root.join("2024/asia/a.mp4"), b"x").unwrap();
    std::fs::write(root.join("2024/asia/notes.md"), b"x").unwrap();
    std::fs::write(root.join("2024/europe/b.mp3"), b"x").unwrap();
    std::fs::write(root.join("2024/europe/clip.bin"), b"x").unwrap();
    std::fs::write(root.join(".hidden.mp4"), b"x").unwrap();
    std::fs::write(root.join("readme"), b"x").unwrap();

    let roots = vec![
        root.to_string_lossy().to_string(),
        // 同一根目录再拖一次，不应重复收集
        root.to_string_lossy().to_string(),
    ];
    let scanned = convy_lib::import::scan(&roots, |ext| {
        !convy_lib::formats::targets_for(ext).is_empty()
    });

    let names: Vec<String> = scanned
        .files
        .iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names.len(), 4, "应收集 4 个受支持文件，实际 {names:?}");
    assert!(names.contains(&"cover.jpg".to_string()));
    assert!(!names.contains(&"clip.bin".to_string()), ".bin 不支持");

    let rel_of = |name: &str| {
        scanned
            .files
            .iter()
            .find(|(p, _)| p.file_name().unwrap().to_string_lossy() == name)
            .map(|(_, r)| r.clone())
            .unwrap_or_default()
    };
    assert_eq!(rel_of("cover.jpg"), "", "根目录下的文件没有子目录");
    assert_eq!(rel_of("a.mp4"), "2024/asia", "应记录嵌套层级");
    assert_eq!(rel_of("b.mp3"), "2024/europe");
    assert!(scanned.unsupported >= 1, "不支持的扩展名要计入统计");
    assert!(scanned.hidden >= 1, "隐藏文件要计入统计");
    assert!(!scanned.truncated);
}

/// 单个文件（非文件夹）也要能正常导入，且没有子目录。
#[test]
fn scan_accepts_plain_files() {
    let dir = tmpdir("scan-file");
    let f = dir.join("one.png");
    std::fs::write(&f, b"x").unwrap();

    let scanned = convy_lib::import::scan(&[f.to_string_lossy().to_string()], |ext| {
        !convy_lib::formats::targets_for(ext).is_empty()
    });
    assert_eq!(scanned.files.len(), 1);
    assert_eq!(scanned.files[0].1, "");
}

/// 保持目录结构：输出落到 输出目录/子目录/ 下；关闭时全部平铺在输出目录。
#[test]
fn preserve_tree_mirrors_subdirs_in_output_folder() {
    let dir = tmpdir("preserve-tree");
    let src = dir.join("a.mp4");
    std::fs::write(&src, b"x").unwrap();
    let out = dir.join("converted");

    let job = Job::new(src, "mkv".into()).in_subdir("2024/asia".into());

    let kept = job.resolve_output(false, out.to_string_lossy().as_ref(), true);
    assert_eq!(
        kept.parent().unwrap(),
        out.join("2024").join("asia").as_path(),
        "开启保持目录结构时应还原层级"
    );
    assert_eq!(kept.file_name().unwrap(), "a.mkv");

    let flat = job.resolve_output(false, out.to_string_lossy().as_ref(), false);
    assert_eq!(flat.parent().unwrap(), out.as_path(), "关闭时应平铺");

    // 没设输出目录时，仍然回到源文件目录，不受子目录影响
    let same = job.resolve_output(false, "", true);
    assert_eq!(same.parent().unwrap(), dir.as_path());
}

/// 子目录字符串里混入 `..` 也不能越出输出目录。
#[test]
fn preserve_tree_rejects_path_escapes() {
    let dir = tmpdir("preserve-escape");
    let src = dir.join("a.mp4");
    std::fs::write(&src, b"x").unwrap();
    let out = dir.join("converted");

    let job = Job::new(src, "mkv".into()).in_subdir("../../etc".into());
    let p = job.resolve_output(false, out.to_string_lossy().as_ref(), true);
    assert_eq!(p.parent().unwrap(), out.join("etc").as_path());
}

// ---------------------------------------------------------------- 引擎版本检测

/// 版本比较必须按数字逐段比，不能按字符串比（"10.0" 应大于 "9.0.2"）。
#[test]
fn version_compare_is_numeric_not_lexicographic() {
    use convy_lib::engines::registry::version_cmp;
    use std::cmp::Ordering;

    assert_eq!(version_cmp("9.0.2", "9.0.1"), Ordering::Greater);
    assert_eq!(version_cmp("10.0", "9.0.2"), Ordering::Greater);
    assert_eq!(version_cmp("9.0.2", "9.0.2"), Ordering::Equal);
    assert_eq!(version_cmp("26.8.1", "25.8.7"), Ordering::Greater);
    // ImageMagick 的补丁号在连字符后，仍应参与比较
    assert_eq!(version_cmp("7.1.2-34", "7.1.2"), Ordering::Greater);
    // v 前缀 / 少写一段都要等价
    assert_eq!(version_cmp("v9.17.0", "9.17.0"), Ordering::Equal);
    assert_eq!(version_cmp("3.13", "3.13.0"), Ordering::Equal);
}

/// 目录页会同时列出多个分支，`pick: max` 必须取最大的那个。
#[test]
fn latest_version_picks_first_or_max() {
    use convy_lib::engines::registry::pick_version;

    let listing = r#"
      <a href="25.8.7/">25.8.7/</a>
      <a href="26.2.6/">26.2.6/</a>
      <a href="26.8.1/">26.8.1/</a>
    "#;
    let re = r"([0-9]+\.[0-9]+\.[0-9]+)/";
    assert_eq!(pick_version(listing, re, true).as_deref(), Some("26.8.1"));
    assert_eq!(pick_version(listing, re, false).as_deref(), Some("25.8.7"));

    // GitHub API 的 JSON 里带 v 前缀
    let gh = "{\n  \"tag_name\": \"v9.17.0\",\n  \"name\": \"calibre 9.17.0\"\n}";
    assert_eq!(
        pick_version(gh, "\"tag_name\"\\s*:\\s*\"v?([0-9][0-9.]*)\"", false).as_deref(),
        Some("9.17.0")
    );

    assert_eq!(pick_version("没有任何版本号", r"(\d+\.\d+)", false), None);
}

/// 内置清单里每个平台都必须有可用的版本检测源——全部为 null 正是
/// 「检查更新永远说已是最新」的根因，这条测试把它钉住。
#[test]
fn builtin_manifest_declares_version_sources() {
    use convy_lib::engines::manifest::EngineManifest;

    let m = EngineManifest::load(&tmpdir("manifest-check"));
    assert!(m.engines.len() >= 5, "内置清单应包含 5 个引擎");
    for def in &m.engines {
        assert!(!def.downloads.is_empty(), "{} 没有任何平台下载源", def.id);
        for (key, spec) in &def.downloads {
            let check = spec
                .latest
                .as_ref()
                .unwrap_or_else(|| panic!("{} ({}) 缺少版本检测源", def.id, key));
            assert!(
                check.url.starts_with("https://"),
                "{} 的版本地址必须是 https",
                def.id
            );
            regex::Regex::new(&check.regex)
                .unwrap_or_else(|e| panic!("{} 版本正则无效：{e}", def.id));
            regex::Regex::new(&spec.version.regex)
                .unwrap_or_else(|e| panic!("{} 探测正则无效：{e}", def.id));
            assert!(!spec.bin.is_empty(), "{} 缺少可执行文件相对路径", def.id);
        }
    }
}

// ---------------------------------------------------------------- 托管目录布局

/// 发行包普遍多一层包装目录，展平后要能在 bin/ 下找到可执行文件，
/// 并且按清单裁剪掉 ffplay / doc 这类用不到的组件。
#[test]
fn extracted_wrapper_dir_is_flattened_and_trimmed() {
    use convy_lib::engines::layout;

    let dir = tmpdir("layout-flat");
    let staged = dir.join("staging");
    let inner = staged.join("ffmpeg-9.0.2-essentials_build");
    std::fs::create_dir_all(inner.join("bin")).unwrap();
    std::fs::create_dir_all(inner.join("doc")).unwrap();
    std::fs::write(inner.join("bin/ffmpeg.exe"), b"x").unwrap();
    std::fs::write(
        inner.join("bin/ffmpeg.exe".replace("ffmpeg", "ffplay")),
        b"x",
    )
    .unwrap();
    std::fs::write(inner.join("doc/readme.txt"), b"x").unwrap();

    let flattened = layout::normalize_layout(&staged);
    assert!(flattened >= 1, "应展平唯一的包装目录");
    assert!(
        staged.join("bin/ffmpeg.exe").is_file(),
        "展平后 bin 应在根目录下"
    );

    let removed = layout::prune(&staged, &["bin/ffplay.exe".to_string(), "doc".to_string()]);
    assert_eq!(removed.len(), 2, "ffplay 与 doc 应被裁掉");
    assert!(!staged.join("doc").exists());
    assert!(
        staged.join("bin/ffmpeg.exe").is_file(),
        "必需的 ffmpeg 不能删"
    );

    // 清单里的相对路径应命中
    let bin = layout::resolve_bin(&staged, "bin/ffmpeg.exe").expect("应找到 ffmpeg");
    assert!(bin.ends_with("bin/ffmpeg.exe"));
    // 清单路径与真实结构不一致时，也要能靠文件名兜底
    let by_name = layout::resolve_bin(&staged, "ffmpeg.exe").expect("应能按文件名兜底");
    assert!(by_name.ends_with("bin/ffmpeg.exe"));
}

/// 安装时记录了真实相对路径（.convy-bin），后续探测直接复用。
#[test]
fn bin_hint_is_reused_when_layout_changes() {
    use convy_lib::engines::layout;

    let dir = tmpdir("layout-hint");
    let version = dir.join("9.0.2");
    std::fs::create_dir_all(version.join("payload/bin")).unwrap();
    std::fs::write(version.join("payload/bin/ffmpeg.exe"), b"x").unwrap();
    layout::write_bin_hint(&version, "payload/bin/ffmpeg.exe").unwrap();

    let found = layout::resolve_bin(&version, "bin/ffmpeg.exe").expect("应通过 hint 找到");
    assert!(found.ends_with("payload/bin/ffmpeg.exe"));
}

/// 真机联网验证：用内置清单里的地址实际抓一遍各引擎的最新版本。
/// 默认 `#[ignore]`（CI 环境通常没网），本地验证用：
/// `cargo test --test convert -- --ignored --nocapture live_version`
#[tokio::test]
#[ignore]
async fn live_version_check_hits_official_endpoints() {
    use convy_lib::engines::manifest::platform_key;
    use convy_lib::engines::registry::EngineManager;

    let mgr = EngineManager::new(tmpdir("live-mgr"));
    let key = platform_key();
    let mut ok = 0;
    let mut failed: Vec<String> = Vec::new();
    for def in &mgr.manifest.engines {
        if !def.downloads.contains_key(&key) {
            continue;
        }
        match mgr.latest_version(&def.id).await {
            Ok(Some(v)) => {
                println!("  {:<12} => {}", def.id, v);
                ok += 1;
            }
            Ok(None) => failed.push(format!("{}: 地址没有返回版本号", def.id)),
            Err(e) => failed.push(format!("{}: {}", def.id, e)),
        }
    }
    assert!(ok >= 3, "至少要有 3 个引擎能查到版本，失败：{failed:?}");
}

/// 真机验证自检链路：本地生成素材 + 走真实转换管线。
/// `cargo test --test convert -- --ignored --nocapture live_selfcheck`
#[tokio::test]
#[ignore]
async fn live_selfcheck_all_domains() {
    use convy_lib::selfcheck;

    let dir = tmpdir("selfcheck-live");
    let engines = Engines::detect(&dir, &managed_root("selfcheck-detect"));
    let settings = Settings::default();
    let items = selfcheck::run(&engines, &settings, &dir.join("work")).await;

    assert_eq!(items.len(), 5, "五个类别都应有一项结果");
    for i in &items {
        println!(
            "{:<22} ok={:<5} skipped={:<5} {} {}",
            i.label,
            i.ok,
            i.skipped,
            i.message,
            i.detail.as_deref().unwrap_or("")
        );
    }
    // 至少视频这种最基础的能力必须能跑通（CI 装了 ffmpeg 时）
    if engines.ffmpeg.is_some() {
        let video = items.iter().find(|i| i.label.contains("视频")).unwrap();
        assert!(
            video.ok,
            "视频自检应通过：{}",
            video.detail.clone().unwrap_or_default()
        );
    }
}
