import type { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";

/**
 * Browsers allow about sixteen WebGL contexts, and each one costs GPU memory. The
 * budget keeps WebGL on the terminals shown most recently and hands the rest to
 * xterm's DOM renderer. Switching among recent tabs stays instant, and a tab
 * that has fallen out of the budget gets its context back when it is shown again.
 */
export class WebglBudget {
  /** Slots holding a context or loading one, most recently shown first. */
  private readonly holders: WebglRendererSlot[] = [];

  constructor(private readonly size: number) {}

  slotFor(terminal: Terminal): WebglRendererSlot {
    return new WebglRendererSlot(terminal, this);
  }

  /** Moves a slot to the front, releasing the least recently shown ones beyond the size. */
  promote(slot: WebglRendererSlot): void {
    this.forget(slot);
    this.holders.unshift(slot);
    while (this.holders.length > this.size) {
      this.holders.pop()?.release();
    }
  }

  forget(slot: WebglRendererSlot): void {
    const index = this.holders.indexOf(slot);
    if (index !== -1) this.holders.splice(index, 1);
  }
}

/** The WebGL renderer of one terminal, drawn only while the budget has room for it. */
export class WebglRendererSlot {
  private state: "none" | "loading" | "attached" = "none";
  private addon: WebglAddon | null = null;
  private disposed = false;

  constructor(
    private readonly terminal: Terminal,
    private readonly budget: WebglBudget,
  ) {}

  /** Called when this terminal becomes the visible one. */
  async show(): Promise<void> {
    if (this.disposed) return;
    if (this.state !== "none") {
      this.budget.promote(this);
      return;
    }
    this.state = "loading";
    this.budget.promote(this);

    // Loaded on first use, so the startup bundle does not carry the addon.
    const { WebglAddon: Addon } = await import("@xterm/addon-webgl");
    // Released while the module was loading, so there is nothing to attach.
    if (this.state !== "loading") return;

    let addon: WebglAddon | undefined;
    try {
      addon = new Addon();
      this.terminal.loadAddon(addon);
    } catch {
      addon?.dispose();
      this.forget();
      return;
    }
    // Browsers drop contexts under memory pressure: forget the addon, keep the DOM renderer.
    const loaded = addon;
    loaded.onContextLoss(() => {
      if (this.addon === loaded) {
        this.addon = null;
        this.forget();
      }
      loaded.dispose();
    });
    this.addon = loaded;
    this.state = "attached";
  }

  /** Gives the context back: the terminal falls back to the DOM renderer. */
  release(): void {
    const addon = this.addon;
    this.addon = null;
    this.forget();
    addon?.dispose();
  }

  dispose(): void {
    this.disposed = true;
    this.release();
  }

  private forget(): void {
    this.state = "none";
    this.budget.forget(this);
  }
}

/** The budget the app uses: WebGL for the six terminals shown most recently. */
export const webglBudget = new WebglBudget(6);
