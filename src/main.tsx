import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

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
      <App />
    </React.StrictMode>,
  );
});
