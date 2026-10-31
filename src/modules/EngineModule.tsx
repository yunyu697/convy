import { useCallback, useEffect, useState } from "react";
import { api, events } from "../api";
import type {
  EngineCheck,
  EngineProgress,
  EngineStatus,
  MirrorTest,
  SelfCheckItem,
} from "../types";

function statusBadge(e: EngineStatus, check?: EngineCheck) {
  // 有更新优先于「已安装」，否则永远看不到更新提示
  if (check?.updateAvailable) {
    return { cls: "is-new", text: check.latest ? `可更新至 ${check.latest}` : "有更新" };
  }
  if (e.installed && e.activeVersion) {
    return {
      cls: "is-ok",
      text: e.system ? "系统已安装" : "已托管",
    };
  }
  if (e.installed) return { cls: "is-ok", text: "已安装" };
  return { cls: "is-missing", text: "未安装" };
}

function EngineCard({
  engine,
  check,
  checking,
  selectedUrl,
  progress,
  busy,
  onSelect,
  onUpdate,
  onRollback,
  onCheck,
  onOpenHome,
  onMirrorsChanged,
  onToast,
}: {
  engine: EngineStatus;
  check?: EngineCheck;
  checking?: boolean;
  selectedUrl: string;
  progress: EngineProgress | null;
  busy: boolean;
  onSelect: (url: string) => void;
  onUpdate: () => void;
  onRollback: (v: string) => void;
  onCheck: () => void;
  onOpenHome: () => void;
  onMirrorsChanged: () => void;
  onToast: (kind: "ok" | "err" | "info", text: string) => void;
}) {
  const [rolling, setRolling] = useState(false);
  const [tests, setTests] = useState<Record<string, MirrorTest | "testing">>({});
  const [managing, setManaging] = useState(false);
  const [newName, setNewName] = useState("");
  const [newUrl, setNewUrl] = useState("");
  const badge = statusBadge(engine, check);
  const downloading = progress?.id === engine.id;
  const currentVersion = check?.current ?? engine.activeVersion ?? null;
  const latestVersion = check?.latest ?? null;

  const closeManage = () => {
    setManaging(false);
    setNewName("");
    setNewUrl("");
  };

  const testUrl = async (url: string) => {
    setTests((t) => ({ ...t, [url]: "testing" }));
    try {
      const r = await api.engineTestMirror(url);
      setTests((t) => ({ ...t, [url]: r }));
    } catch (e) {
      setTests((t) => ({
        ...t,
        [url]: { ok: false, latencyMs: null, message: String(e) },
      }));
    }
  };

  const addMirror = async () => {
    const url = newUrl.trim();
    if (!url) return;
    try {
      await api.engineAddMirror(engine.id, newName.trim(), url);
      onToast("ok", "镜像已添加");
      setNewName("");
      setNewUrl("");
      onMirrorsChanged();
    } catch (e) {
      onToast("err", String(e));
    }
  };

  const removeMirror = async (url: string) => {
    try {
      await api.engineRemoveMirror(engine.id, url);
      if (selectedUrl === url) onSelect("");
      onToast("info", "镜像已删除");
      onMirrorsChanged();
    } catch (e) {
      onToast("err", String(e));
    }
  };

  const sourceOptions = [
    { value: "", label: "官方原地址" },
    ...engine.mirrors.map((m) => ({ value: m.url, label: m.name })),
  ];
  const selectedTest = tests[selectedUrl];

  return (
    <article className={`enginecard${engine.installed ? " is-installed" : ""}`}>
      <header className="enginecard__head">
        <div className="enginecard__ident">
          <h3 className="enginecard__name">{engine.label}</h3>
          <span className={`enginecard__badge ${badge.cls}`}>{badge.text}</span>
        </div>
        <span className="enginecard__license">{engine.license}</span>
      </header>

      <p className="enginecard__desc">{engine.description}</p>

      {engine.sizeHint && <p className="enginecard__hint">ⓘ {engine.sizeHint}</p>}

      <div className="enginecard__meta">
        <div className="enginecard__kv">
          <span>当前版本</span>
          <b>{currentVersion ?? "—"}</b>
        </div>
        <div className="enginecard__kv">
          <span>远程最新</span>
          <b title={check?.source ?? ""}>{latestVersion ?? (check ? "未知" : "—")}</b>
        </div>
        <div className="enginecard__kv">
          <span>来源</span>
          <b>{engine.system ? "系统 PATH" : engine.managed ? "本程序托管" : "—"}</b>
        </div>
      </div>

      {check?.error && (
        <div className="enginecard__check is-bad" title={check.source ?? ""}>
          检查失败：{check.error}
        </div>
      )}
      {check && !check.error && latestVersion && !check.updateAvailable && (
        <div className="enginecard__check is-ok">
          已是最新（{new Date(check.checkedAt).toLocaleTimeString()} 检查）
        </div>
      )}
      {check && !check.error && !latestVersion && (
        <div className="enginecard__check">该地址没有返回可识别的版本号</div>
      )}

      {engine.binPath && (
        <div className="enginecard__path" title={engine.binPath}>
          {engine.binPath}
        </div>
      )}

      {downloading && (
        <div className="enginecard__progress">
          <div className="enginecard__bar">
            <div
              className="enginecard__barfill"
              style={{ width: `${Math.round((progress?.progress ?? 0) * 100)}%` }}
            />
          </div>
          <span className="enginecard__phase">{progress?.message ?? progress?.phase}</span>
        </div>
      )}

      <div className="enginecard__actions">
        {engine.managed ? (
          <button className="btn btn--primary" onClick={onUpdate} disabled={busy || downloading}>
            {engine.installed ? "更新到最新" : "一键下载"}
          </button>
        ) : (
          <button className="btn btn--primary" onClick={onUpdate} disabled={busy || downloading}>
            {engine.installed ? "获取新版本" : "下载安装包"}
          </button>
        )}

        <button className="btn btn--ghost" onClick={onCheck} disabled={busy || downloading || checking}>
          {checking ? "检查中…" : "检查更新"}
        </button>

        {engine.homepage && (
          <button className="btn btn--ghost" onClick={onOpenHome}>
            官网
          </button>
        )}

        {engine.availableVersions.length > 1 && (
          <div className="rollback">
            <button
              className="btn btn--ghost"
              onClick={() => setRolling(!rolling)}
              disabled={busy}
            >
              版本回退 ▾
            </button>
            {rolling && (
              <div className="rollback__menu">
                {engine.availableVersions.map((v) => (
                  <button
                    key={v}
                    className={v === engine.activeVersion ? "is-current" : ""}
                    onClick={() => {
                      onRollback(v);
                      setRolling(false);
                    }}
                  >
                    {v}
                    {v === engine.activeVersion && <em>当前</em>}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {/* 下载源：下拉选择 + 测速；管理镜像收进可展开面板 */}
      <div className="enginecard__sources">
        <div className="mirrorrow">
          <span className="mirrorrow__label">下载源</span>
          <select
            className="mirrorrow__select"
            value={selectedUrl}
            onChange={(e) => onSelect(e.target.value)}
            title={selectedUrl || "官方原地址"}
          >
            {sourceOptions.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="btn btn--ghost"
            disabled={selectedTest === "testing"}
            onClick={() => void testUrl(selectedUrl)}
          >
            {selectedTest === "testing" ? "测速中…" : "测速"}
          </button>
          <button type="button" className="btn btn--ghost" onClick={() => (managing ? closeManage() : setManaging(true))}>
            {managing ? "收起" : "管理镜像"}
          </button>
        </div>

        {selectedTest && selectedTest !== "testing" && (
          <div className={`mirrortest ${selectedTest.ok ? "is-ok" : "is-bad"}`}>
            {selectedTest.ok
              ? `可用 · ${selectedTest.message}`
              : `不可用 · ${selectedTest.message}`}
          </div>
        )}

        {managing && (
          <div className="mirrorpanel">
            <ul className="mirrorlist">
              {engine.mirrors.map((m) => {
                const t = tests[m.url];
                return (
                  <li key={m.url} className={m.url === selectedUrl ? "is-selected" : ""}>
                    <button
                      type="button"
                      className="mirrorlist__pick"
                      title={m.url}
                      onClick={() => onSelect(m.url)}
                    >
                      <span className="mirrorlist__name">{m.name}</span>
                      <span className="mirrorlist__url">{m.url}</span>
                    </button>
                    {t && t !== "testing" && (
                      <span className={`mirrorlist__latency ${t.ok ? "is-ok" : "is-bad"}`}>
                        {t.ok ? `${t.latencyMs} ms` : "不可用"}
                      </span>
                    )}
                    <button
                      type="button"
                      className="mirrorlist__test"
                      title="测速"
                      disabled={t === "testing"}
                      onClick={() => void testUrl(m.url)}
                    >
                      {t === "testing" ? "…" : "测速"}
                    </button>
                    {!m.builtin && (
                      <button
                        type="button"
                        className="mirrorlist__del"
                        title="删除此镜像"
                        onClick={() => void removeMirror(m.url)}
                      >
                        ×
                      </button>
                    )}
                  </li>
                );
              })}
              {engine.mirrors.length === 0 && (
                <li className="mirrorlist__empty">暂无自定义镜像，可在下方添加</li>
              )}
            </ul>

            <div className="mirroradd">
              <input
                value={newName}
                placeholder="名称（可留空）"
                onChange={(e) => setNewName(e.target.value)}
              />
              <input
                value={newUrl}
                placeholder="https://mirror.example.com/..."
                onChange={(e) => setNewUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void addMirror();
                  if (e.key === "Escape") closeManage();
                }}
              />
              <button type="button" className="btn btn--primary" onClick={() => void addMirror()}>
                添加
              </button>
              <button type="button" className="btn btn--ghost" onClick={closeManage}>
                取消
              </button>
            </div>
          </div>
        )}
      </div>

      {engine.url && (
        <div className="enginecard__rawurl" title={engine.url}>
          {engine.url}
        </div>
      )}
    </article>
  );
}

/** 同一次运行里只自动检查一遍：反复切页面不该反复打网络 */
let autoCheckedOnce = false;

export default function EngineModule({
  onToast,
  autoCheck = true,
}: {
  onToast: (kind: "ok" | "err" | "info", text: string) => void;
  /** 进入引擎页时自动跑一次版本检查（对应设置里的「自动检查更新」） */
  autoCheck?: boolean;
}) {
  const [list, setList] = useState<EngineStatus[]>([]);
  const [mirrorSel, setMirrorSel] = useState<Record<string, string>>({});
  const [progress, setProgress] = useState<EngineProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [checks, setChecks] = useState<Record<string, EngineCheck>>({});
  const [checking, setChecking] = useState<Record<string, boolean>>({});
  const [selfCheck, setSelfCheck] = useState<SelfCheckItem[] | null>(null);
  const [selfChecking, setSelfChecking] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const engines = await api.engineList();
      setList(engines);
      // 清理已不存在的镜像选择
      setMirrorSel((sel) => {
        const next: Record<string, string> = {};
        for (const e of engines) {
          const cur = sel[e.id] ?? "";
          const valid = cur === "" || e.mirrors.some((m) => m.url === cur);
          next[e.id] = valid ? cur : "";
        }
        return next;
      });
    } catch (e) {
      onToast("err", String(e));
    }
  }, [onToast]);

  useEffect(() => {
    void refresh();
    const un = events.engineProgress((p) => setProgress(p));
    const un2 = events.enginesChanged(() => void refresh());
    return () => {
      void un.then((f) => f());
      void un2.then((f) => f());
    };
  }, [refresh]);

  const doUpdate = async (e: EngineStatus) => {
    setBusy(true);
    setProgress(null);
    const url = mirrorSel[e.id] || null;
    try {
      const r = await api.engineUpdate(e.id, url);
      if (r.installed) {
        onToast("ok", `${e.label} ${r.version ?? ""} 已安装并启用`);
      } else if (r.savedPath) {
        onToast("info", `${e.label} 安装包已保存到 ${r.savedPath}`);
      } else if (r.homepage) {
        onToast("info", `请前往官网下载 ${e.label}`);
        await api.openUrl(r.homepage);
      }
      await refresh();
    } catch (err) {
      onToast("err", String(err));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const doCheck = async (e: EngineStatus) => {
    setChecking((c) => ({ ...c, [e.id]: true }));
    try {
      const r = await api.engineCheckUpdate(e.id);
      setChecks((c) => ({ ...c, [e.id]: r }));
      if (r.error) {
        onToast("err", `${e.label} 版本检查失败：${r.error}`);
      } else if (r.updateAvailable) {
        onToast("info", `${e.label} 有新版本 ${r.latest}（当前 ${r.current ?? "未知"}）`);
      } else if (r.latest) {
        onToast("ok", `${e.label} 已是最新（${r.latest}）`);
      } else {
        onToast("info", `${e.label} 的版本地址没有返回可识别的版本号`);
      }
    } catch (err) {
      onToast("err", String(err));
    } finally {
      setChecking((c) => ({ ...c, [e.id]: false }));
    }
  };

  /** 一次性检查全部引擎：请求并发发出，结果按 id 落表 */
  const doCheckAll = async () => {
    setBusy(true);
    try {
      const results = await api.engineCheckAll();
      setChecks((c) => {
        const next = { ...c };
        results.forEach((r) => {
          next[r.id] = r;
        });
        return next;
      });
      const failed = results.filter((r) => r.error);
      const updatable = results.filter((r) => r.updateAvailable);
      if (failed.length) {
        onToast("err", `${failed.length} 个引擎检查失败：${failed[0].error ?? ""}`);
      } else if (updatable.length) {
        onToast(
          "info",
          `有可用更新：${updatable.map((r) => `${r.id} → ${r.latest}`).join("，")}`,
        );
      } else {
        onToast("ok", "所有引擎均为最新版本");
      }
    } catch (err) {
      onToast("err", String(err));
    } finally {
      setBusy(false);
    }
  };

  // 进入引擎页时按设置自动检查一次（同一次运行只跑一遍）
  useEffect(() => {
    if (!autoCheck || autoCheckedOnce) return;
    autoCheckedOnce = true;
    void doCheckAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoCheck]);

  /** 自检：本地生成极小素材，真实跑一遍五类转换 */
  const doSelfCheck = async () => {
    setSelfChecking(true);
    setSelfCheck(null);
    try {
      const items = await api.engineSelfcheck();
      setSelfCheck(items);
      const failed = items.filter((i) => !i.ok && !i.skipped);
      const okCount = items.filter((i) => i.ok).length;
      if (failed.length) {
        onToast("err", `${failed.length} 个类别自检未通过：${failed[0].label}`);
      } else {
        onToast("ok", `自检通过 ${okCount} 项${items.length > okCount ? "，其余因缺引擎跳过" : ""}`);
      }
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setSelfChecking(false);
    }
  };

  const doRollback = async (e: EngineStatus, version: string) => {
    setBusy(true);
    try {
      await api.engineRollback(e.id, version);
      onToast("ok", `${e.label} 已回退到 ${version}`);
      await refresh();
    } catch (err) {
      onToast("err", String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="engines">
      <div className="engines__bar">
        <div className="engines__info">
          <h2 className="engines__title">第三方引擎</h2>
          <p className="engines__desc">
            转换能力由外部工具提供。托管型引擎可一键下载并自动启用；可选官方地址或加速镜像，
            支持测速与自己添加镜像。版本检查走各引擎的官方版本接口。
          </p>
        </div>
        <button
          className="btn btn--ghost"
          onClick={() => void doSelfCheck()}
          disabled={busy || selfChecking}
          title="本地生成测试素材，真实跑一遍五类转换"
        >
          {selfChecking ? "自检中…" : "引擎自检"}
        </button>
        <button
          className="btn btn--ghost"
          onClick={() => void doCheckAll()}
          disabled={busy || list.length === 0}
        >
          检查全部更新
        </button>
        <button className="btn btn--ghost" onClick={() => void refresh()} disabled={busy}>
          刷新状态
        </button>
      </div>

      {selfCheck && (
        <div className="selfcheck">
          <div className="selfcheck__head">
            <strong>自检结果</strong>
            <span>本地生成素材，走与正式转换完全相同的链路</span>
            <button className="linkbtn" onClick={() => setSelfCheck(null)}>
              收起
            </button>
          </div>
          <ul className="selfcheck__list">
            {selfCheck.map((i) => (
              <li
                key={i.domain}
                className={`selfcheck__item ${i.ok ? "is-ok" : i.skipped ? "is-skip" : "is-bad"}`}
              >
                <span className="selfcheck__mark" aria-hidden>
                  {i.ok ? "✓" : i.skipped ? "–" : "!"}
                </span>
                <div className="selfcheck__body">
                  <div className="selfcheck__label">{i.label}</div>
                  <div className="selfcheck__msg">{i.message}</div>
                  {i.detail && <pre className="selfcheck__detail">{i.detail}</pre>}
                </div>
                {i.ok && <span className="selfcheck__time">{i.elapsedMs} ms</span>}
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="engines__grid">        {list.map((e) => (
          <EngineCard
            key={e.id}
            engine={e}
            check={checks[e.id]}
            checking={checking[e.id]}
            selectedUrl={mirrorSel[e.id] ?? ""}
            progress={progress}
            busy={busy}
            onSelect={(url) => setMirrorSel((m) => ({ ...m, [e.id]: url }))}
            onUpdate={() => void doUpdate(e)}
            onCheck={() => void doCheck(e)}
            onRollback={(v) => void doRollback(e, v)}
            onOpenHome={() => e.homepage && void api.openUrl(e.homepage)}
            onMirrorsChanged={() => void refresh()}
            onToast={onToast}
          />
        ))}
        {list.length === 0 && <div className="engines__empty">正在检测引擎…</div>}
      </div>
    </div>
  );
}
