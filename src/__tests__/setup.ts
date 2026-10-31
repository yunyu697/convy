/**
 * 测试的公共前置。
 *
 * 为什么要把语言钉死
 * ------------------
 * 应用默认「跟随系统语言」，而 jsdom 的 `navigator.language` 固定是 `en-US`。
 * 不钉的话，组件测试里断言的界面文案会跟着宿主环境漂 —— 今天在英文环境过、
 * 换台机器就红，而且断言的内容和组件源码里写的那句中文对不上号。
 *
 * 钉成简体中文（源语言）之后：字典查不到就回退成 key 本身，而 key 就是源码里的
 * 中文原文，于是断言能一眼对上组件里写的那行字。
 */
import { beforeEach } from "vitest";
import { setLang } from "../i18n";

beforeEach(() => {
  setLang("zh-Hans");
});
