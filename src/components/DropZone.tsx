import { useState, type DragEvent } from "react";
import { useT } from "../i18n";

/**
 * 紧凑的添加文件入口：单行条（小图标 + 提示文字 + 选择文件 / 选择文件夹按钮）。
 * 拖放主要有两条通道：系统级拖拽（整个窗口的 dropveil 覆盖层）
 * 和本元素自带的 HTML5 拖放，均不受此条尺寸影响。
 */
export default function DropZone({
  onPick,
  onPickFolder,
  title,
  hint,
  compact,
}: {
  onPick: () => void;
  onPickFolder?: () => void;
  title?: string;
  hint?: string;
  compact?: boolean;
}) {
  const t = useT();
  const [over, setOver] = useState(false);

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    onPick();
  };

  return (
    <div
      className={`dropzone dropzone--slim${over ? " is-over" : ""}${compact ? " dropzone--compact" : ""}`}
      onClick={onPick}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") onPick();
      }}
    >
      <span className="dropzone__ring" aria-hidden>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
          <path
            d="M12 16V4m0 0L7.5 8.5M12 4l4.5 4.5"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <path
            d="M4 15v3.5A1.5 1.5 0 005.5 20h13a1.5 1.5 0 001.5-1.5V15"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </svg>
      </span>
      <span className="dropzone__text">
        {title ?? t("把文件或文件夹拖到这里")}{t("，或拖到窗口任意位置")}
      </span>
      {onPickFolder && (
        <button
          type="button"
          className="btn btn--ghost dropzone__btn"
          title={t("递归导入文件夹里所有受支持的文件")}
          onClick={(e) => {
            e.stopPropagation();
            onPickFolder();
          }}
        >
          {t("文件夹")}
        </button>
      )}
      <button
        type="button"
        className="btn btn--primary dropzone__btn"
        onClick={(e) => {
          e.stopPropagation();
          onPick();
        }}
      >
        {t("选择文件")}
      </button>
      {hint && <span className="dropzone__hint">{hint}</span>}
    </div>
  );
}
