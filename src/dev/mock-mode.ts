/**
 * True when the page runs against the in-memory backend (`?mock=…`, see main.tsx). Always false in
 * a release build, so UI that only the mock should show never appears in the real app.
 */
export function isMockBackend(): boolean {
  return (
    import.meta.env.DEV &&
    Boolean(new URLSearchParams(window.location.search).get("mock"))
  );
}
