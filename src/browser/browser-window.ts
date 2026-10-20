/**
 * The browser window is the same interface shown with only the browser. The backend opens it at
 * `index.html?window=browser`; the window's own label is checked as well, in case the query is
 * dropped on the way.
 */
export function isBrowserWindow(): boolean {
  if (new URLSearchParams(window.location.search).get("window") === "browser") {
    return true;
  }
  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__?: {
        metadata?: { currentWindow?: { label?: string } };
      };
    }
  ).__TAURI_INTERNALS__;
  return internals?.metadata?.currentWindow?.label === "browser";
}
