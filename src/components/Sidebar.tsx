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
            <span>万象转换</span>
          </div>
        )}
        <button
          type="button"
          className="sidebar__collapse"
          onClick={onToggle}
          title={collapsed ? "展开菜单" : "折叠菜单"}
          aria-label={collapsed ? "展开菜单" : "折叠菜单"}
        >
          {collapsed ? "»" : "«"}
        </button>
      </div>

      <nav className="sidebar__nav">
        {groups.map((g) => (
          <div key={g} className="sidebar__group">
            {!collapsed && <div className="sidebar__grouptitle">{g}</div>}
            {NAV_ITEMS.filter((n) => n.group === g).map((n) => {
              const isDomain = !["engines", "settings", "history"].includes(n.id);
              const ready = isDomain ? domainReady(n.id as Domain) : true;
              return (
                <button
                  key={n.id}
                  type="button"
                  className={`navitem${active === n.id ? " is-active" : ""}`}
                  onClick={() => onSelect(n.id)}
                  title={n.label}
                >
                  <span className="navitem__icon" aria-hidden>
                    {n.icon}
                  </span>
                  {!collapsed && <span className="navitem__label">{n.label}</span>}
                  {isDomain && !ready && (
                    <span className="navitem__warn" title="缺少该模块所需的引擎">
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
        <div className="hwchip" title={report ? `当前 H.264 编码链路：${report.h264Encoder}` : "检测中"}>
          <span className={`hwchip__dot${report?.hardwareAccel ? " is-hw" : ""}`} />
          {!collapsed && (
            <span className="hwchip__text">{report ? report.h264Encoder : "检测中…"}</span>
          )}
        </div>
        {!collapsed && pendingCount > 0 && (
          <div className="sidebar__pending">
            {pendingCount} 个任务待处理
          </div>
        )}
      </div>
    </aside>
  );
}
