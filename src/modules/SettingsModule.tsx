import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Field, Group, NumberInput, Segmented, Toggle } from "../components/ui";
import { ACCENTS, THEMES, type GlobalSettings } from "../types";

export default function SettingsModule({
  value,
  onChange,
  onToast,
  onShowGuide,
}: {
  value: GlobalSettings;
  onChange: (v: GlobalSettings) => void;
  onToast: (kind: "ok" | "err" | "info", text: string) => void;
  /** 重新打开首次启动向导 */
  onShowGuide: () => void;
}) {
  const set = <K extends keyof GlobalSettings>(k: K, v: GlobalSettings[K]) =>
    onChange({ ...value, [k]: v });

  const [picking, setPicking] = useState(false);

  const chooseFolder = async () => {
    setPicking(true);
    try {
      const dir = await open({ directory: true, multiple: false, title: "选择输出目录" });
      if (typeof dir === "string") set("outputFolder", dir);
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setPicking(false);
    }
  };

  return (
    <div className="settingspage">
      <div className="settingspage__card">
        <Group title="外观" desc="主题与强调色">
          <Field label="主题">
            <div className="themegrid">
              {THEMES.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  className={`themegrid__item${t.id === value.theme ? " is-active" : ""}`}
                  onClick={() => set("theme", t.id)}
                >
                  <span className="themegrid__swatch">
                    <i style={{ background: t.swatch[0] }} />
                    <i style={{ background: t.swatch[1] }} />
                    <i style={{ background: t.swatch[2] }} />
                  </span>
                  <span className="themegrid__label">{t.label}</span>
                </button>
              ))}
            </div>
          </Field>

          <Field label="强调色" hint="按钮、进度条与高亮">
            <div className="accentrow">
              {ACCENTS.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  className={`accentrow__dot${a.id === value.accent ? " is-active" : ""}`}
                  style={{ background: a.color }}
                  title={a.label}
                  onClick={() => set("accent", a.id)}
                />
              ))}
              <span className="accentrow__label">
                {ACCENTS.find((a) => a.id === value.accent)?.label}
              </span>
            </div>
          </Field>
        </Group>

        <Group title="队列" desc="并发与输出位置">
          <Field label="并发任务数" hint="同时运行的转换数量，硬件吃紧时调小">
            <NumberInput
              value={value.concurrentJobs}
              onChange={(v) => set("concurrentJobs", v)}
              min={1}
              max={8}
              suffix="个"
            />
          </Field>

          <Field label="输出目录" hint="留空表示输出到源文件所在目录" wide>
            <div className="field__row field__row--path">
              <input
                className="text-input"
                value={value.outputFolder}
                placeholder="与源文件同目录"
                onChange={(e) => set("outputFolder", e.target.value)}
              />
              <button className="btn btn--ghost" onClick={() => void chooseFolder()} disabled={picking}>
                {picking ? "选择中…" : "浏览…"}
              </button>
              {value.outputFolder && (
                <button className="btn btn--ghost" onClick={() => set("outputFolder", "")}>
                  清除
                </button>
              )}
            </div>
          </Field>

          <Field label="同名文件" hint="已存在同名输出时">
            <Segmented
              value={value.overwrite ? "overwrite" : "rename"}
              onChange={(v) => set("overwrite", v === "overwrite")}
              options={[
                { value: "rename", label: "自动重命名 (1)(2)" },
                { value: "overwrite", label: "直接覆盖" },
              ]}
            />
          </Field>

          <Field
            label="保持目录结构"
            hint="批量导入文件夹时，在输出目录下还原原有子目录层级（未设输出目录时不生效）"
            wide
          >
            <Toggle
              checked={value.preserveTree}
              onChange={(v) => set("preserveTree", v)}
              label={value.preserveTree ? "按原层级输出" : "全部平铺到输出目录"}
            />
          </Field>
        </Group>

        <Group title="引擎" desc="第三方工具的更新策略">
          <Field label="启动时自动检查更新" hint="联网查询各引擎的最新版本">
            <Toggle
              checked={value.autoCheckUpdate}
              onChange={(v) => set("autoCheckUpdate", v)}
              label={value.autoCheckUpdate ? "已启用" : "已关闭"}
            />
          </Field>
        </Group>

        <Group title="关于" defaultOpen={false}>
          <Field label="新手引导" hint="忘了怎么开始？重新走一遍三步设置">
            <button type="button" className="btn btn--ghost" onClick={onShowGuide}>
              重新查看
            </button>
          </Field>

          <div className="prose">
            <p>
              <strong>Convy · 万象转换</strong> 是一个跨平台格式转换工具，
              界面层使用 Tauri 2 + React，转换能力由 FFmpeg、ImageMagick、Calibre、
              LibreOffice、Pandoc 等开源引擎提供。
            </p>
            <p>
              本工具仅以独立进程方式调用这些引擎，不修改其代码，
              各引擎遵循其原有开源许可证。
            </p>
            <p className="prose__muted">
              所有改动会自动保存到系统应用数据目录的 <code>settings.json</code>。
            </p>
          </div>
        </Group>
      </div>
    </div>
  );
}
