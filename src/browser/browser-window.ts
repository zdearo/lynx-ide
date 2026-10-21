/**
 * The browser window is the same interface shown with only the browser. The backend opens it at
 * `index.html?window=browser`; the window's own label is checked as well, in case the query is
 * dropped on the way. A tab in its own window is labelled `browser-page-<id>`.
 */
export function isBrowserWindow(): boolean {
  if (new URLSearchParams(window.location.search).get("window") === "browser") {
    return true;
  }
  const label = windowLabel();
  return label === "browser" || pageOfLabel(label) !== null;
}

/**
 * The page a tab's own window shows: `page` in the query (`index.html?window=browser&page=<id>`),
 * or the number at the end of its label. Null in the main window and in a window that is not one.
 */
export function browserWindowPage(): number | null {
  const fromQuery = parsePage(
    new URLSearchParams(window.location.search).get("page"),
  );
  if (fromQuery !== null) return fromQuery;
  return pageOfLabel(windowLabel());
}

function windowLabel(): string | undefined {
  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__?: {
        metadata?: { currentWindow?: { label?: string } };
      };
    }
  ).__TAURI_INTERNALS__;
  return internals?.metadata?.currentWindow?.label;
}

function pageOfLabel(label: string | undefined): number | null {
  const match = /^browser-page-(\d+)$/.exec(label ?? "");
  return match ? parsePage(match[1]) : null;
}

function parsePage(text: string | null | undefined): number | null {
  if (text === null || text === undefined || !/^\d+$/.test(text)) return null;
  const page = Number(text);
  return Number.isSafeInteger(page) ? page : null;
}
