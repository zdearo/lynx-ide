import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DetachedBrowserApp } from "@/browser/DetachedBrowserApp";
import { isBrowserWindow } from "@/browser/browser-window";
import "./index.css";

// Right-click never opens the web view's own menu. Views that have one of their
// own (a worktree, a file row) open it instead.
window.addEventListener("contextmenu", (event) => event.preventDefault());

/** `?mock=full` opens the interface in a plain browser against an in-memory backend. */
async function installDevBackend() {
  if (!import.meta.env.DEV) return;
  const scenario = new URLSearchParams(window.location.search).get("mock");
  if (!scenario) return;
  const { installMockTauri } = await import("@/dev/mock-tauri");
  installMockTauri(scenario);
}

void installDevBackend().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {isBrowserWindow() ? <DetachedBrowserApp /> : <App />}
    </React.StrictMode>,
  );
});
