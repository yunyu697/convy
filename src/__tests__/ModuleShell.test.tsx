import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import ModuleShell, { type ModuleShellProps } from "../modules/ModuleShell";
import type { Job } from "../types";

// 没开 vitest globals 时 RTL 不会自动注册 cleanup，得手动挂，
// 否则上一个用例的 DOM 会留在 document 里，getByRole 会撞到多个节点
afterEach(cleanup);

// ModuleShell / JobRow 只在「引擎缺失」提示里用 api，这里给个空壳避免加载 Tauri 运行时
vi.mock("../api", () => ({
  api: {
    openPath: vi.fn(async () => {}),
    revealOutput: vi.fn(async () => {}),
    setTarget: vi.fn(async () => {}),
  },
  events: {},
}));

/** 必须是稳定引用：JobRow 的 useEffect 依赖它，每次新建函数会让每行反复触发 */
const TARGETS = async () => ["mp4", "mkv"];
const noop = () => {};

function makeJobs(n: number, domain: Job["domain"] = "video"): Job[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `j${i}`,
    source: `C:/in/${i}.mp4`,
    fileName: `file-${i}.mp4`,
    size: 1024,
    domain,
    sourceExt: "mp4",
    targetFormat: "mkv",
    relDir: null,
    status: "pending" as const,
    progress: 0,
    detail: null,
    outputPath: null,
    outputSize: null,
    error: null,
    startedAt: null,
    finishedAt: null,
  }));
}

function props(over: Partial<ModuleShellProps> = {}): ModuleShellProps {
  return {
    domain: "video",
    settingsPanel: <div />,
    presets: [],
    activePreset: null,
    onApplyPreset: noop,
    onSavePreset: noop,
    onDeletePreset: noop,
    jobs: [],
    liveProgress: {},
    targetsCache: {},
    onTargets: TARGETS,
    onPick: noop,
    onPickFolder: noop,
    onChangeTarget: noop,
    onRemove: noop,
    onCancel: noop,
    onRetry: noop,
    onStart: noop,
    onClearFinished: noop,
    busy: false,
    ...over,
  };
}

const rowCount = (c: HTMLElement) => c.querySelectorAll(".row").length;
const listOf = (c: HTMLElement) => c.querySelector(".queue__list") as HTMLElement;

describe("队列增量渲染", () => {
  it("3000 条任务首屏只挂载 80 行", () => {
    const { container } = render(<ModuleShell {...props({ jobs: makeJobs(3000) })} />);
    expect(rowCount(container)).toBe(80);
    expect(screen.getByRole("button", { name: /已显示 80 \/ 3000/ })).toBeTruthy();
  });

  it("点「加载更多」追加一页", () => {
    const { container } = render(<ModuleShell {...props({ jobs: makeJobs(300) })} />);
    fireEvent.click(screen.getByRole("button", { name: /已显示 80 \/ 300/ }));
    expect(rowCount(container)).toBe(160);
  });

  it("滚到接近底部自动追加", () => {
    const { container } = render(<ModuleShell {...props({ jobs: makeJobs(300) })} />);
    fireEvent.scroll(listOf(container));
    expect(rowCount(container)).toBe(160);
  });

  it("列表不足一页时不显示「加载更多」", () => {
    const { container } = render(<ModuleShell {...props({ jobs: makeJobs(12) })} />);
    expect(rowCount(container)).toBe(12);
    expect(screen.queryByRole("button", { name: /已显示/ })).toBeNull();
  });

  it("切换模块后回到首屏行数", () => {
    const { container, rerender } = render(
      <ModuleShell {...props({ jobs: [...makeJobs(300), ...makeJobs(300, "audio")] })} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /已显示 80 \/ 300/ }));
    expect(rowCount(container)).toBe(160);

    rerender(
      <ModuleShell
        {...props({
          domain: "audio",
          jobs: [...makeJobs(300), ...makeJobs(300, "audio")],
        })}
      />,
    );
    expect(rowCount(container)).toBe(80);
  });
});
