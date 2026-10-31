import { useCallback, useEffect, useMemo, useState } from "react";
import { api, events } from "../api";
import { DOMAIN_META, type Domain, type EngineStatus } from "../types";

/** 与 App.tsx 里 Toast 的 kind 保持一致（那边是私有 interface，不对外导出） */
type ToastKind = "ok" | "info" | "err";

/**
 * 首次启动向导。
 *
 * 解决的是一个很具体的问题：新用户装完打开，看到的是一个空界面 + 几个「!」角标，
 * 既不知道要先装引擎，也不知道装了引擎能干什么。README 里写清楚了，但没人读。
 *
 * 三步：选用途 → 装对应引擎 → 立刻转一个文件试试。
 * 只有 FFmpeg 是托管型（能一键下载），其余引擎只能引导去官网 —— 如实说，
 * 不要给一个点不动的按钮。
 */

const STEPS = ["选择用途", "准备引擎", "开始转换"] as const;

/** 用途 → 需要的引擎。视频和音频共用 FFmpeg。 */
const DOMAIN_ENGINES: Record<Domain, string[]> = {
  video: ["ffmpeg"],
  audio: ["ffmpeg"],
  image: ["imagemagick"],
  ebook: ["calibre"],
  document: ["pandoc", "libreoffice"],
};

const DOMAINS: Domain[] = ["video", "audio", "image", "ebook", "document"];

/**
 * 引擎安装分四段（download → verify → extract → done），只有 download 有真实百分比。
 * 后两段若继续显示「下载中 87%」会让人以为卡住了，所以按阶段换文案。
 */
function busyLabel(p: { phase: string; pct: number } | null): string {
  if (!p) return "准备中…";
  if (p.phase === "verify") return "校验中…";
  if (p.phase === "extract") return "解压中…";
  return `下载中 ${Math.round(p.pct * 100)}%`;
}

interface Props {
  onClose: () => void;
  /** 用户想在第三步直接选文件 */
  onPickFile: () => void;
  onToast: (kind: ToastKind, text: string) => void;
}

export default function Onboarding({ onClose, onPickFile, onToast }: Props) {
  const [step, setStep] = useState(0);
  const [picked, setPicked] = useState<Domain[]>([]);
  const [engines, setEngines] = useState<EngineStatus[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [progress, setProgress] = useState<{
    id: string;
    phase: string;
    pct: number;
    msg: string;
  } | null>(null);

  const refreshEngines = useCallback(async () => {
    try {
      setEngines(await api.engineList());
    } catch {
      // 引擎探测还没完成时这里会失败，不打断引导
    }
  }, []);

  useEffect(() => {
    void refreshEngines();
    const un = events.engineProgress((p) => {
      if (p.phase === "done") {
        setProgress(null);
        return;
      }
      setProgress({ id: p.id, phase: p.phase, pct: p.progress, msg: p.message });
    });
    const un2 = events.enginesChanged(() => void refreshEngines());
    return () => {
      void un.then((f) => f());
      void un2.then((f) => f());
    };
  }, [refreshEngines]);

  /** 去重后的引擎需求列表 */
  const needed = useMemo(() => {
    const ids = new Set<string>();
    for (const d of picked) for (const id of DOMAIN_ENGINES[d]) ids.add(id);
    return [...ids];
  }, [picked]);

  const engineOf = (id: string) => engines.find((e) => e.id === id);

  const readyCount = needed.filter((id) => engineOf(id)?.installed).length;

  const toggleDomain = (d: Domain) => {
    setPicked((prev) => (prev.includes(d) ? prev.filter((x) => x !== d) : [...prev, d]));
  };

  const install = async (e: EngineStatus) => {
    setBusyId(e.id);
    try {
      await api.engineUpdate(e.id, null);
      onToast("ok", `${e.label} 已安装并启用`);
      await refreshEngines();
    } catch (err) {
      onToast("err", String(err));
    } finally {
      setBusyId(null);
      setProgress(null);
    }
  };

  const finish = () => {
    onClose();
  };

  return (
    <div className="onb">
      <div className="onb__card" role="dialog" aria-modal="true" aria-label="首次使用引导">
        <header className="onb__head">
          <div className="onb__brand">
            <span className="onb__logo" aria-hidden>
              ⇄
            </span>
            <div>
              <h1 className="onb__title">欢迎使用 Convy</h1>
              <p className="onb__sub">三步就能转第一个文件，大约一分钟</p>
            </div>
          </div>
          <ol className="onb__steps">
            {STEPS.map((s, i) => (
              <li
                key={s}
                className={`onb__step${i === step ? " is-active" : ""}${
                  i < step ? " is-done" : ""
                }`}
              >
                <span className="onb__stepnum">{i < step ? "✓" : i + 1}</span>
                {s}
              </li>
            ))}
          </ol>
        </header>

        <div className="onb__body">
          {step === 0 && (
            <>
              <p className="onb__lead">你主要想转哪一类文件？可以多选。</p>
              <div className="onb__domains">
                {DOMAINS.map((d) => {
                  const on = picked.includes(d);
                  const meta = DOMAIN_META[d];
                  return (
                    <button
                      key={d}
                      className={`onb__domain${on ? " is-on" : ""}`}
                      onClick={() => toggleDomain(d)}
                      aria-pressed={on}
                    >
                      <span className="onb__domainicon" aria-hidden>
                        {meta.icon}
                      </span>
                      <strong>{meta.label}</strong>
                      <span className="onb__domaindesc">{meta.desc}</span>
                    </button>
                  );
                })}
              </div>
              <p className="onb__note">
                只选你用得上的，没选的引擎之后也能在「引擎管理」里随时补装。
              </p>
            </>
          )}

          {step === 1 && (
            <>
              {needed.length === 0 ? (
                <p className="onb__lead">
                  你没有选择用途，可以跳过这一步 —— 需要时去「引擎管理」装引擎即可。
                </p>
              ) : (
                <>
                  <p className="onb__lead">
                    这些引擎用来完成你刚选的任务。已经装好的会显示 ✓。
                  </p>
                  <div className="onb__engines">
                    {needed.map((id) => {
                      const e = engineOf(id);
                      const pr = progress?.id === id ? progress : null;
                      return (
                        <div className="onb__engine" key={id}>
                          <div className="onb__enginemain">
                            <strong>{e?.label ?? id}</strong>
                            <span className="onb__enginedesc">
                              {e?.sizeHint ?? e?.description ?? "正在检测…"}
                            </span>
                          </div>
                          <div className="onb__engineact">
                            {!e ? (
                              <span className="onb__tag">检测中…</span>
                            ) : e.installed ? (
                              <span className="onb__tag onb__tag--ok">
                                ✓ 已就绪{e.activeVersion ? ` · ${e.activeVersion}` : ""}
                              </span>
                            ) : e.managed ? (
                              <button
                                className="btn btn--primary"
                                onClick={() => void install(e)}
                                disabled={busyId !== null}
                              >
                                {busyId === id ? busyLabel(pr) : "一键下载"}
                              </button>
                            ) : e.homepage ? (
                              <button
                                className="btn btn--ghost"
                                onClick={() => void api.openUrl(e.homepage!)}
                              >
                                去官网安装
                              </button>
                            ) : (
                              <span className="onb__tag">需手动安装</span>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                  <p className="onb__note">
                    {readyCount > 0
                      ? `已就绪 ${readyCount} / ${needed.length}。没装完也能先用其他格式，随时回来补。`
                      : "引擎体积较大（FFmpeg 约 115 MB），可以先跳过，之后在「引擎管理」里装。"}
                  </p>
                </>
              )}
            </>
          )}

          {step === 2 && (
            <>
              <p className="onb__lead">
                把文件拖进主界面任意位置就能开始，也可以直接选一个试试。
              </p>
              <div className="onb__final">
                <button className="btn btn--primary" onClick={onPickFile}>
                  选择一个文件
                </button>
                <button className="btn btn--ghost" onClick={finish}>
                  先看看界面
                </button>
              </div>
              <ul className="onb__tips">
                <li>文件夹也能直接拖进来，会递归扫描里面的受支持格式</li>
                <li>转换参数有「快速调整」条，改编码或质量不用翻完整面板</li>
                <li>失败的任务行上有「重试」，不用删了重新加</li>
              </ul>
            </>
          )}
        </div>

        <footer className="onb__foot">
          <button className="linkbtn" onClick={finish}>
            跳过引导
          </button>
          <div className="onb__nav">
            {step > 0 && (
              <button className="btn btn--ghost" onClick={() => setStep((s) => s - 1)}>
                上一步
              </button>
            )}
            {step < 2 ? (
              <button className="btn btn--primary" onClick={() => setStep((s) => s + 1)}>
                下一步
              </button>
            ) : (
              <button className="btn btn--primary" onClick={finish}>
                开始使用
              </button>
            )}
          </div>
        </footer>
      </div>
    </div>
  );
}
