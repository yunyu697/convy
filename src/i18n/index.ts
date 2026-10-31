/**
 * 轻量 i18n。
 *
 * 为什么 key 直接用中文原文
 * --------------------------
 * 这个项目的界面文案是中文写出来的，先有文案后有 i18n。如果改成 `settings.update.title`
 * 这种命名 key，得为四百多条文案逐个起名字，而且改文案就要改 key、四份字典跟着一起动。
 * 直接拿中文原文当 key（gettext 的做法）：改文案时旧 key 失效会自动回退成新文案本身，
 * 漏翻的地方显示中文而不是显示一个 key 或者空白 —— 坏也坏得看得懂。
 *
 * 为什么不放 Context
 * ------------------
 * 语言要在 React 之外也能用（比如 api 层的错误提示、非组件函数），
 * Context 只能在组件里读。这里用模块级状态 + 订阅：
 * 组件里 `const t = useT()` 会在语言切换时自动重渲染，非组件代码直接 `t(...)`。
 */

import { useEffect, useState } from "react";
import en from "./en";
import es from "./es";
import ja from "./ja";
import zhHant from "./zh-Hant";

export type Lang = "zh-Hans" | "zh-Hant" | "en" | "ja" | "es";
/** 用户的选择：跟随系统，或指定某一门语言 */
export type LangPref = "system" | Lang;

export interface LangDef {
  id: LangPref;
  /** 语言名一律用它自己的语言写（简体中文 / 繁體中文 / English…），不翻译 */
  native: string;
}

export const LANGUAGES: LangDef[] = [
  { id: "system", native: "系统语言" },
  { id: "zh-Hans", native: "简体中文" },
  { id: "zh-Hant", native: "繁體中文" },
  { id: "en", native: "English" },
  { id: "ja", native: "日本語" },
  { id: "es", native: "Español" },
];

/**
 * 简体中文是源语言，字典为空：查不到就返回 key 本身，正好就是中文。
 *
 * 哪些 key 故意不进字典
 * --------------------
 * 编解码器、分辨率、采样率、格式名和品牌名（`H.264`、`Full HD · 1920×1080`、
 * `44.1 kHz · CD`、`LibreOffice`…）在这些语言里本来就该原样显示。它们照样走
 * `t()`，查不到就回退成自己，于是界面文案的写法保持一致，也不必为几十个技术名
 * 在四份字典里各留一行同样的内容。
 *
 * 导出是为了让测试能守住「四份字典的 key 集合与占位符必须一致」——
 * 新增一条 `t("…")` 却漏翻某个语种，肉眼很难发现，跑一次测试就能拦住。
 */
export const DICTS: Record<Lang, Record<string, string>> = {
  "zh-Hans": {},
  "zh-Hant": zhHant,
  en,
  ja,
  es,
};

/**
 * 系统语言 → 受支持的语言。
 *
 * 只认主干和少数常见地区变体：`zh-TW` / `zh-HK` 是繁体，其余 `zh-*` 按简体处理。
 * 一门都不匹配时按需求落到英语，而不是留在中文。
 */
const ALIASES: Record<string, Lang> = {
  zh: "zh-Hans",
  "zh-hans": "zh-Hans",
  "zh-cn": "zh-Hans",
  "zh-sg": "zh-Hans",
  "zh-hant": "zh-Hant",
  "zh-tw": "zh-Hant",
  "zh-hk": "zh-Hant",
  "zh-mo": "zh-Hant",
  en: "en",
  ja: "ja",
  es: "es",
};

export function detectSystemLang(): Lang {
  if (typeof navigator === "undefined") return "en";
  const candidates = [navigator.language, ...(navigator.languages ?? [])];
  for (const raw of candidates) {
    if (!raw) continue;
    const key = raw.toLowerCase().replace("_", "-");
    if (ALIASES[key]) return ALIASES[key];
    const base = key.split("-")[0];
    if (ALIASES[base]) return ALIASES[base];
  }
  return "en";
}

/** 把设置里的语言偏好解析成实际生效的语言 */
export function resolveLang(pref: string | undefined | null): Lang {
  if (!pref || pref === "system") return detectSystemLang();
  if (pref in DICTS) return pref as Lang;
  // 配置里写了个不认识的值（手改坏了 / 未来版本降级回来）→ 当作跟随系统
  return detectSystemLang();
}

type Vars = Record<string, string | number>;

let current: Lang = detectSystemLang();
const listeners = new Set<() => void>();

function apply(lang: Lang) {
  if (lang === current) return;
  current = lang;
  if (typeof document !== "undefined") document.documentElement.lang = lang;
  listeners.forEach((fn) => fn());
}

/** 由 App 在设置变化时调用；幂等 */
export function setLang(pref: string | undefined | null) {
  apply(resolveLang(pref));
}

export function currentLang(): Lang {
  return current;
}

function format(text: string, vars?: Vars): string {
  if (!vars) return text;
  let out = text;
  for (const [k, v] of Object.entries(vars)) {
    out = out.split(`{${k}}`).join(String(v));
  }
  return out;
}

/**
 * 取翻译。
 *
 * `t("{n} 个任务待处理", { n: 12 })` —— 占位符写成 `{n}`，
 * 这样词序不同的语言（日语把数量词放后面、西语放前面）能各自排。
 */
export function t(key: string, vars?: Vars): string {
  const dict = DICTS[current];
  const hit = dict?.[key];
  return format(hit ?? key, vars);
}

/**
 * 组件里用这个：语言切换时订阅者会重渲染。
 * 直接 `import { t }` 也能用，但那个不会触发重渲染。
 */
export function useT() {
  const [, bump] = useState(0);
  useEffect(() => {
    const fn = () => bump((n) => n + 1);
    listeners.add(fn);
    return () => {
      listeners.delete(fn);
    };
  }, []);
  return t;
}

/** 语言选择器里「跟随系统」那一项要显示成什么 */
export function langLabel(id: LangPref): string {
  if (id === "system") return t("系统语言");
  return LANGUAGES.find((l) => l.id === id)?.native ?? id;
}

export type { LangPref as LanguagePref };
