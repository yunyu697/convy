import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, events } from "./api";
import { Sidebar, type ViewId } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import EngineModule from "./modules/EngineModule";
import HistoryModule from "./modules/HistoryModule";
import ModuleShell from "./modules/ModuleShell";
import SettingsModule from "./modules/SettingsModule";
import {
  AudioPanel,
  DocumentPanel,
  EbookPanel,
  ImagePanel,
  VideoPanel,
} from "./modules/panels";
import {
  ACCENTS,
  DEFAULT_SETTINGS,
  DOMAIN_META,
  type Domain,
  type EngineReport,
  type Job,
  type Preset,
  type Settings,
} from "./types";

interface Toast {
  id: number;
  kind: "ok" | "err" | "info";
  text: string;
}

const DOMAIN_VIEWS: Domain[] = ["video", "audio", "image", "ebook", "document"];

export default function App() {
  const [view, setView] = useState<ViewId>("video");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  /**
   * 实时进度单独存一张表，不塞进 jobs 数组。
   * 以前每个进度事件都重建整个 jobs 数组，几百个任务时每一帧都要重渲染所有行；
   * 拆开之后 + JobRow 的 memo，只有正在跑的那一行会更新。
   */
  const [live, setLive] = useState<Record<string, { progress: number; detail: string | null }>>({});
  const [report, setReport] = useState<EngineReport | null>(null);
  const [targets, setTargets] = useState<Record<string, string[]>>({});
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [activePreset, setActivePreset] = useState<Record<string, string | null>>({});
  const [sidebarCollapsed, setSidebarCollapsed] = useState(
    () => localStorage.getItem("convy.sidebarCollapsed") === "1",
  );

  const toggleSidebar = () =>
    setSidebarCollapsed((v) => {
      localStorage.setItem("convy.sidebarCollapsed", v ? "0" : "1");
      return !v;
    });

  const toastSeq = useRef(0);
  const targetsRef = useRef(targets);
  targetsRef.current = targets;

  const toast = useCallback((kind: Toast["kind"], text: string) => {
    const id = ++toastSeq.current;
    setToasts((prev) => [...prev, { id, kind, text }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, kind === "err" ? 6000 : 3200);
  }, []);

  // ---------------------------------------------------------- 初始加载
  // 启动容错：若后端 IPC / 设置读取失败（打包后 WebView2 异常的典型症状），
  // 不再无限停在 spinner，而是回退到前端内置默认设置继续渲染界面。
  useEffect(() => {
    void api.uiReady().catch(() => {});
    let settled = false;
    // 后端 5 秒内无响应就先用默认设置渲染，后续设置保存会自动写入后端
    const watchdog = window.setTimeout(() => {
      if (!settled) {
        settled = true;
        setSettings((prev) => prev ?? DEFAULT_SETTINGS);
      }
    }, 5000);
    void api
      .getSettings()
      .then((s) => {
        settled = true;
        window.clearTimeout(watchdog);
        setSettings(s);
      })
      .catch((e) => {
        settled = true;
        window.clearTimeout(watchdog);
        setSettings(DEFAULT_SETTINGS);
        toast("err", `设置加载失败，已使用默认配置：${String(e)}`);
      });
    void api
      .listPresets()
      .then(setPresets)
      .catch(() => {});
    void api
      .engineReport()
      .then(setReport)
      .catch(() => {});
    return () => window.clearTimeout(watchdog);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---------------------------------------------------------- 主题应用
  useEffect(() => {
    if (!settings) return;
    const root = document.documentElement;
    root.dataset.theme = settings.global.theme;
    const accent = ACCENTS.find((a) => a.id === settings.global.accent) ?? ACCENTS[0];
    root.style.setProperty("--accent", accent.color);
    root.style.setProperty(
      "--accent-soft",
      `color-mix(in srgb, ${accent.color} 14%, transparent)`,
    );
    root.style.setProperty(
      "--accent-line",
      `color-mix(in srgb, ${accent.color} 34%, transparent)`,
    );
  }, [settings?.global.theme, settings?.global.accent]);

  // ---------------------------------------------------------- 设置持久化
  const saveTimer = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!settings) return;
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      void api.saveSettings(settings).catch((e) => toast("err", String(e)));
    }, 400);
    return () => window.clearTimeout(saveTimer.current);
  }, [settings, toast]);

  // 面板里的「目标格式」变化 → 立即同步到该域所有待处理任务
  // （否则只有之后新加的文件才会用新目标，看起来像“设置不生效”）
  const jobsRef = useRef(jobs);
  jobsRef.current = jobs;
  useEffect(() => {
    if (!settings) return;
    (["video", "audio", "image", "ebook", "document"] as Domain[]).forEach((d) => {
      const want = (settings[d] as { defaultTarget?: string }).defaultTarget ?? "";
      if (!want) return;
      const stale = jobsRef.current.filter(
        (j) => j.status === "pending" && j.domain === d && j.targetFormat !== want,
      );
      if (!stale.length) return;
      setJobs((prev) =>
        prev.map((j) =>
          j.status === "pending" && j.domain === d && j.targetFormat !== want
            ? { ...j, targetFormat: want }
            : j,
        ),
      );
      stale.forEach((j) => void api.setTarget(j.id, want).catch(() => {}));
    });
  }, [settings]);

  // ---------------------------------------------------------- 后端事件
  useEffect(() => {
    let disposed = false;
    const uns: Array<() => void> = [];
    (async () => {
      const u1 = await events.jobUpdate((job) => {
        setJobs((prev) => {
          const i = prev.findIndex((j) => j.id === job.id);
          if (i === -1) return [...prev, job];
          const next = prev.slice();
          next[i] = job;
          return next;
        });
        if (job.status === "failed" && job.error) toast("err", job.error);
        // 注意：这里不能 setBusy(false)。单个任务完成不代表整个队列结束，
        // 提前解锁「开始转换」会让用户再点一次、新开一个信号量，实际并发翻倍。
      });
      const u2 = await events.jobProgress(({ id, progress, detail }) => {
        setLive((prev) => {
          const cur = prev[id];
          const nextProgress = progress ?? cur?.progress ?? 0;
          const nextDetail = detail ?? cur?.detail ?? null;
          // 进度和说明都没变就不产生新对象，避免无意义的重渲染
          if (cur && cur.progress === nextProgress && cur.detail === nextDetail) {
            return prev;
          }
          return { ...prev, [id]: { progress: nextProgress, detail: nextDetail } };
        });
      });
      const u3 = await events.queueDone(() => setBusy(false));
      const u4 = await events.enginesChanged(() => {
        void api.engineReport().then(setReport).catch(() => {});
      });
      if (disposed) [u1, u2, u3, u4].forEach((f) => f());
      else uns.push(u1, u2, u3, u4);
    })();
    return () => {
      disposed = true;
      uns.forEach((f) => f());
    };
  }, [toast]);

  // ---------------------------------------------------------- 系统拖拽
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      const un = await getCurrentWebview().onDragDropEvent((ev) => {
        const p = ev.payload;
        if (p.type === "enter" || p.type === "over") setDragging(true);
        else if (p.type === "leave") setDragging(false);
        else if (p.type === "drop") {
          setDragging(false);
          void addPaths(p.paths);
        }
      });
      if (cancelled) un();
      else dispose = un;
    })();
    return () => {
      cancelled = true;
      dispose?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---------------------------------------------------------- 动作
  const loadTargets = useCallback(async (ext: string) => {
    if (targetsRef.current[ext]) return targetsRef.current[ext];
    try {
      const t = await api.listTargets(ext);
      setTargets((prev) => ({ ...prev, [ext]: t }));
      return t;
    } catch {
      return [];
    }
  }, []);

  async function addPaths(paths: string[]) {
    if (!paths.length) return;
    try {
      const res = await api.addFiles(paths);
      const added = res.jobs;
      if (!added.length) {
        if (res.dirs > 0) {
          toast(
            "err",
            res.unsupported > 0
              ? `文件夹里没有可转换的文件（跳过 ${res.unsupported} 个不支持的格式）`
              : "文件夹里没有可转换的文件",
          );
        } else {
          toast("err", "这些文件类型暂不支持");
        }
        return;
      }
      setJobs((prev) => {
        const seen = new Set(prev.map((j) => j.id));
        return [...prev, ...added.filter((j) => !seen.has(j.id))];
      });
      added.forEach((j) => void loadTargets(j.sourceExt));
      // 新加入的文件自动切到它所属的模块，方便立刻调参
      const first = added[0].domain;
      if (first && DOMAIN_VIEWS.includes(first)) setView(first);

      if (res.dirs > 0) {
        const skipped = res.unsupported > 0 ? `，跳过 ${res.unsupported} 个不支持的文件` : "";
        toast("ok", `已从 ${res.dirs} 个文件夹添加 ${added.length} 个文件${skipped}`);
      }
      if (res.truncated) {
        toast("info", `文件数量超出上限，本次只导入了前 ${added.length} 个`);
      }
    } catch (err) {
      toast("err", String(err));
    }
  }

  async function pickFiles() {
    try {
      const picked = await openDialog({ multiple: true, title: "选择要转换的文件" });
      if (!picked) return;
      await addPaths(Array.isArray(picked) ? picked : [picked]);
    } catch (err) {
      toast("err", String(err));
    }
  }

  /** 选一个文件夹，递归导入其中所有受支持的文件 */
  async function pickFolder() {
    try {
      const dir = await openDialog({
        directory: true,
        multiple: false,
        title: "选择要批量转换的文件夹",
      });
      if (typeof dir !== "string") return;
      await addPaths([dir]);
    } catch (err) {
      toast("err", String(err));
    }
  }

  // 用 useCallback 稳定引用，否则每次渲染都是新函数，JobRow 的 memo 会失效
  const changeTarget = useCallback((id: string, fmt: string) => {
    setJobs((prev) =>
      prev.map((j) =>
        j.id === id ? { ...j, targetFormat: fmt, status: "pending", progress: 0 } : j,
      ),
    );
    void api.setTarget(id, fmt);
  }, []);

  const removeJob = useCallback((id: string) => {
    setJobs((prev) => prev.filter((j) => j.id !== id));
    setLive((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
    void api.removeJob(id);
  }, []);

  const cancelJob = useCallback((id: string) => {
    void api.cancelJob(id);
  }, []);

  /** 失败/取消的任务重新入队（保留源文件与目标格式） */
  const retryJob = useCallback((id: string) => {
    // 先本地置回 pending，界面立刻有反馈；后端事件随后覆盖
    setJobs((prev) =>
      prev.map((j) =>
        j.id === id
          ? { ...j, status: "pending", progress: 0, error: null, detail: null, outputPath: null, outputSize: null }
          : j,
      ),
    );
    setLive((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
    void api.retryJob(id).catch((e) => toast("err", String(e)));
  }, [toast]);

  const clearFinished = useCallback(() => {
    void api.clearFinished();
    setJobs((prev) => prev.filter((j) => j.status === "pending" || j.status === "running"));
    setLive((prev) => {
      // 只保留仍在队列里的任务的进度
      const keep = new Set(
        jobsRef.current
          .filter((j) => j.status === "pending" || j.status === "running")
          .map((j) => j.id),
      );
      const next: Record<string, { progress: number; detail: string | null }> = {};
      for (const [id, v] of Object.entries(prev)) if (keep.has(id)) next[id] = v;
      return next;
    });
  }, []);

  const start = async (domain?: Domain) => {
    setBusy(true);
    try {
      const n = await api.startQueue(domain);
      if (n === 0) {
        setBusy(false);
        toast("info", "没有待处理的任务");
      }
    } catch (err) {
      toast("err", String(err));
      setBusy(false);
    }
  };

  // ---------------------------------------------------------- 预设
  const applyPreset = (p: Preset) => {
    if (!settings) return;
    const domain = p.domain;
    const patch = p.settings as Record<string, unknown>;
    // 内置预设显式定义了分辨率 / 码率等参数，必须退出「保持源参数」模式，
    // 否则这些参数会被源文件的值覆盖，预设等于没生效。
    // 用户自己保存的预设若已包含 keepSource，则尊重其原值。
    const exitKeepSource =
      (domain === "video" || domain === "audio") && !("keepSource" in patch)
        ? { keepSource: false }
        : {};
    setSettings((prev) =>
      prev
        ? {
            ...prev,
            [domain]: { ...prev[domain], ...patch, ...exitKeepSource },
          }
        : prev,
    );
    setActivePreset((prev) => ({ ...prev, [domain]: p.name }));
    toast("ok", `已应用预设「${p.name}」`);
  };

  const savePreset = async (name: string) => {
    if (!settings) return;
    const domain = (DOMAIN_VIEWS.includes(view as Domain) ? view : "video") as Domain;
    const slice = settings[domain] as unknown as Record<string, unknown>;
    try {
      await api.savePreset(name, domain, slice);
      setPresets(await api.listPresets());
      setActivePreset((prev) => ({ ...prev, [domain]: name }));
      toast("ok", `预设「${name}」已保存`);
    } catch (e) {
      toast("err", String(e));
    }
  };

  const deletePreset = async (name: string) => {
    try {
      await api.deletePreset(name);
      setPresets(await api.listPresets());
      toast("info", `预设「${name}」已删除`);
    } catch (e) {
      toast("err", String(e));
    }
  };

  // ---------------------------------------------------------- 派生
  const pendingTotal = useMemo(
    () => jobs.filter((j) => j.status === "pending").length,
    [jobs],
  );
  /** 队列里还有任务在跑：此时不允许再点「开始」，否则会新开一个队列、并发翻倍 */
  const runningCount = useMemo(
    () => jobs.filter((j) => j.status === "running").length,
    [jobs],
  );

  const patchSettings = useCallback((updater: (s: Settings) => Settings) => {
    setSettings((prev) => (prev ? updater(prev) : prev));
  }, []);

  if (!settings) {
    return (
      <div className="boot">
        <div className="boot__spinner" aria-hidden />
        <p>正在启动 Convy…</p>
      </div>
    );
  }

  const isDomain = DOMAIN_VIEWS.includes(view as Domain);
  const currentDomain = (isDomain ? view : "video") as Domain;
  const meta = DOMAIN_META[currentDomain];

  const titleMap: Record<ViewId, string> = {
    video: "视频转换",
    audio: "音频转换",
    image: "图片转换",
    ebook: "电子书转换",
    document: "文档转换",
    history: "转换历史",
    engines: "引擎管理",
    settings: "全局设置",
  };
  const subtitleMap: Record<ViewId, string> = {
    video: meta.desc,
    audio: meta.desc,
    image: meta.desc,
    ebook: meta.desc,
    document: meta.desc,
    history: "最近的转换记录：可以再转一次、打开产物或查看失败原因",
    engines: "管理第三方解码与转换引擎：下载、更新、镜像与版本回退",
    settings: "外观、队列与引擎更新策略",
  };

  const engineMissing = (() => {
    if (!report) return undefined;
    switch (currentDomain) {
      case "video":
      case "audio":
        return report.ffmpeg && report.ffprobe
          ? undefined
          : "缺少 FFmpeg 引擎，视频与音频转换不可用";
      case "image":
        return report.image ? undefined : "缺少图片引擎（ImageMagick / libvips）";
      case "ebook":
        return report.ebook ? undefined : "缺少 Calibre 引擎，电子书转换不可用";
      case "document":
        return report.document || report.pandoc
          ? undefined
          : "缺少文档引擎（LibreOffice / Pandoc）";
    }
  })();

  return (
    <div className={`app${dragging ? " app--drag" : ""}${sidebarCollapsed ? " app--sidebar-collapsed" : ""}`}>
      <Sidebar
        active={view}
        onSelect={setView}
        report={report}
        pendingCount={pendingTotal}
        collapsed={sidebarCollapsed}
        onToggle={toggleSidebar}
      />

      <main className="main">
        <TopBar
          title={titleMap[view]}
          subtitle={subtitleMap[view]}
          theme={settings.global.theme}
          accent={settings.global.accent}
          onTheme={(id) =>
            patchSettings((s) => ({ ...s, global: { ...s.global, theme: id } }))
          }
          onAccent={(id) =>
            patchSettings((s) => ({ ...s, global: { ...s.global, accent: id } }))
          }
        />

        <div className="main__body" key={view}>
          {isDomain && (
            <ModuleShell
              domain={currentDomain}
              engineMissing={engineMissing}
              settingsPanel={
                currentDomain === "video" ? (
                  <VideoPanel
                    value={settings.video}
                    onChange={(v) => patchSettings((s) => ({ ...s, video: v }))}
                  />
                ) : currentDomain === "audio" ? (
                  <AudioPanel
                    value={settings.audio}
                    onChange={(v) => patchSettings((s) => ({ ...s, audio: v }))}
                  />
                ) : currentDomain === "image" ? (
                  <ImagePanel
                    value={settings.image}
                    onChange={(v) => patchSettings((s) => ({ ...s, image: v }))}
                  />
                ) : currentDomain === "ebook" ? (
                  <EbookPanel
                    value={settings.ebook}
                    onChange={(v) => patchSettings((s) => ({ ...s, ebook: v }))}
                  />
                ) : (
                  <DocumentPanel
                    value={settings.document}
                    onChange={(v) => patchSettings((s) => ({ ...s, document: v }))}
                  />
                )
              }
              presets={presets}
              activePreset={activePreset[currentDomain] ?? null}
              onApplyPreset={applyPreset}
              onSavePreset={savePreset}
              onDeletePreset={deletePreset}
              jobs={jobs}
              liveProgress={live}
              targetsCache={targets}
              onTargets={loadTargets}
              onPick={() => void pickFiles()}
              onPickFolder={() => void pickFolder()}
              onChangeTarget={changeTarget}
              onRemove={removeJob}
              onCancel={cancelJob}
              onRetry={retryJob}
              onStart={() => void start(currentDomain)}
              onClearFinished={clearFinished}
              busy={busy || runningCount > 0}
            />
          )}

          {view === "engines" && (
            <EngineModule
              onToast={toast}
              autoCheck={settings.global.autoCheckUpdate}
            />
          )}

          {view === "history" && (
            <HistoryModule onToast={toast} onAddPaths={(paths) => void addPaths(paths)} />
          )}

          {view === "settings" && (
            <SettingsModule
              value={settings.global}
              onChange={(v) => patchSettings((s) => ({ ...s, global: v }))}
              onToast={toast}
            />
          )}
        </div>
      </main>

      <div className="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast--${t.kind}`}>
            <span className="toast__icon" aria-hidden>
              {t.kind === "ok" ? "✓" : t.kind === "err" ? "!" : "i"}
            </span>
            <span className="toast__text">{t.text}</span>
            <button
              className="toast__close"
              onClick={() => setToasts((prev) => prev.filter((x) => x.id !== t.id))}
            >
              ×
            </button>
          </div>
        ))}
      </div>

      {dragging && (
        <div className="dropveil">
          <div className="dropveil__inner">松开以添加文件</div>
        </div>
      )}
    </div>
  );
}
