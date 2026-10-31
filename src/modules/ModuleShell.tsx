import { useEffect, useRef, useState, type ReactNode } from "react";
import DropZone from "../components/DropZone";
import JobRow from "../components/JobRow";
import PresetBar from "../components/PresetBar";
import { useT } from "../i18n";
import { api } from "../api";
import { DOMAIN_META, type Domain, type Job, type Preset } from "../types";

/**
 * 队列一次渲染多少行。
 *
 * 导入上限是 3000 个文件（见 import.rs），原来直接 `list.map(...)` 会一次性
 * 挂载 3000 个 JobRow —— 每个都是六列 grid + 动画，导入刚结束那一下会明显卡顿。
 * 改成增量渲染：首屏只画 80 行，滚到接近底部再追加。
 *
 * 为什么不用「绝对定位 + 固定行高」那种真虚拟化：这一行的高度不是常数
 * （带子目录胶囊、长文件名换行都会撑高），硬定高会把内容裁掉。
 * 增量渲染不改布局、不需要测量，代价只是滚到底时 DOM 仍会增长到全量 ——
 * 但用户真滚到第 3000 条时，那点开销已经无所谓了。
 */
const PAGE = 80;
/** 距底部多少像素开始追加下一批 */
const PRELOAD_PX = 400;

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

  const t = useT();
  const meta = DOMAIN_META[domain];
  const list = jobs.filter((j) => j.domain === domain);
  const pending = list.filter((j) => j.status === "pending").length;
  const done = list.filter((j) => j.status === "done").length;

  const [shown, setShown] = useState(PAGE);
  const listRef = useRef<HTMLDivElement>(null);

  // 换模块（或列表被清空后重新导入）时回到首屏数量，
  // 否则会沿用上一个模块滚出来的行数
  useEffect(() => {
    setShown(PAGE);
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [domain]);

  const onListScroll = () => {
    const el = listRef.current;
    if (!el) return;
    const remain = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (remain < PRELOAD_PX) {
      setShown((n) => (n >= list.length ? n : Math.min(n + PAGE, list.length)));
    }
  };

  const visible = list.length > shown ? list.slice(0, shown) : list;

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
              <h2 className="panelhead__title">{t("{name}参数", { name: t(meta.label) })}</h2>
              <p className="panelhead__desc">{meta.desc}</p>
            </div>
          </div>

          {engineMissing && (
            <div className="notice notice--warn">
              <span aria-hidden>⚠</span>
              <div>
                <strong>{engineMissing}</strong>
                <p>{t("可在「引擎管理」中一键下载并启用。")}</p>
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
            title={t("拖入{name}文件或文件夹", { name: t(meta.label) })}
            hint={t("支持批量与子目录")}
          />

          <div className="queue">
            <div className="queue__head">
              <div className="queue__counts">
                <span className="queue__count">
                  {t("共")} <b>{list.length}</b> {t("项")}
                </span>
                {pending > 0 && (
                  <span className="queue__count queue__count--pending">
                    {t("待处理")} <b>{pending}</b>
                  </span>
                )}
                {done > 0 && (
                  <span className="queue__count queue__count--done">
                    {t("已完成")} <b>{done}</b>
                  </span>
                )}
              </div>
              <div className="queue__tools">
                <button
                  className="btn btn--ghost"
                  onClick={onClearFinished}
                  disabled={done === 0}
                >
                  {t("清理已完成")}
                </button>
                <button
                  className="btn btn--primary"
                  onClick={onStart}
                  disabled={pending === 0 || busy}
                >
                  {busy ? t("转换中…") : `${t("开始转换")}${pending ? ` (${pending})` : ""}`}
                </button>
              </div>
            </div>

            <div className="queue__list" ref={listRef} onScroll={onListScroll}>
              {list.length === 0 ? (
                <div className="queue__empty">
                  <span className="queue__emptyicon" aria-hidden>
                    {meta.icon}
                  </span>
                  <p>{t("还没有{name}文件", { name: t(meta.label) })}</p>
                  <span>{t("拖入文件后会出现在这里")}</span>
                </div>
              ) : (
                <>
                  {visible.map((j) => (
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
                  ))}
                  {shown < list.length && (
                    <button
                      className="queue__more"
                      onClick={() => setShown((n) => Math.min(n + PAGE, list.length))}
                    >
                      {t("已显示 {shown} / {total} 项 · 继续滚动或点击加载更多", { shown, total: list.length })}
                    </button>
                  )}
                </>
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
  const t = useT();
  if (!path) return null;
  return (
    <div className="outputactions">
      <button className="btn btn--ghost" onClick={() => api.openPath(path)}>
        {t("打开")}
      </button>
      <button className="btn btn--ghost" onClick={() => api.revealOutput(path)}>
        {t("所在文件夹")}
      </button>
    </div>
  );
}
