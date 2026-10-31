import { useState } from "react";
import { useT } from "../i18n";
import type { Domain, Preset } from "../types";

export default function PresetBar({
  domain,
  presets,
  onApply,
  onSave,
  onDelete,
  activeName,
}: {
  domain: Domain;
  presets: Preset[];
  onApply: (p: Preset) => void;
  onSave: (name: string) => void;
  onDelete: (name: string) => void;
  activeName: string | null;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");

  const list = presets.filter((p) => p.domain === domain);

  const submit = () => {
    const n = name.trim();
    if (!n) return;
    onSave(n);
    setName("");
    setNaming(false);
  };

  return (
    <div className={`presetbar${open ? " is-open" : ""}`}>
      <button
        type="button"
        className="presetbar__toggle"
        onClick={() => setOpen(!open)}
        title={t("展开/收起预设")}
      >
        <span className="presetbar__label">{t("预设")}</span>
        <span className="presetbar__count">{list.length}</span>
        {activeName && <span className="presetbar__active">{activeName}</span>}
        <span className="presetbar__arrow" aria-hidden>
          ▾
        </span>
      </button>

      {open && (
        <div className="presetbar__panel">
          <div className="presetbar__chips">
            {list.map((p) => (
              <span key={p.name} className="presetchip">
                <button
                  type="button"
                  className={`presetchip__main${activeName === p.name ? " is-active" : ""}`}
                  onClick={() => onApply(p)}
                  title={p.builtin ? t("内置预设") : t("自定义预设")}
                >
                  {p.name}
                </button>
                {!p.builtin && (
                  <button
                    type="button"
                    className="presetchip__del"
                    title={t("删除此预设")}
                    onClick={() => onDelete(p.name)}
                  >
                    ×
                  </button>
                )}
              </span>
            ))}

            {naming ? (
              <span className="presetchip presetchip--editing">
                <input
                  autoFocus
                  value={name}
                  placeholder={t("预设名称")}
                  onChange={(e) => setName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") submit();
                    if (e.key === "Escape") setNaming(false);
                  }}
                />
                <button type="button" className="presetchip__ok" onClick={submit}>
                  ✓
                </button>
              </span>
            ) : (
              <button
                type="button"
                className="presetchip presetchip--new"
                onClick={() => setNaming(true)}
                title={t("把当前设置保存为预设")}
              >
                ＋ {t("保存当前")}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
