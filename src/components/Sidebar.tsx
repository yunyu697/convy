import { useT } from "../i18n";
import type { Domain, EngineReport } from "../types";

export type ViewId = Domain | "engines" | "history" | "settings";

export interface NavItem {
  id: ViewId;
  label: string;
  icon: string;
  group: string;
}

export const NAV_ITEMS: NavItem[] = [
  { id: "video", label: "视频转换", icon: "▶", group: "转换" },
  { id: "audio", label: "音频转换", icon: "♪", group: "转换" },
  { id: "image", label: "图片转换", icon: "◈", group: "转换" },
  { id: "ebook", label: "电子书转换", icon: "❦", group: "转换" },
  { id: "document", label: "文档转换", icon: "▤", group: "转换" },
  { id: "history", label: "转换历史", icon: "⟲", group: "系统" },
  { id: "engines", label: "引擎管理", icon: "⬡", group: "系统" },
  { id: "settings", label: "全局设置", icon: "⚙", group: "系统" },
];

// 导航文案是模块级常量，不能在里面直接调 t()（那样只在导入时求值一次，
// 切换语言不会更新）。渲染时再翻译。

export function Sidebar({
  active,
  onSelect,
  report,
  pendingCount,
  collapsed,
  onToggle,
}: {
  active: ViewId;
  onSelect: (v: ViewId) => void;
  report: EngineReport | null;
  pendingCount: number;
  /** 是否折叠为纯图标窄栏 */
  collapsed: boolean;
  onToggle: () => void;
}) {
  const t = useT();
  const groups = Array.from(new Set(NAV_ITEMS.map((n) => n.group)));

  /** 某个域的引擎是否齐备 */
  const domainReady = (d: Domain): boolean => {
    if (!report) return false;
    switch (d) {
      case "video":
      case "audio":
        return report.ffmpeg && report.ffprobe;
      case "image":
        return !!report.image;
      case "ebook":
        return report.ebook;
      case "document":
        return report.document || report.pandoc;
    }
  };

  return (
    <aside className={`sidebar${collapsed ? " sidebar--collapsed" : ""}`}>
      <div className="sidebar__brand">
        <div className="sidebar__logo" aria-hidden>
          ⬢
        </div>
        {!collapsed && (
          <div className="sidebar__brandtext">
            <strong>Convy</strong>
            <span>{t("万象转换")}</span>
          </div>
        )}
        <button
          type="button"
          className="sidebar__collapse"
          onClick={onToggle}
          title={collapsed ? t("展开菜单") : t("折叠菜单")}
          aria-label={collapsed ? t("展开菜单") : t("折叠菜单")}
        >
          {collapsed ? "»" : "«"}
        </button>
      </div>

      <nav className="sidebar__nav">
        {groups.map((g) => (
          <div key={g} className="sidebar__group">
            {!collapsed && <div className="sidebar__grouptitle">{t(g)}</div>}
            {NAV_ITEMS.filter((n) => n.group === g).map((n) => {
              const isDomain = !["engines", "settings", "history"].includes(n.id);
              const ready = isDomain ? domainReady(n.id as Domain) : true;
              return (
                <button
                  key={n.id}
                  type="button"
                  className={`navitem${active === n.id ? " is-active" : ""}`}
                  onClick={() => onSelect(n.id)}
                  title={t(n.label)}
                >
                  <span className="navitem__icon" aria-hidden>
                    {n.icon}
                  </span>
                  {!collapsed && <span className="navitem__label">{t(n.label)}</span>}
                  {isDomain && !ready && (
                    <span className="navitem__warn" title={t("缺少该模块所需的引擎")}>
                      !
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className="sidebar__foot">
        <div className="hwchip" title={report ? t("当前 H.264 编码链路：{enc}", { enc: report.h264Encoder }) : t("检测中")}>
          <span className={`hwchip__dot${report?.hardwareAccel ? " is-hw" : ""}`} />
          {!collapsed && (
            <span className="hwchip__text">{report ? report.h264Encoder : t("检测中…")}</span>
          )}
        </div>
        {!collapsed && pendingCount > 0 && (
          <div className="sidebar__pending">
            {t("{n} 个任务待处理", { n: pendingCount })}
          </div>
        )}
      </div>
    </aside>
  );
}
