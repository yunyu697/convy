import type { ReactNode } from "react";
import DropZone from "../components/DropZone";
import JobRow from "../components/JobRow";
import PresetBar from "../components/PresetBar";
import { api } from "../api";
import { DOMAIN_META, type Domain, type Job, type Preset } from "../types";
export interface ModuleShellProps {
  domain: Domain;
  /** 该模块的精细化设置面板 */
  settingsPanel: ReactNode;
  /** 该模块缺引擎时的提示 */
  engineMissing?: string;

  presets: Preset[];
  activePreset: string | null;
  onApplyPreset: (p: Preset) => void;
  onSavePreset: (name: string) => void;
  onDeletePreset: (name: string) => void;

  jobs: Job[];
  /** 实时进度（按任务 id），与 jobs 分开避免整表重渲染 */
  liveProgress: Record<string, { progress: number; detail: string | null }>;
  targetsCache: Record<string, string[]>;
  onTargets: (ext: string) => Promise<string[]>;
  onPick: () => void;
  /** 选择文件夹（递归批量导入） */
  onPickFolder?: () => void;
  onChangeTarget: (id: string, fmt: string) => void;
  onRemove: (id: string) => void;
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onStart: () => void;
  onClearFinished: () => void;
  busy: boolean;
}

export default function ModuleShell(props: ModuleShellProps) {
  const {
    domain,
    settingsPanel,
    engineMissing,
    presets,
    activePreset,
    onApplyPreset,
    onSavePreset,
    onDeletePreset,
    jobs,
    liveProgress,
    targetsCache,
    onTargets,
    onPick,
    onPickFolder,
    onChangeTarget,
    onRemove,
    onCancel,
    onRetry,
    onStart,
    onClearFinished,
    busy,
  } = props;

  const meta = DOMAIN_META[domain];
  const list = jobs.filter((j) => j.domain === domain);
  const pending = list.filter((j) => j.status === "pending").length;
  const done = list.filter((j) => j.status === "done").length;

  return (
    <div className="module">
      <PresetBar
        domain={domain}
        presets={presets}
        activeName={activePreset}
        onApply={onApplyPreset}
        onSave={onSavePreset}
        onDelete={onDeletePreset}
      />

      <div className="module__grid">
        {/* 左：精细化设置 */}
        <aside className="module__settings">
          <div className="panelhead">
            <span className="panelhead__icon" aria-hidden>
              {meta.icon}
            </span>
            <div>
              <h2 className="panelhead__title">{meta.label}参数</h2>
              <p className="panelhead__desc">{meta.desc}</p>
            </div>
          </div>

          {engineMissing && (
            <div className="notice notice--warn">
              <span aria-hidden>⚠</span>
              <div>
                <strong>{engineMissing}</strong>
                <p>可在「引擎管理」中一键下载并启用。</p>
              </div>
            </div>
          )}

          <div className="module__settingsbody">{settingsPanel}</div>
        </aside>

        {/* 右：文件与队列 */}
        <section className="module__work">
          <DropZone
            onPick={onPick}
            onPickFolder={onPickFolder}
            title={`拖入${meta.label}文件或文件夹`}
            hint="支持批量与子目录"
          />

          <div className="queue">
            <div className="queue__head">
              <div className="queue__counts">
                <span className="queue__count">
                  共 <b>{list.length}</b> 项
                </span>
                {pending > 0 && (
                  <span className="queue__count queue__count--pending">
                    待处理 <b>{pending}</b>
                  </span>
                )}
                {done > 0 && (
                  <span className="queue__count queue__count--done">
                    已完成 <b>{done}</b>
                  </span>
                )}
              </div>
              <div className="queue__tools">
                <button
                  className="btn btn--ghost"
                  onClick={onClearFinished}
                  disabled={done === 0}
                >
                  清理已完成
                </button>
                <button
                  className="btn btn--primary"
                  onClick={onStart}
                  disabled={pending === 0 || busy}
                >
                  {busy ? "转换中…" : `开始转换${pending ? ` (${pending})` : ""}`}
                </button>
              </div>
            </div>

            <div className="queue__list">
              {list.length === 0 ? (
                <div className="queue__empty">
                  <span className="queue__emptyicon" aria-hidden>
                    {meta.icon}
                  </span>
                  <p>还没有{meta.label}文件</p>
                  <span>拖入文件后会出现在这里</span>
                </div>
              ) : (
                list.map((j) => (
                  <JobRow
                    key={j.id}
                    job={j}
                    live={liveProgress[j.id]}
                    targets={targetsCache[j.sourceExt] ?? []}
                    onTargets={onTargets}
                    onRemove={onRemove}
                    onCancel={onCancel}
                    onRetry={onRetry}
                    onChangeTarget={onChangeTarget}
                  />
                ))
              )}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

/** 模块顶部的「打开输出目录」等通用按钮组 */
export function OutputActions({ path }: { path?: string | null }) {
  if (!path) return null;
  return (
    <div className="outputactions">
      <button className="btn btn--ghost" onClick={() => api.openPath(path)}>
        打开
      </button>
      <button className="btn btn--ghost" onClick={() => api.revealOutput(path)}>
        所在文件夹
      </button>
    </div>
  );
}
