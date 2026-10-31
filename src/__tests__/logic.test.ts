import { describe, expect, it } from "vitest";
import { applyTargets, mergeSettings, pickStaleTargets } from "../logic";
import { DEFAULT_SETTINGS, type Job } from "../types";

function job(over: Partial<Job>): Job {
  return {
    id: "j1",
    source: "C:/in/a.mp4",
    fileName: "a.mp4",
    size: 1024,
    domain: "video",
    sourceExt: "mp4",
    targetFormat: "mkv",
    relDir: null,
    status: "pending",
    progress: 0,
    detail: null,
    outputPath: null,
    outputSize: null,
    error: null,
    startedAt: null,
    finishedAt: null,
    ...over,
  };
}

describe("pickStaleTargets", () => {
  it("只挑同域、待处理、格式不同的任务", () => {
    const jobs = [
      job({ id: "a" }), // 命中
      job({ id: "b", targetFormat: "mp4" }), // 已是目标格式
      job({ id: "c", status: "running" }), // 正在跑，不能改
      job({ id: "d", status: "done" }), // 已完成，改了文案会对不上产物
      job({ id: "e", domain: "audio", sourceExt: "flac" }), // 别的模块
    ];
    expect(pickStaleTargets(jobs, "video", "mp4").map((j) => j.id)).toEqual(["a"]);
  });

  it("目标格式为空时不动任何任务（空字符串代表「跟随推荐」）", () => {
    expect(pickStaleTargets([job({})], "video", "")).toEqual([]);
  });

  it("全部命中时返回全部", () => {
    const jobs = [job({ id: "a" }), job({ id: "b" })];
    expect(pickStaleTargets(jobs, "video", "webm")).toHaveLength(2);
  });
});

describe("applyTargets", () => {
  it("改掉命中行的格式，未命中的行保持同一个对象引用", () => {
    const a = job({ id: "a" });
    const b = job({ id: "b", targetFormat: "mp4" });
    const out = applyTargets([a, b], "video", "mp4");

    expect(out[0].targetFormat).toBe("mp4");
    expect(out[0]).not.toBe(a); // 改过的行是新对象
    expect(out[1]).toBe(b); // 没改的行复用引用，memo 的 JobRow 不会重渲染
  });

  it("没有需要改的行时原样返回入参数组", () => {
    const jobs = [job({ id: "a", targetFormat: "mp4" })];
    expect(applyTargets(jobs, "video", "mp4")).toBe(jobs);
  });

  it("不污染入参", () => {
    const jobs = [job({ id: "a" })];
    applyTargets(jobs, "video", "avi");
    expect(jobs[0].targetFormat).toBe("mkv");
  });
});

describe("mergeSettings", () => {
  it("后端返回完整设置时逐字段保留", () => {
    const custom = {
      ...DEFAULT_SETTINGS,
      global: { ...DEFAULT_SETTINGS.global, concurrentJobs: 5, theme: "dark" },
      video: { ...DEFAULT_SETTINGS.video, crf: 18 },
    };
    const merged = mergeSettings(custom);
    expect(merged.global.concurrentJobs).toBe(5);
    expect(merged.global.theme).toBe("dark");
    expect(merged.video.crf).toBe(18);
  });

  it("老配置缺字段时补默认值", () => {
    // 1.0.0 的 settings.json 里没有 preserveTree
    const merged = mergeSettings({
      global: { theme: "dark", concurrentJobs: 3 },
    });
    expect(merged.global.theme).toBe("dark");
    expect(merged.global.concurrentJobs).toBe(3);
    expect(merged.global.preserveTree).toBe(DEFAULT_SETTINGS.global.preserveTree);
    expect(merged.video.keepSource).toBe(DEFAULT_SETTINGS.video.keepSource);
  });

  it("null 与类型不符的字段一律丢弃，不让它漏进界面", () => {
    const merged = mergeSettings({
      video: { crf: null, speed: 123, resolution: "r1080p" },
      global: { theme: "dark" },
    });
    expect(merged.video.crf).toBe(DEFAULT_SETTINGS.video.crf);
    expect(merged.video.speed).toBe(DEFAULT_SETTINGS.video.speed);
    expect(merged.video.resolution).toBe("r1080p");
  });

  it("整块缺失或输入不是对象时，返回完整默认值", () => {
    for (const bad of [null, undefined, 42, "x", []]) {
      expect(mergeSettings(bad)).toEqual(DEFAULT_SETTINGS);
    }
  });

  it("合并结果一定是完整结构，每个域都不缺键", () => {
    const merged = mergeSettings({ image: { width: 800 } });
    for (const section of Object.keys(DEFAULT_SETTINGS) as (keyof typeof DEFAULT_SETTINGS)[]) {
      expect(Object.keys(merged[section]).sort()).toEqual(
        Object.keys(DEFAULT_SETTINGS[section]).sort(),
      );
    }
  });
});
