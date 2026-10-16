import { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";

/**
 * Draws the terminal with WebGL when the browser allows it. Block and box
 * characters are then drawn by code, so they line up whatever font the system
 * falls back to. Without WebGL, or after the context is lost, xterm keeps its
 * DOM renderer, which draws those characters from the font.
 */
export function loadWebglRenderer(terminal: Terminal): void {
  let addon: WebglAddon | undefined;
  try {
    addon = new WebglAddon();
    terminal.loadAddon(addon);
  } catch {
    addon?.dispose();
    return;
  }

  // Browsers drop contexts under memory pressure or when too many are open.
  const loaded = addon;
  loaded.onContextLoss(() => loaded.dispose());
}
