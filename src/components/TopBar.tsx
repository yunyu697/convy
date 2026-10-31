import { useEffect, useRef, useState } from "react";
import { ACCENTS, THEMES } from "../types";

function ThemePicker({
  theme,
  accent,
  onTheme,
  onAccent,
}: {
  theme: string;
  accent: string;
  onTheme: (id: string) => void;
  onAccent: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const current = THEMES.find((t) => t.id === theme) ?? THEMES[0];
  const currentAccent = ACCENTS.find((a) => a.id === accent) ?? ACCENTS[0];

  return (
    <div className="themepicker" ref={ref}>
      <button
        type="button"
        className="themepicker__trigger"
        onClick={() => setOpen(!open)}
        title="外观"
      >
        <span className="themepicker__swatch">
          <i style={{ background: current.swatch[0] }} />
          <i style={{ background: current.swatch[1] }} />
          <i style={{ background: current.swatch[2] }} />
        </span>
        <span className="themepicker__name">{current.label}</span>
      </button>

      {open && (
        <div className="themepicker__pop">
          <div className="themepicker__section">
            <div className="themepicker__label">主题</div>
            <div className="themepicker__grid">
              {THEMES.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  className={`themeopt${t.id === theme ? " is-active" : ""}`}
                  onClick={() => onTheme(t.id)}
                  title={t.label}
                >
                  <span className="themeopt__swatch">
                    <i style={{ background: t.swatch[0] }} />
                    <i style={{ background: t.swatch[1] }} />
                    <i style={{ background: t.swatch[2] }} />
                  </span>
                  <span className="themeopt__label">{t.label}</span>
                </button>
              ))}
            </div>
          </div>

          <div className="themepicker__section">
            <div className="themepicker__label">强调色</div>
            <div className="themepicker__accents">
              {ACCENTS.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  className={`accentdot${a.id === accent ? " is-active" : ""}`}
                  style={{ background: a.color }}
                  title={a.label}
                  onClick={() => onAccent(a.id)}
                />
              ))}
            </div>
            <div className="themepicker__accname">{currentAccent.label}</div>
          </div>
        </div>
      )}
    </div>
  );
}

export function TopBar({
  title,
  subtitle,
  theme,
  accent,
  onTheme,
  onAccent,
  right,
}: {
  title: string;
  subtitle?: string;
  theme: string;
  accent: string;
  onTheme: (id: string) => void;
  onAccent: (id: string) => void;
  right?: React.ReactNode;
}) {
  return (
    <header className="topbar">
      <div className="topbar__heading">
        <h1 className="topbar__title">{title}</h1>
        {subtitle && <p className="topbar__sub">{subtitle}</p>}
      </div>
      <div className="topbar__actions">
        {right}
        <ThemePicker theme={theme} accent={accent} onTheme={onTheme} onAccent={onAccent} />
      </div>
    </header>
  );
}
