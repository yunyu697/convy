// 从 App.tsx 里抽出来的纯逻辑，单独放一处是为了能被单元测试直接覆盖。
// 这里的东西不允许有副作用、不允许碰 React 状态。

import { DEFAULT_SETTINGS, type Domain, type Job, type Settings } from "./types";

/**
 * 找出「目标格式该跟着变但还没变」的待处理任务。
 *
 * 只动 pending：正在跑或已完成的任务改目标格式没有意义，
 * 已完成的任务改了还会让「已转换到 mp4」这种文案与实际产物对不上。
 */
export function pickStaleTargets(jobs: Job[], domain: Domain, want: string): Job[] {
  if (!want) return [];
  return jobs.filter(
    (j) => j.status === "pending" && j.domain === domain && j.targetFormat !== want,
  );
}

/**
 * 按 pickStaleTargets 的结论生成新数组。
 *
 * 没变的行原样返回同一个对象引用 —— JobRow 是 memo 的，
 * 引用不变就不会重渲染，3000 行的队列改一次目标格式也不会卡。
 */
export function applyTargets(jobs: Job[], domain: Domain, want: string): Job[] {
  const stale = new Set(pickStaleTargets(jobs, domain, want).map((j) => j.id));
  if (!stale.size) return jobs;
  return jobs.map((j) => (stale.has(j.id) ? { ...j, targetFormat: want } : j));
}

/** 逐字段合并：缺的补默认值，类型不对的（null / 字符串混进来）一律丢弃 */
function mergeSection<T extends object>(base: T, raw: unknown): T {
  if (!raw || typeof raw !== "object") return { ...base };
  const src = raw as Record<string, unknown>;
  const baseRec = base as Record<string, unknown>;
  const out: Record<string, unknown> = { ...baseRec };
  for (const key of Object.keys(baseRec)) {
    const v = src[key];
    if (v === undefined || v === null) continue;
    if (typeof v !== typeof baseRec[key]) continue;
    out[key] = v;
  }
  return out as T;
}

/**
 * 把后端读回来的设置补齐成完整结构。
 *
 * 场景：老版本写下的 settings.json 缺新字段、文件被手工改坏、
 * 或者后端版本比前端旧。缺字段时如果直接用，界面上会出现
 * `undefined` 落进 <select>、`NaN` 进输入框这类很难排查的怪象，
 * 所以入口处统一补一遍默认值。
 */
export function mergeSettings(raw: unknown): Settings {
  const src = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    video: mergeSection(DEFAULT_SETTINGS.video, src.video),
    audio: mergeSection(DEFAULT_SETTINGS.audio, src.audio),
    image: mergeSection(DEFAULT_SETTINGS.image, src.image),
    ebook: mergeSection(DEFAULT_SETTINGS.ebook, src.ebook),
    document: mergeSection(DEFAULT_SETTINGS.document, src.document),
    global: mergeSection(DEFAULT_SETTINGS.global, src.global),
  };
}
