import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import {
  DICTS,
  LANGUAGES,
  detectSystemLang,
  resolveLang,
  setLang,
  t,
  useT,
  type Lang,
} from "../i18n";

afterEach(cleanup);

/** 除简体中文（源语言，字典为空）外，其余四门都必须有完整译文 */
const TRANSLATED: Lang[] = ["en", "zh-Hant", "ja", "es"];

const placeholders = (s: string) => (s.match(/\{(\w+)\}/g) ?? []).sort();

/** jsdom 里 navigator.language 固定 en-US，这里按需覆盖 */
function mockLocale(language: string, languages: string[] = [language]) {
  vi.spyOn(navigator, "language", "get").mockReturnValue(language);
  vi.spyOn(navigator, "languages", "get").mockReturnValue(languages);
}

describe("系统语言识别", () => {
  it("繁简按地区分开：zh-TW / zh-HK / zh-MO 是繁体", () => {
    for (const tag of ["zh-TW", "zh-HK", "zh-MO", "zh-Hant"]) {
      mockLocale(tag);
      expect(detectSystemLang(), tag).toBe("zh-Hant");
    }
  });

  it("其余 zh-* 一律按简体处理", () => {
    for (const tag of ["zh", "zh-CN", "zh-SG", "zh-Hans"]) {
      mockLocale(tag);
      expect(detectSystemLang(), tag).toBe("zh-Hans");
    }
  });

  it("en / ja / es 及其地区变体都认得", () => {
    mockLocale("en-GB");
    expect(detectSystemLang()).toBe("en");
    mockLocale("ja-JP");
    expect(detectSystemLang()).toBe("ja");
    mockLocale("es-419");
    expect(detectSystemLang()).toBe("es");
  });

  it("系统语言不在支持范围内时落到英语，而不是留在中文", () => {
    mockLocale("fr-FR", ["fr-FR", "de-DE", "ko-KR"]);
    expect(detectSystemLang()).toBe("en");
  });

  it("首选语言不支持时继续看后面的候选", () => {
    mockLocale("de-DE", ["de-DE", "ja-JP"]);
    expect(detectSystemLang()).toBe("ja");
  });
});

describe("语言偏好解析", () => {
  it("指定了受支持的语言就用它", () => {
    expect(resolveLang("zh-Hant")).toBe("zh-Hant");
    expect(resolveLang("es")).toBe("es");
  });

  it("system / 空值都走系统语言", () => {
    mockLocale("ja-JP");
    expect(resolveLang("system")).toBe("ja");
    expect(resolveLang(undefined)).toBe("ja");
    expect(resolveLang(null)).toBe("ja");
  });

  it("配置里写了不认识的值时退回跟随系统", () => {
    mockLocale("es-ES");
    expect(resolveLang("klingon")).toBe("es");
  });
});

describe("取词", () => {
  it("字典里没有的 key 原样返回，不会变成空白或 key 名", () => {
    expect(t("这条文案还没进字典")).toBe("这条文案还没进字典");
  });

  it("占位符按变量替换，重复出现也全部替换", () => {
    expect(t("共 {n} 个文件", { n: 3 })).toBe("共 3 个文件");
    expect(t("{a}-{a}", { a: "x" })).toBe("x-x");
  });

  it("切换语言后取词跟着变，切回简体又变回中文原文", () => {
    setLang("en");
    expect(t("共")).toBe("Total");
    setLang("zh-Hans");
    expect(t("共")).toBe("共");
  });

  it("切换语言会同步 <html lang>", () => {
    setLang("ja");
    expect(document.documentElement.lang).toBe("ja");
    setLang("zh-Hans");
    expect(document.documentElement.lang).toBe("zh-Hans");
  });
});

describe("组件订阅", () => {
  it("useT 在语言切换时触发重渲染", () => {
    function Probe() {
      const tr = useT();
      return <span>{tr("共")}</span>;
    }
    render(<Probe />);
    expect(screen.getByText("共")).toBeTruthy();

    act(() => setLang("en"));
    expect(screen.getByText("Total")).toBeTruthy();
  });
});

describe("字典完整性", () => {
  it("简体中文是源语言，字典故意为空", () => {
    expect(Object.keys(DICTS["zh-Hans"])).toHaveLength(0);
  });

  it("四份字典的 key 集合完全一致", () => {
    const base = Object.keys(DICTS.en).sort();
    expect(base.length).toBeGreaterThan(400);
    for (const lang of TRANSLATED) {
      expect(Object.keys(DICTS[lang]).sort(), lang).toEqual(base);
    }
  });

  it("每条译文的占位符与原文严格对应", () => {
    for (const lang of TRANSLATED) {
      for (const [key, value] of Object.entries(DICTS[lang])) {
        expect(placeholders(value), `${lang}: ${key}`).toEqual(placeholders(key));
      }
    }
  });

  it("没有空译文", () => {
    for (const lang of TRANSLATED) {
      for (const [key, value] of Object.entries(DICTS[lang])) {
        expect(value.trim().length, `${lang}: ${key}`).toBeGreaterThan(0);
      }
    }
  });

  // 只对英文和西文做「不许照抄原文」的检查。
  // 繁中和日文不能这么查：「1–100，越高越清晰」全是不随简繁变化的字符，
  // 「垂直 / 水平」在日语里本来就是同形词 —— 译文与原文逐字相同是对的，
  // 强行要求不同只会逼人把对的翻成错的。英/西文里出现中文原文，则一定是漏翻。
  it("英文 / 西文没有把中文原文照抄当译文", () => {
    for (const lang of ["en", "es"] as Lang[]) {
      for (const [key, value] of Object.entries(DICTS[lang])) {
        expect(value, `${lang}: ${key}`).not.toBe(key);
      }
    }
  });
});

describe("语言列表", () => {
  it("六项，第一项是跟随系统，其余五项顺序固定", () => {
    expect(LANGUAGES.map((l) => l.id)).toEqual([
      "system",
      "zh-Hans",
      "zh-Hant",
      "en",
      "ja",
      "es",
    ]);
  });

  it("语言名用自己的文字写，不翻译", () => {
    const native = Object.fromEntries(LANGUAGES.map((l) => [l.id, l.native]));
    expect(native["zh-Hans"]).toBe("简体中文");
    expect(native["zh-Hant"]).toBe("繁體中文");
    expect(native.en).toBe("English");
    expect(native.ja).toBe("日本語");
    expect(native.es).toBe("Español");
  });
});
