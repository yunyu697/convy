import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { api } from "./api";
import "./styles.css";

// ErrorBoundary 只捕获「渲染期」异常；事件处理器和 Promise 链里的错误不会冒泡到它，
// 同样需要留下现场 —— 否则用户能描述的只有「点了没反应」，日志里什么都没有。
window.addEventListener("error", (e) => {
  void api
    .logFrontendError(`[window.error] ${e.message} @ ${e.filename}:${e.lineno}`)
    .catch(() => {});
});

window.addEventListener("unhandledrejection", (e) => {
  const r = e.reason;
  const text = r instanceof Error ? (r.stack ?? r.message) : String(r);
  void api.logFrontendError(`[unhandledrejection] ${text}`).catch(() => {});
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>
);
