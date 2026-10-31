import { useEffect, useRef, useState, type ReactNode } from "react";

/** 带说明文字的字段容器 */
export function Field({
  label,
  hint,
  children,
  wide,
  locked,
  lockedText = "跟随源文件",
}: {
  label: string;
  hint?: string;
  children: ReactNode;
  wide?: boolean;
  /** 该字段当前由「保持源参数」接管，控件不可用，显示为只读提示 */
  locked?: boolean;
  lockedText?: string;
}) {
  return (
    <label className={`field${wide ? " field--wide" : ""}${locked ? " is-locked" : ""}`}>
      <span className="field__label">
        {label}
        {hint && <em className="field__hint">{hint}</em>}
      </span>
      <span className="field__control">
        {locked ? (
          <span className="field__locked" title="由「保持源参数」接管，输出与源文件一致">
            {lockedText}
          </span>
        ) : (
          children
        )}
      </span>
    </label>
  );
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  disabled,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
  disabled?: boolean;
}) {
  return (
    <div className="select">
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <span className="select__arrow" aria-hidden>
        ▾
      </span>
    </div>
  );
}

/** 分段按钮：互斥选项的一行，比下拉框更快 */
export function Segmented<T extends string>({
  value,
  onChange,
  options,
  compact,
  disabled,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string; title?: string }[];
  compact?: boolean;
  disabled?: boolean;
}) {
  return (
    <div
      className={`segmented${compact ? " segmented--compact" : ""}${disabled ? " is-disabled" : ""}`}
      role="tablist"
      aria-disabled={disabled || undefined}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          title={o.title ?? o.label}
          aria-selected={o.value === value}
          disabled={disabled}
          className={o.value === value ? "is-active" : ""}
          onClick={() => !disabled && onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** 开关 */
export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
}) {
  return (
    <button
      type="button"
      className={`toggle${checked ? " is-on" : ""}`}
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle__track">
        <span className="toggle__thumb" />
      </span>
      {label && <span className="toggle__label">{label}</span>}
    </button>
  );
}

/** 数字输入，带步进按钮 */
export function NumberInput({
  value,
  onChange,
  min = 0,
  max = 99999,
  step = 1,
  suffix,
  disabled,
}: {
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
  disabled?: boolean;
}) {
  const [text, setText] = useState(String(value));
  const focused = useRef(false);

  useEffect(() => {
    if (!focused.current) setText(String(value));
  }, [value]);

  const clamp = (v: number) => Math.min(max, Math.max(min, v));

  return (
    <div className="number">
      <button
        type="button"
        className="number__step"
        disabled={disabled || value <= min}
        onClick={() => onChange(clamp(value - step))}
        aria-label="减少"
      >
        −
      </button>
      <input
        type="number"
        value={text}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onFocus={() => (focused.current = true)}
        onBlur={() => {
          focused.current = false;
          const n = Number(text);
          setText(String(Number.isFinite(n) ? clamp(n) : value));
        }}
        onChange={(e) => {
          setText(e.target.value);
          const n = Number(e.target.value);
          if (Number.isFinite(n)) onChange(clamp(n));
        }}
      />
      {suffix && <span className="number__suffix">{suffix}</span>}
      <button
        type="button"
        className="number__step"
        disabled={disabled || value >= max}
        onClick={() => onChange(clamp(value + step))}
        aria-label="增加"
      >
        +
      </button>
    </div>
  );
}

/** 滑块 + 数值 */
export function Slider({
  value,
  onChange,
  min,
  max,
  step = 1,
  format,
  disabled,
}: {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  step?: number;
  format?: (v: number) => string;
  disabled?: boolean;
}) {
  const pct = max === min ? 0 : ((value - min) / (max - min)) * 100;
  return (
    <div className="slider">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        style={{ ["--pct" as string]: `${pct}%` }}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <span className="slider__value">{format ? format(value) : value}</span>
    </div>
  );
}

export function TextInput({
  value,
  onChange,
  placeholder,
  disabled,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  disabled?: boolean;
}) {
  return (
    <input
      className="text-input"
      type="text"
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/** 可折叠的设置分组 */
export function Group({
  title,
  desc,
  children,
  defaultOpen = true,
}: {
  title: string;
  desc?: string;
  children: ReactNode;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className={`group${open ? " is-open" : ""}`}>
      <button type="button" className="group__head" onClick={() => setOpen(!open)}>
        <span className="group__chev" aria-hidden>
          ▸
        </span>
        <span className="group__title">{title}</span>
        {desc && <span className="group__desc">{desc}</span>}
      </button>
      {open && <div className="group__body">{children}</div>}
    </section>
  );
}
