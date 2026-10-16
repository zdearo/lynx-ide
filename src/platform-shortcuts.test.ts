import { describe, expect, it } from "vitest";

import { matchShortcut, shortcutLabel } from "@/platform-shortcuts";

const press = (
  key: string,
  mods: Partial<Record<"meta" | "ctrl" | "shift" | "alt", boolean>>,
) => ({
  key,
  metaKey: !!mods.meta,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
});

describe("matchShortcut", () => {
  it("uses Cmd on macOS", () => {
    expect(matchShortcut(press("t", { meta: true }), true)).toBe("newTerminal");
    expect(matchShortcut(press("W", { meta: true }), true)).toBe(
      "closeTerminal",
    );
    expect(matchShortcut(press("n", { meta: true }), true)).toBe("newWorktree");
    expect(matchShortcut(press("b", { meta: true }), true)).toBe(
      "toggleSidebar",
    );
  });

  it("uses Ctrl+Shift elsewhere, so plain Ctrl keys still reach the shell", () => {
    expect(matchShortcut(press("T", { ctrl: true, shift: true }), false)).toBe(
      "newTerminal",
    );
    expect(matchShortcut(press("t", { ctrl: true }), false)).toBeNull();
    expect(matchShortcut(press("w", { ctrl: true }), false)).toBeNull();
  });

  it("opens settings with Cmd+, or plain Ctrl+, since no shell uses it", () => {
    expect(matchShortcut(press(",", { meta: true }), true)).toBe(
      "openSettings",
    );
    expect(matchShortcut(press(",", { ctrl: true }), false)).toBe(
      "openSettings",
    );
    expect(
      matchShortcut(press(",", { ctrl: true, shift: true }), false),
    ).toBeNull();
    expect(matchShortcut(press("t", { ctrl: true }), false)).toBeNull();
  });

  it("toggles the files panel like the other shell-clashing shortcuts", () => {
    expect(matchShortcut(press("e", { meta: true }), true)).toBe(
      "toggleFilesPanel",
    );
    expect(matchShortcut(press("E", { ctrl: true, shift: true }), false)).toBe(
      "toggleFilesPanel",
    );
    expect(matchShortcut(press("e", { ctrl: true }), false)).toBeNull();
  });

  it("ignores other modifier combinations and unknown keys", () => {
    expect(
      matchShortcut(press("t", { meta: true, shift: true }), true),
    ).toBeNull();
    expect(
      matchShortcut(press("t", { ctrl: true, meta: true }), true),
    ).toBeNull();
    expect(
      matchShortcut(press("t", { ctrl: true, shift: true, alt: true }), false),
    ).toBeNull();
    expect(
      matchShortcut(press("q", { ctrl: true, shift: true }), false),
    ).toBeNull();
    expect(matchShortcut(press("t", {}), false)).toBeNull();
  });
});

describe("shortcutLabel", () => {
  it("renders the platform's convention", () => {
    expect(shortcutLabel("newTerminal", true)).toBe("⌘T");
    expect(shortcutLabel("newTerminal", false)).toBe("Ctrl+Shift+T");
    expect(shortcutLabel("openSettings", true)).toBe("⌘,");
    expect(shortcutLabel("openSettings", false)).toBe("Ctrl+,");
  });
});
