import React from "react";
import ReactDOM from "react-dom/client";
import { ResultPanel } from "./result/ResultPanel";
import { SettingsApp } from "./settings/SettingsApp";
import "./styles/base.css";
import "./styles/controls.css";

if (import.meta.hot) {
  const report = (kind: string, detail: unknown) =>
    import.meta.hot?.send("webview:log", { view: window.location.search, kind, detail: String(detail) });
  window.addEventListener("error", (event) => report("error", event.error?.stack ?? event.message));
  window.addEventListener("unhandledrejection", (event) => report("rejection", event.reason?.stack ?? event.reason));
  window.setTimeout(() => report("boot", `${document.body.innerText.slice(0, 80).replace(/\s+/g, " ")} | ${window.innerWidth}x${window.innerHeight} visible=${document.visibilityState}`), 1500);
}

const isResultPanel = new URLSearchParams(window.location.search).get("view") === "result";
document.documentElement.dataset.view = isResultPanel ? "result" : "settings";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isResultPanel ? <ResultPanel /> : <SettingsApp />}</React.StrictMode>,
);
