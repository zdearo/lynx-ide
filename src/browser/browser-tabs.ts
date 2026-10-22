import type { BrowserPageInfo, HarnessId } from "@/bindings";
import { copy } from "@/ui-copy";

/** Every harness the icon set draws; the record makes the compiler name any one that is missing. */
const HARNESS_IDS: Record<HarnessId, true> = {
  claude: true,
  codex: true,
  opencode: true,
  omp: true,
};

/** An agent's name comes from a command line, so it can be anything; only the known ones get a mark. */
export function isHarnessId(value: string): value is HarnessId {
  return Object.hasOwn(HARNESS_IDS, value);
}

/** The name shown for an agent; a harness without a name of its own is just "Agent". */
export function agentName(harness: string): string {
  const names: Readonly<Record<string, string>> = copy.browser.agentNames;
  return Object.hasOwn(names, harness)
    ? names[harness]
    : copy.browser.agentNames.unknown;
}

/** The tabs that belong to one worktree, oldest first. Bot pages are left out. */
export function worktreePages(
  pages: readonly BrowserPageInfo[],
  worktreePath: string,
): BrowserPageInfo[] {
  return pages
    .filter(
      (page) =>
        page.owner.kind === "workspace" &&
        page.owner.worktreePath === worktreePath,
    )
    .sort((a, b) => a.id - b.id);
}
