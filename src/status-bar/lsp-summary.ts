import { copy } from "@/ui-copy";
import type { LspServerStatus } from "@/status-bar/lsp-status-store";

/** What the status bar says about the language servers: none, or each one with its state. */
export function lspSummary(servers: readonly LspServerStatus[]): string {
  const text = copy.statusBar.lsp;
  if (servers.length === 0) return text.none;
  const list = servers
    .map((server) => `${server.language} (${text.state[server.state]})`)
    .join(", ");
  return text.running(list);
}
