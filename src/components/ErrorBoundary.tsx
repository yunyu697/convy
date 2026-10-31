import { Component, type ErrorInfo, type ReactNode } from "react";
import { api } from "../api";

/**
 * 界面崩溃兜底。
 *
 * 为什么必须有：桌面应用没有浏览器那种「刷新一下」的退路。任何组件在渲染期抛异常，
 * React 会卸载整棵树 —— 用户看到的就是一片空白，既没有提示也没有入口，
 * 只能删配置目录或者重装。这个项目第 4 轮修过一次白屏（settings 为 null 卡在 spinner），
 * 当时的看门狗只覆盖了「设置加载失败」这一种情况，管不到渲染异常。
 *
 * 这里要做的是：把白屏换成一张能读懂的错误页，并且给两条出路 ——
 * 重新加载（任务队列在后端，重新加载不会丢任务）、打开日志（把现场交出来）。
 */
interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  componentStack: string | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, componentStack: null };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const detail = [
      error.message,
      error.stack ?? "(无堆栈)",
      info.componentStack ?? "(无组件栈)",
    ].join("\n");
    // 写日志失败不能再抛，否则会掉进「错误边界自己出错」的死循环
    void api.logFrontendError(detail).catch(() => {});
    this.setState({ componentStack: info.componentStack ?? null });
  }

  private reload = () => {
    window.location.reload();
  };

  private openLog = () => {
    void api.openLogFile().catch(() => {});
  };

  render() {
    const { error, componentStack } = this.state;
    if (!error) return this.props.children;

    const detail = [error.message, error.stack, componentStack]
      .filter(Boolean)
      .join("\n\n");

    return (
      <div className="crash">
        <div className="crash__card">
          <div className="crash__badge" aria-hidden>
            !
          </div>
          <h1 className="crash__title">界面遇到了问题</h1>
          <p className="crash__lead">
            转换任务保存在后台，<strong>重新加载不会丢失队列</strong>。
            如果反复出现，点「打开日志」把文件发给开发者。
          </p>

          <pre className="crash__detail">{detail}</pre>

          <div className="crash__actions">
            <button className="btn btn--primary" onClick={this.reload}>
              重新加载界面
            </button>
            <button className="btn btn--ghost" onClick={this.openLog}>
              打开日志
            </button>
          </div>

          <p className="crash__hint">
            日志位置：系统临时目录下的 <code>convy.log</code>
          </p>
        </div>
      </div>
    );
  }
}
