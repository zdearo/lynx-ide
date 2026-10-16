/**
 * Terminals need Ctrl+T, Ctrl+W, Ctrl+N and Ctrl+B for themselves (transpose,
 * delete word, history, cursor), so outside macOS the shortcuts that collide
 * with a shell key use Ctrl+Shift. The rest use plain Ctrl, like Cmd on macOS.
 */
export type ShortcutAction =
  | "newTerminal"
  | "closeTerminal"
  | "newWorktree"
  | "toggleSidebar"
  | "openSettings"
  | "toggleFilesPanel";

interface ShortcutDefinition {
  key: string;
  /** Whether a shell already uses plain Ctrl+key, so Shift must be added outside macOS. */
  shellKey: boolean;
}

const SHORTCUTS: Record<ShortcutAction, ShortcutDefinition> = {
  newTerminal: { key: "t", shellKey: true },
  closeTerminal: { key: "w", shellKey: true },
  newWorktree: { key: "n", shellKey: true },
  toggleSidebar: { key: "b", shellKey: true },
  openSettings: { key: ",", shellKey: false },
  toggleFilesPanel: { key: "e", shellKey: true },
};

const ACTIONS = Object.keys(SHORTCUTS) as ShortcutAction[];

export function isMacPlatform(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.userAgent);
}

type KeyEventLike = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey"
>;

function modifiersMatch(
  event: KeyEventLike,
  definition: ShortcutDefinition,
  mac: boolean,
): boolean {
  if (event.altKey) return false;
  if (mac) return event.metaKey && !event.ctrlKey && !event.shiftKey;
  return (
    event.ctrlKey && !event.metaKey && event.shiftKey === definition.shellKey
  );
}

export function matchShortcut(
  event: KeyEventLike,
  mac = isMacPlatform(),
): ShortcutAction | null {
  const key = event.key.toLowerCase();
  return (
    ACTIONS.find(
      (action) =>
        SHORTCUTS[action].key === key &&
        modifiersMatch(event, SHORTCUTS[action], mac),
    ) ?? null
  );
}

export function shortcutLabel(
  action: ShortcutAction,
  mac = isMacPlatform(),
): string {
  const { key, shellKey } = SHORTCUTS[action];
  const shown = key.toUpperCase();
  if (mac) return `⌘${shown}`;
  return shellKey ? `Ctrl+Shift+${shown}` : `Ctrl+${shown}`;
}
