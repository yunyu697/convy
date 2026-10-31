import { memo, useEffect } from "react";
import { useT } from "../i18n";
import { api } from "../api";
import { DOMAIN_META, STATUS_META, formatSize, type Job } from "../types";

const GROUPS: { label: string; exts: string[] }[] = [
  { label: "视频", exts: ["mp4", "mkv", "webm", "mov", "avi", "m4v", "ts", "gif", "flv", "wmv"] },
  { label: "音频", exts: ["mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "wma", "aiff"] },
  { label: "图片", exts: ["jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "ico", "heic"] },
  { label: "电子书", exts: ["epub", "mobi", "azw3", "fb2", "htmlz"] },
  { label: "文档", exts: ["pdf", "docx", "odt", "rtf", "txt", "html", "md", "csv", "xlsx", "ods"] },
];

interface Props {
  job: Job;
  /** 实时进度（由父组件单独维护，运行时才变化） */
  live?: { progress: number; detail: string | null };
  targets: string[];
  onTargets: (ext: string) => Promise<string[]>;
  onRemove: (id: string) => void;
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onChangeTarget: (id: string, fmt: string) => void;
}

function JobRow({
  job,
  live,
  targets,
  onTargets,
  onRemove,
  onCancel,
  onRetry,
  onChangeTarget,
}: Props) {
  const t = useT();

  useEffect(() => {
    void onTargets(job.sourceExt);
  }, [job.sourceExt, onTargets]);

  const domain = job.domain ? DOMAIN_META[job.domain] : null;
  const status = STATUS_META[job.status];
  const locked = job.status === "running";
  const pct = Math.round(Math.min(1, Math.max(0, live?.progress ?? job.progress ?? 0)) * 100);
  const detail = live?.detail ?? job.detail;

  // 完成后的大小对比：输出相对源文件的百分比，变小为负（省空间）
  const sizeCompare = (() => {
    if (job.status !== "done" || job.outputSize == null || !job.size) return null;
    const delta = Math.round(((job.outputSize - job.size) / job.size) * 100);
    if (delta === 0) return { pct: 0, cls: "is-even", text: "±0%" };
    return {
      pct: delta,
      cls: delta < 0 ? "is-smaller" : "is-bigger",
      text: `${delta > 0 ? "+" : ""}${delta}%`,
    };
  })();

  const grouped = new Set(GROUPS.flatMap((g) => g.exts));
  const rest = targets.filter((t) => !grouped.has(t));

  return (
    <div className={`row row--${job.status}`}>
      <div className="row__badge" data-domain={job.domain ?? "none"}>
        {job.sourceExt.slice(0, 4).toUpperCase() || "FILE"}
      </div>

      <div className="row__info">
        <div className="row__name" title={job.source}>
          {job.fileName}
        </div>
        <div className="row__meta">
          {job.relDir && (
            <span className="row__dir" title={t("来自子文件夹：{dir}", { dir: job.relDir ?? "" })}>
              {job.relDir}
            </span>
          )}
          {job.status === "done" && job.outputSize != null ? (
            <>
              <span className="sizecmp__from">{formatSize(job.size)}</span>
              <span className="row__dot">→</span>
              <b className="sizecmp__to">{formatSize(job.outputSize)}</b>
              {sizeCompare && (
                <span className={`sizecmp ${sizeCompare.cls}`} title={t("输出相对源文件的大小变化")}>
                  {sizeCompare.text}
                </span>
              )}
            </>
          ) : (
            <>
              {formatSize(job.size)}
              {domain && <span className="row__dot">·</span>}
              {domain?.label}
              {detail && job.status === "running" && (
                <span className="row__speed">{detail}</span>
              )}
            </>
          )}
        </div>
      </div>

      <div className="row__arrow" aria-hidden="true">
        →
      </div>

      <select
        className="row__select"
        value={job.targetFormat}
        disabled={locked}
        onChange={(e) => onChangeTarget(job.id, e.target.value)}
      >
        {GROUPS.map((g) => {
          const opts = targets.filter((t) => g.exts.includes(t));
          if (!opts.length) return null;
          return (
            <optgroup key={g.label} label={t(g.label)}>
              {opts.map((t) => (
                <option key={t} value={t}>
                  {t.toUpperCase()}
                </option>
              ))}
            </optgroup>
          );
        })}
        {rest.length > 0 && (
          <optgroup label={t("其他")}>
            {rest.map((t) => (
              <option key={t} value={t}>
                {t.toUpperCase()}
              </option>
            ))}
          </optgroup>
        )}
      </select>

      <div className="row__status">
        {job.status === "running" ? (
          <div className="bar">
            <div className="bar__fill" style={{ width: `${pct}%` }} />
            <span className="bar__label">{pct}%</span>
          </div>
        ) : job.status === "done" ? (
          <div className="row__actions">
            <button className="linkbtn" onClick={() => job.outputPath && api.openPath(job.outputPath)}>
              {t("打开")}
            </button>
            <button
              className="linkbtn"
              onClick={() => job.outputPath && api.revealOutput(job.outputPath)}
            >
              {t("所在文件夹")}
            </button>
          </div>
        ) : job.status === "failed" ? (
          <div className="row__actions">
            <button className="linkbtn" onClick={() => onRetry(job.id)} title={t("用相同参数重新转换")}>
              {t("重试")}
            </button>
            <span className="row__fail" title={job.error ?? ""}>
              {t("失败")}
            </span>
          </div>
        ) : job.status === "canceled" ? (
          <div className="row__actions">
            <button className="linkbtn" onClick={() => onRetry(job.id)} title={t("重新入队")}>
              {t("重试")}
            </button>
            <span className="row__idle" style={{ color: status.color }}>
              {t(status.label)}
            </span>
          </div>
        ) : (
          <span className="row__idle" style={{ color: status.color }}>
            {t(status.label)}
          </span>
        )}
      </div>

      <button
        className="row__close"
        title={locked ? t("取消") : t("移除")}
        onClick={() => (locked ? onCancel(job.id) : onRemove(job.id))}
      >
        ×
      </button>
    </div>
  );
}

/**
 * 任务行数量可能上千，进度事件每秒刷新多次。
 * memo + 稳定的回调引用后，只有正在转换的那一行会重渲染。
 */
export default memo(JobRow);
