import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { t, useT } from "../i18n";
import { DOMAIN_META, STATUS_META, formatSize, type HistoryEntry } from "../types";

/** 相对时间：刚刚 / 5 分钟前 / 3 小时前 / 昨天 / 具体日期 */
function relTime(ms: number): string {
  if (!ms) return "—";
  const diff = Date.now() - ms;
  if (diff < 60_000) return t("刚刚");
  if (diff < 3_600_000) return t("{n} 分钟前", { n: Math.floor(diff / 60_000) });
  if (diff < 86_400_000) return t("{n} 小时前", { n: Math.floor(diff / 3_600_000) });
  if (diff < 172_800_000) return t("昨天");
  const d = new Date(ms);
  return t("{month} 月 {day} 日", { month: d.getMonth() + 1, day: d.getDate() });
}

function duration(ms: number): string {
  if (!ms) return "—";
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

export default function HistoryModule({
  onToast,
  onAddPaths,
}: {
  onToast: (kind: "ok" | "err" | "info", text: string) => void;
  onAddPaths: (paths: string[]) => void;
}) {
  const t = useT();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<"all" | "done" | "failed">("all");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setEntries(await api.listHistory(300));
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setLoading(false);
    }
  }, [onToast]);

  useEffect(() => {
    void load();
  }, [load]);

  const shown = useMemo(() => {
    if (filter === "all") return entries;
    if (filter === "done") return entries.filter((e) => e.status === "done");
    return entries.filter((e) => e.status === "failed" || e.status === "canceled");
  }, [entries, filter]);

  const stats = useMemo(() => {
    const done = entries.filter((e) => e.status === "done").length;
    const failed = entries.filter((e) => e.status === "failed").length;
    const saved = entries.reduce((sum, e) => {
      if (e.status !== "done" || e.outputSize == null) return sum;
      return sum + Math.max(0, e.inputSize - e.outputSize);
    }, 0);
    return { done, failed, saved };
  }, [entries]);

  /** 再转一次：把源文件重新加回队列（沿用当前模块参数，目标格式随后可改） */
  const again = (e: HistoryEntry) => {
    onAddPaths([e.source]);
    onToast("info", t("已把「{name}」重新加入队列", { name: e.fileName }));
  };

  const clearAll = async () => {
    try {
      await api.clearHistory();
      setEntries([]);
      onToast("info", t("历史已清空"));
    } catch (err) {
      onToast("err", String(err));
    }
  };

  return (
    <div className="historypage">
      <div className="engines__bar">
        <div className="engines__info">
          <h2 className="engines__title">{t("最近的转换")}</h2>
          <p className="engines__desc">
            {t("共 {n} 条记录，成功 {ok} / 失败 {fail}", { n: entries.length, ok: stats.done, fail: stats.failed })}
            {stats.saved > 0 && t("，累计节省 {size}", { size: formatSize(stats.saved) })}
          </p>
        </div>
        <div className="historypage__tools">
          <div className="segmented segmented--compact">
            {(
              [
                { v: "all", label: t("全部") },
                { v: "done", label: t("成功") },
                { v: "failed", label: t("失败") },
              ] as const
            ).map((o) => (
              <button
                key={o.v}
                type="button"
                className={filter === o.v ? "is-active" : ""}
                onClick={() => setFilter(o.v)}
              >
                {o.label}
              </button>
            ))}
          </div>
          <button className="btn btn--ghost" onClick={() => void load()} disabled={loading}>
            {loading ? t("读取中…") : t("刷新")}
          </button>
          <button
            className="btn btn--ghost"
            onClick={() => void clearAll()}
            disabled={!entries.length}
          >
            {t("清空历史")}
          </button>
        </div>
      </div>

      <div className="historylist">
        {shown.length === 0 ? (
          <div className="queue__empty">
            <span className="queue__emptyicon" aria-hidden>
              ⟲
            </span>
            <p>{loading ? t("正在读取历史…") : t("还没有转换记录")}</p>
            <span>{t("完成任意一次转换后，这里会留下记录")}</span>
          </div>
        ) : (
          shown.map((e) => {
            const status = STATUS_META[e.status];
            const domain = e.domain ? DOMAIN_META[e.domain] : null;
            const delta =
              e.status === "done" && e.outputSize != null && e.inputSize
                ? Math.round(((e.outputSize - e.inputSize) / e.inputSize) * 100)
                : null;
            return (
              <div className="hrow" key={`${e.id}-${e.finishedAt}`}>
                <span className={`hrow__dot hrow__dot--${e.status}`} aria-hidden />
                <div className="hrow__main">
                  <div className="hrow__name" title={e.source}>
                    {e.fileName}
                  </div>
                  <div className="hrow__meta">
                    <span>{relTime(e.finishedAt)}</span>
                    <span className="row__dot">·</span>
                    <span>
                      {domain ? t(domain.label) : t("文件")} → {e.targetFormat.toUpperCase()}
                    </span>
                    <span className="row__dot">·</span>
                    <span>{duration(e.durationMs)}</span>
                    {e.outputSize != null && (
                      <>
                        <span className="row__dot">·</span>
                        <span>
                          {formatSize(e.inputSize)} → {formatSize(e.outputSize)}
                          {delta != null && delta !== 0 && (
                            <em className={delta < 0 ? "is-smaller" : "is-bigger"}>
                              {delta > 0 ? "+" : ""}
                              {delta}%
                            </em>
                          )}
                        </span>
                      </>
                    )}
                  </div>
                  {e.error && (
                    <div className="hrow__err" title={e.error}>
                      {e.error}
                    </div>
                  )}
                </div>
                <span className="hrow__status" style={{ color: status.color }}>
                  {status.label}
                </span>
                <div className="hrow__actions">
                  {e.outputPath && e.status === "done" && (
                    <>
                      <button className="linkbtn" onClick={() => api.openPath(e.outputPath!)}>
                        {t("打开")}
                      </button>
                      <button
                        className="linkbtn"
                        onClick={() => api.revealOutput(e.outputPath!)}
                      >
                        {t("所在文件夹")}
                      </button>
                    </>
                  )}
                  <button
                    className="linkbtn"
                    title={t("把源文件重新加入队列，使用当前模块参数")}
                    onClick={() => again(e)}
                  >
                    {t("再转一次")}
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
