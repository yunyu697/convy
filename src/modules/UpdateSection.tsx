import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { api, events } from "../api";
import { Field, Group, Select } from "../components/ui";
import { t, useT } from "../i18n";
import { formatSize, type AppUpdateInfo, type GlobalSettings, type UpdateDownload } from "../types";

/**
 * 设置页的「更新」与「更新日志」两块。
 *
 * 单独拆出来是因为它有一整套自己的状态机（检查中 / 有更新 / 下载中 / 已下载待安装 / 安装中），
 * 塞回 SettingsModule 会让那个文件变成第二个 panels.tsx。
 *
 * 三个按钮的行为差异是这一块的核心：
 * - 更新：下载 + 立刻安装（Windows 静默装完自动退出）
 * - 仅下载：只下安装包，落盘后把路径给用户，并留一个「安装」按钮
 * - 打开下载页面：什么都不下，直接开浏览器到 Release 页 —— 给不想让应用碰安装包的人
 */

interface Props {
  info: AppUpdateInfo | null;
  /** force = 无视检查频率，用户手动点的 */
  onCheck: (force: boolean) => Promise<AppUpdateInfo | null>;
  settings: GlobalSettings;
  onPatch: (patch: Partial<GlobalSettings>) => void;
  onToast: (kind: "ok" | "err" | "info", text: string) => void;
}

function when(ms: number): string {
  if (!ms) return "";
  const diff = Date.now() - ms;
  if (diff < 60_000) return t("刚刚");
  if (diff < 3_600_000) return t("{n} 分钟前", { n: Math.floor(diff / 60_000) });
  if (diff < 86_400_000) return t("{n} 小时前", { n: Math.floor(diff / 3_600_000) });
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

/**
 * 安装结论 → 提示文案。
 *
 * 后端只回 `mode`，措辞留在前端：只有前端知道界面现在是哪门语言。
 */
function installToast(outcome: { mode: "silent" | "opened" }): string {
  return outcome.mode === "silent"
    ? t("安装程序已在后台启动，应用即将退出")
    : t("已用系统默认程序打开安装包");
}

/** 把 Release 正文按 Markdown 的几种常见标记渲染成节点（不注入 HTML） */
function Notes({ text }: { text: string }) {
  const lines = text.split("\n");
  const blocks: ReactNode[] = [];
  let list: string[] = [];

  const flush = () => {
    if (!list.length) return;
    blocks.push(
      <ul className="relnotes__list" key={`ul${blocks.length}`}>
        {list.map((li, i) => (
          <li key={i}>{li}</li>
        ))}
      </ul>,
    );
    list = [];
  };

  for (const raw of lines) {
    const trimmed = raw.trim();
    if (!trimmed) {
      flush();
      continue;
    }
    if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
      list.push(trimmed.slice(2));
      continue;
    }
    flush();
    const heading = trimmed.match(/^#{1,6}\s+(.*)$/);
    if (heading) {
      blocks.push(
        <h4 className="relnotes__h" key={`h${blocks.length}`}>
          {heading[1]}
        </h4>,
      );
      continue;
    }
    // 行内 `code` 与 **加粗**：只处理这两种，够用了
    const parts = trimmed.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean);
    blocks.push(
      <p className="relnotes__p" key={`p${blocks.length}`}>
        {parts.map((p, i) =>
          p.startsWith("`") && p.endsWith("`") ? (
            <code key={i}>{p.slice(1, -1)}</code>
          ) : p.startsWith("**") && p.endsWith("**") ? (
            <strong key={i}>{p.slice(2, -2)}</strong>
          ) : (
            <span key={i}>{p}</span>
          ),
        )}
      </p>,
    );
  }
  flush();

  if (!blocks.length) return <p className="relnotes__empty">（这个版本没有写更新说明）</p>;
  return <div className="relnotes">{blocks}</div>;
}

export default function UpdateSection({ info, onCheck, settings, onPatch, onToast }: Props) {
  const t = useT();
  const [busy, setBusy] = useState<"check" | "download" | "install" | null>(null);
  const [pct, setPct] = useState<number | null>(null);
  const [msg, setMsg] = useState("");
  const [saved, setSaved] = useState<UpdateDownload | null>(null);
  const [mirrors, setMirrors] = useState<{ id: string; name: string }[]>([]);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // 更新源列表只在打开设置页时取一次
  useEffect(() => {
    void api
      .appUpdateMirrors()
      .then((m) => {
        if (mounted.current) setMirrors(m.map((x) => ({ id: x.id, name: x.name })));
      })
      .catch(() => {});
  }, []);

  // 下载进度
  useEffect(() => {
    const un = events.appUpdateProgress((p) => {
      if (!mounted.current) return;
      setPct(p.progress);
      setMsg(p.message);
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const runCheck = useCallback(
    async (force: boolean) => {
      setBusy("check");
      try {
        await onCheck(force);
      } finally {
        if (mounted.current) setBusy(null);
      }
    },
    [onCheck],
  );

  /**
   * 进设置页时补一次检查。
   *
   * 启动时那次检查可能因为「还没到频率」被跳过，那样更新日志会是空的 ——
   * 用户点进设置就是想看有没有新版本，这里补一次是值得的。
   * 刚查过（10 分钟内）就不再打扰服务器。
   */
  useEffect(() => {
    if (info && !info.skippedByInterval && Date.now() - info.checkedAt < 600_000) return;
    void runCheck(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const download = useCallback(async () => {
    setBusy("download");
    setPct(0);
    setMsg("");
    try {
      const r = await api.appUpdateDownload();
      if (mounted.current) setSaved(r);
      return r;
    } catch (e) {
      onToast("err", String(e));
      return null;
    } finally {
      if (mounted.current) {
        setBusy(null);
        setPct(null);
      }
    }
  }, [onToast]);

  /** 更新：下载完直接装 */
  const updateNow = useCallback(async () => {
    const r = saved ?? (await download());
    if (!r) return;
    setBusy("install");
    try {
      const outcome = await api.appUpdateInstall(r.path);
      onToast("ok", installToast(outcome));
    } catch (e) {
      onToast("err", String(e));
    } finally {
      if (mounted.current) setBusy(null);
    }
  }, [download, onToast, saved]);

  /** 仅下载：只落盘，把路径给用户，留一个安装按钮 */
  const downloadOnly = useCallback(async () => {
    const r = await download();
    if (r) onToast("ok", t("安装包已保存到 {path}", { path: r.path }));
  }, [download, onToast, t]);

  const installSaved = useCallback(async () => {
    if (!saved) return;
    setBusy("install");
    try {
      const outcome = await api.appUpdateInstall(saved.path);
      onToast("ok", installToast(outcome));
    } catch (e) {
      onToast("err", String(e));
    } finally {
      if (mounted.current) setBusy(null);
    }
  }, [onToast, saved]);

  const latest = info?.latestVersion;
  const hasUpdate = Boolean(info?.updateAvailable);
  const skipped = Boolean(latest && settings.skippedVersion === latest);
  const downloading = busy === "download";

  return (
    <>
      <Group title={t("更新")} desc={t("检查新版本，下载并安装")}>
        <div className="upd">
          <div className="upd__ver">
            <span className="upd__verlabel">{t("当前版本")}</span>
            <span className="upd__vernum">{info?.currentVersion ?? "—"}</span>
            {hasUpdate && latest && (
              <span className="upd__new">{t("新版本 {v}", { v: latest })}</span>
            )}
          </div>

          <div className="upd__status">
            {busy === "check" ? (
              <span className="upd__muted">{t("正在检查…")}</span>
            ) : info?.error ? (
              <span className="upd__err">{t("检查失败：{err}", { err: info.error })}</span>
            ) : info?.checkedAt ? (
              <span className="upd__muted">
                {hasUpdate ? t("发现新版本 {v}", { v: latest ?? "" }) : t("已是最新版本")}
                {" · "}
                {t("上次检查 {when}", { when: when(info.checkedAt) })}
              </span>
            ) : (
              <span className="upd__muted">{t("还没有检查过")}</span>
            )}
          </div>

          {hasUpdate && latest && (
            <div className="upd__actions">
              {saved ? (
                <button
                  className="btn btn--primary"
                  onClick={() => void installSaved()}
                  disabled={busy !== null}
                >
                  {busy === "install" ? t("正在安装…") : t("安装 {v}", { v: saved.version })}
                </button>
              ) : (
                <button
                  className="btn btn--primary"
                  onClick={() => void updateNow()}
                  disabled={busy !== null}
                >
                  {downloading
                    ? pct != null
                      ? t("下载中 {pct}%", { pct: Math.round(pct * 100) })
                      : t("准备中…")
                    : busy === "install"
                      ? t("正在安装…")
                      : t("更新到 {v}", { v: latest })}
                </button>
              )}

              <button
                className="btn btn--ghost"
                onClick={() => void downloadOnly()}
                disabled={busy !== null}
              >
                {t("仅下载")}
              </button>

              {info?.htmlUrl && (
                <button
                  className="btn btn--ghost"
                  onClick={() => void api.openUrl(info.htmlUrl!)}
                  disabled={busy !== null}
                >
                  {t("打开下载页面")}
                </button>
              )}

              <button
                className="btn btn--ghost"
                onClick={() => onPatch({ skippedVersion: skipped ? "" : latest })}
                disabled={busy !== null}
              >
                {skipped ? t("不再跳过此版本") : t("跳过此版本")}
              </button>
            </div>
          )}

          {downloading && pct != null && (
            <div
              className="upd__bar"
              role="progressbar"
              aria-valuenow={Math.round(pct * 100)}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <i style={{ width: `${Math.round(pct * 100)}%` }} />
              <span>{msg}</span>
            </div>
          )}

          {saved && !downloading && (
            <p className="upd__saved">
              {t("已下载 {name}（{size}），保存在 {path}", {
                name: saved.asset,
                size: formatSize(saved.size),
                path: saved.path,
              })}
            </p>
          )}

          {skipped && (
            <p className="upd__muted">
              {t("你跳过了 {v}，启动时不会再提示，但可以随时在这里安装。", {
                v: latest ?? "",
              })}
            </p>
          )}
        </div>

        <Field label={t("检查频率")} hint={t("多久检查一次新版本")}>
          <Select
            value={settings.updateCheckFreq}
            onChange={(v) => onPatch({ updateCheckFreq: v })}
            options={[
              { value: "startup", label: t("每次启动") },
              { value: "daily", label: t("每天") },
              { value: "weekly", label: t("每周") },
              { value: "monthly", label: t("每月") },
              { value: "never", label: t("从不") },
            ]}
          />
        </Field>

        <Field label={t("更新源")} hint={t("GitHub 直连不通时可换镜像加速")}>
          <Select
            value={settings.updateMirror}
            onChange={(v) => onPatch({ updateMirror: v })}
            options={[
              { value: "auto", label: t("自动择优") },
              ...mirrors.map((m) => ({ value: m.id, label: m.name })),
            ]}
          />
        </Field>

        <div className="upd__foot">
          <button
            className="btn btn--ghost"
            onClick={() => void runCheck(true)}
            disabled={busy !== null}
          >
            {busy === "check" ? t("检查中…") : t("立即检查")}
          </button>
        </div>
      </Group>

      <Group title={t("更新日志")} desc={t("最近一次发布说明")}>
        {info?.publishedAt && latest && (
          <p className="relnotes__meta">
            {latest} · {info.publishedAt.slice(0, 10)}
          </p>
        )}
        {info?.notes ? (
          <Notes text={info.notes} />
        ) : (
          <p className="relnotes__empty">
            {busy === "check" ? t("正在获取…") : t("暂无更新日志")}
          </p>
        )}
        {info?.htmlUrl && (
          <div className="upd__foot">
            <button className="btn btn--ghost" onClick={() => void api.openUrl(info.htmlUrl!)}>
              {t("查看完整发布页")}
            </button>
          </div>
        )}
      </Group>
    </>
  );
}
