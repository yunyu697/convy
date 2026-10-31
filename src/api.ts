import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddResult,
  Domain,
  EngineCheck,
  EngineProgress,
  EngineReport,
  EngineStatus,
  EngineUpdateResult,
  HistoryEntry,
  Job,
  MirrorTest,
  Preset,
  SelfCheckItem,
  Settings,
} from "./types";

export const api = {
  // ---- 队列
  uiReady: () => invoke<void>("ui_ready"),
  engineReport: () => invoke<EngineReport>("engine_report"),
  listTargets: (ext: string) => invoke<string[]>("list_targets", { ext }),
  addFiles: (paths: string[]) => invoke<AddResult>("add_files", { paths }),
  setTarget: (id: string, format: string) => invoke<void>("set_target", { id, format }),
  retryJob: (id: string) => invoke<Job>("retry_job", { id }),
  removeJob: (id: string) => invoke<void>("remove_job", { id }),
  clearFinished: () => invoke<void>("clear_finished"),
  listHistory: (limit?: number) =>
    invoke<HistoryEntry[]>("list_history", { limit: limit ?? null }),
  clearHistory: () => invoke<void>("clear_history"),
  setOverwrite: (v: boolean) => invoke<void>("set_overwrite", { v }),
  cancelJob: (id: string) => invoke<void>("cancel_job", { id }),
  startQueue: (domain?: Domain) => invoke<number>("start_queue", { domain: domain ?? null }),
  revealOutput: (path: string) => invoke<void>("reveal_output", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  /** 界面崩溃时把现场写进 convy.log */
  logFrontendError: (message: string) =>
    invoke<void>("log_frontend_error", { message }),
  openLogFile: () => invoke<void>("open_log_file"),

  // ---- 设置与预设
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  listPresets: () => invoke<Preset[]>("list_presets"),
  savePreset: (name: string, domain: Domain, settings: unknown) =>
    invoke<void>("save_preset", { name, domain, settingsJson: settings }),
  deletePreset: (name: string) => invoke<void>("delete_preset", { name }),

  // ---- 引擎
  engineList: () => invoke<EngineStatus[]>("engine_list"),
  engineCheckUpdate: (id: string) => invoke<EngineCheck>("engine_check_update", { id }),
  engineCheckAll: () => invoke<EngineCheck[]>("engine_check_all"),
  engineSelfcheck: () => invoke<SelfCheckItem[]>("engine_selfcheck"),
  engineUpdate: (id: string, mirrorUrl: string | null) =>
    invoke<EngineUpdateResult>("engine_update", { id, mirrorUrl }),
  engineTestMirror: (url: string) => invoke<MirrorTest>("engine_test_mirror", { url }),
  engineAddMirror: (id: string, name: string, url: string) =>
    invoke<void>("engine_add_mirror", { id, name, url }),
  engineRemoveMirror: (id: string, url: string) =>
    invoke<void>("engine_remove_mirror", { id, url }),
  engineRollback: (id: string, version: string) =>
    invoke<void>("engine_rollback", { id, version }),
};

export const events = {
  jobUpdate: (cb: (job: Job) => void): Promise<UnlistenFn> =>
    listen<Job>("job:update", (e) => cb(e.payload)),
  jobProgress: (
    cb: (payload: { id: string; progress?: number; detail?: string }) => void,
  ): Promise<UnlistenFn> =>
    listen<{ id: string; progress?: number; detail?: string }>("job:progress", (e) =>
      cb(e.payload),
    ),
  engineProgress: (cb: (payload: EngineProgress) => void): Promise<UnlistenFn> =>
    listen<EngineProgress>("engine:progress", (e) => cb(e.payload)),
  enginesChanged: (cb: () => void): Promise<UnlistenFn> =>
    listen("engines:changed", () => cb()),
  queueDone: (cb: () => void): Promise<UnlistenFn> =>
    listen("queue:done", () => cb()),
};
