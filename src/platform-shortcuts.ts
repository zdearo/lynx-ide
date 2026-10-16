/**
 * Terminals need Ctrl+T, Ctrl+W, Ctrl+N and Ctrl+B for themselves (transpose,
 * delete word, history, cursor), so outside macOS app shortcuts use Ctrl+Shift.
 */
export type ShortcutAction =
  "newTerminal" | "closeTerminal" | "newWorktree" | "toggleSidebar";

const SHORTCUT_KEYS: Record<ShortcutAction, string> = {
  newTerminal: "t",
  closeTerminal: "w",
  newWorktree: "n",
  toggleSidebar: "b",
};

const ACTIONS: ShortcutAction[] = [
  "newTerminal",
  "closeTerminal",
  "newWorktree",
  "toggleSidebar",
];

export function isMacPlatform(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.userAgent);
}

type KeyEventLike = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey"
>;

export function matchShortcut(
  event: KeyEventLike,
  mac = isMacPlatform(),
): ShortcutAction | null {
  if (event.altKey) return null;
  const modifiersMatch = mac
    ? event.metaKey && !event.ctrlKey && !event.shiftKey
    : event.ctrlKey && event.shiftKey && !event.metaKey;
  if (!modifiersMatch) return null;

  const key = event.key.toLowerCase();
  return ACTIONS.find((action) => SHORTCUT_KEYS[action] === key) ?? null;
}

export function shortcutLabel(
  action: ShortcutAction,
  mac = isMacPlatform(),
): string {
  const key = SHORTCUT_KEYS[action].toUpperCase();
  return mac ? `⌘${key}` : `Ctrl+Shift+${key}`;
}
