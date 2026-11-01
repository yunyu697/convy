import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Field, Group, NumberInput, Segmented, Toggle } from "../components/ui";
import { LANGUAGES, langLabel, useT } from "../i18n";
import UpdateSection from "./UpdateSection";
import {
  ACCENTS,
  THEMES,
  type AppUpdateInfo,
  type GlobalSettings,
} from "../types";

export default function SettingsModule({
  value,
  onChange,
  onToast,
  onShowGuide,
  update,
  onCheckUpdate,
}: {
  value: GlobalSettings;
  onChange: (v: GlobalSettings) => void;
  onToast: (kind: "ok" | "err" | "info", text: string) => void;
  /** 重新打开首次启动向导 */
  onShowGuide: () => void;
  /** 上一次应用更新检查的结果（启动时查过就带过来，避免重复请求） */
  update: AppUpdateInfo | null;
  onCheckUpdate: (force: boolean) => Promise<AppUpdateInfo | null>;
}) {
  const t = useT();
  const set = <K extends keyof GlobalSettings>(k: K, v: GlobalSettings[K]) =>
    onChange({ ...value, [k]: v });
  const patch = (p: Partial<GlobalSettings>) => onChange({ ...value, ...p });

  const [picking, setPicking] = useState(false);

  const chooseFolder = async () => {
    setPicking(true);
    try {
      const dir = await open({ directory: true, multiple: false, title: t("选择输出目录") });
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
        <Group title={t("外观")} desc={t("主题、强调色与语言")}>
          <Field label={t("主题")}>
            <div className="themegrid">
              {THEMES.map((th) => (
                <button
                  key={th.id}
                  type="button"
                  className={`themegrid__item${th.id === value.theme ? " is-active" : ""}`}
                  onClick={() => set("theme", th.id)}
                >
                  <span className="themegrid__swatch">
                    <i style={{ background: th.swatch[0] }} />
                    <i style={{ background: th.swatch[1] }} />
                    <i style={{ background: th.swatch[2] }} />
                  </span>
                  <span className="themegrid__label">{t(th.label)}</span>
                </button>
              ))}
            </div>
          </Field>

          <Field label={t("强调色")} hint={t("按钮、进度条与高亮")}>
            <div className="accentrow">
              {ACCENTS.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  className={`accentrow__dot${a.id === value.accent ? " is-active" : ""}`}
                  style={{ background: a.color }}
                  title={t(a.label)}
                  onClick={() => set("accent", a.id)}
                />
              ))}
              <span className="accentrow__label">
                {t(ACCENTS.find((a) => a.id === value.accent)?.label ?? "")}
              </span>
            </div>
          </Field>

          <Field label={t("语言")} hint={t("切换后界面立即生效")} wide>
            <div className="langgrid">
              {LANGUAGES.map((l) => (
                <button
                  key={l.id}
                  type="button"
                  className={`langgrid__item${l.id === value.language ? " is-active" : ""}`}
                  onClick={() => set("language", l.id)}
                >
                  {langLabel(l.id)}
                </button>
              ))}
            </div>
          </Field>
        </Group>

        <Group title={t("队列")} desc={t("并发与输出位置")}>
          <Field label={t("并发任务数")} hint={t("同时运行的转换数量，硬件吃紧时调小")}>
            <NumberInput
              value={value.concurrentJobs}
              onChange={(v) => set("concurrentJobs", v)}
              min={1}
              max={8}
              suffix={t("个")}
            />
          </Field>

          <Field label={t("同名文件")} hint={t("已存在同名输出时")}>
            <Segmented
              value={value.overwrite ? "overwrite" : "rename"}
              onChange={(v) => set("overwrite", v === "overwrite")}
              options={[
                { value: "rename", label: t("自动重命名 (1)(2)") },
                { value: "overwrite", label: t("直接覆盖") },
              ]}
            />
          </Field>

          <Field label={t("输出目录")} hint={t("留空表示输出到源文件所在目录")} wide>
            <div className="field__row field__row--path">
              <input
                className="text-input"
                value={value.outputFolder}
                placeholder={t("与源文件同目录")}
                onChange={(e) => set("outputFolder", e.target.value)}
              />
              <button
                className="btn btn--ghost"
                onClick={() => void chooseFolder()}
                disabled={picking}
              >
                {picking ? t("选择中…") : t("浏览…")}
              </button>
              {value.outputFolder && (
                <button className="btn btn--ghost" onClick={() => set("outputFolder", "")}>
                  {t("清除")}
                </button>
              )}
            </div>
          </Field>

          <Field
            label={t("保持目录结构")}
            hint={t("批量导入文件夹时，在输出目录下还原原有子目录层级（未设输出目录时不生效）")}
            wide
          >
            <Toggle
              checked={value.preserveTree}
              onChange={(v) => set("preserveTree", v)}
              label={value.preserveTree ? t("按原层级输出") : t("全部平铺到输出目录")}
            />
          </Field>
        </Group>

        <UpdateSection
          info={update}
          onCheck={onCheckUpdate}
          settings={value}
          onPatch={patch}
          onToast={onToast}
        />

        <Group title={t("引擎")} desc={t("第三方工具的更新策略")}>
          <Field label={t("启动时自动检查更新")} hint={t("联网查询各引擎的最新版本")}>
            <Toggle
              checked={value.autoCheckUpdate}
              onChange={(v) => set("autoCheckUpdate", v)}
              label={value.autoCheckUpdate ? t("已启用") : t("已关闭")}
            />
          </Field>
        </Group>

        <Group title={t("关于")} defaultOpen={false}>
          <Field label={t("新手引导")} hint={t("忘了怎么开始？重新走一遍三步设置")} wide>
            <button type="button" className="btn btn--ghost" onClick={onShowGuide}>
              {t("重新查看")}
            </button>
          </Field>

          <div className="prose">
            <p>
              <strong>Convy · {t("万象转换")}</strong>{" "}
              {t("是一个跨平台格式转换工具，界面层使用 Tauri 2 + React，转换能力由 FFmpeg、ImageMagick、Calibre、LibreOffice、Pandoc 等开源引擎提供。")}
            </p>
            <p>
              {t("本工具仅以独立进程方式调用这些引擎，不修改其代码，各引擎遵循其原有开源许可证。")}
            </p>
            <p className="prose__muted">
              {t("所有改动会自动保存到系统应用数据目录的")} <code>settings.json</code>
              {t("。")}
            </p>
          </div>
        </Group>
      </div>
    </div>
  );
}
