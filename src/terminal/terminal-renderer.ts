import type { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";

/**
 * Browsers allow about sixteen WebGL contexts, and each one costs GPU memory. The budget gives the contexts to the
 * terminals seen on a screen most recently, and hands the rest to xterm's DOM renderer. A terminal that loses its
 * context keeps running and keeps its scrollback, and it gets a context back when it is asked for again.
 */
export class WebglBudget {
  /** Slots holding a context, or loading one. */
  private readonly holders: WebglRendererSlot[] = [];
  /** When each terminal was last on a screen; the one seen least recently gives its context up first. */
  private readonly seen = new Map<string, number>();

  constructor(private readonly size: number) {}

  slotFor(
    terminal: Terminal,
    id: string,
    onRenderer: () => void = () => {},
  ): WebglRendererSlot {
    return new WebglRendererSlot(terminal, this, id, onRenderer);
  }

  /** Records that the terminal was on a screen at `time`. */
  markSeen(id: string, time: number): void {
    this.seen.set(id, time);
  }

  /**
   * Gives the slot a context: a free one, or the context of the terminal seen least recently, when this one was seen
   * more recently. Returns false when every holder was seen at least as recently, so the slot waits.
   */
  claim(slot: WebglRendererSlot): boolean {
    if (this.holders.includes(slot)) return true;
    if (this.holders.length < this.size) {
      this.holders.push(slot);
      return true;
    }
    // A context is never taken from a terminal on a screen now: it is painted, and the claimer waits on the DOM view.
    const hidden = this.holders.filter((holder) => !holder.visible);
    if (hidden.length === 0) return false;
    const oldest = hidden.reduce((least, holder) =>
      this.lastSeen(holder.id) < this.lastSeen(least.id) ? holder : least,
    );
    if (this.lastSeen(slot.id) <= this.lastSeen(oldest.id)) return false;
    oldest.release();
    this.forget(oldest);
    this.holders.push(slot);
    return true;
  }

  forget(slot: WebglRendererSlot): void {
    const index = this.holders.indexOf(slot);
    if (index !== -1) this.holders.splice(index, 1);
  }

  /** Drops the last-seen time of a terminal that is gone, so the map does not grow with every session. */
  forgetSeen(id: string): void {
    this.seen.delete(id);
  }

  /** How many terminals hold a WebGL context now: for measuring the budget. */
  holding(): number {
    return this.holders.length;
  }

  private lastSeen(id: string): number {
    return this.seen.get(id) ?? 0;
  }
}

/** The private path to the cursor blink that the workaround below reads. */
interface AddonInternals {
  _renderer?: { _cursorBlinkStateManager?: { clear?: () => void } };
}

/**
 * shortcut: xterm 0.19 leaves the cursor blink running after dispose (fixed upstream in 0.20, xterm.js PR #5817), so
 * every disposed addon kept a 600 ms interval alive. Stop it first; remove this and the private access once
 * @xterm/addon-webgl 0.20 is stable.
 */
function disposeAddon(addon: WebglAddon): void {
  const internals = addon as unknown as AddonInternals;
  internals._renderer?._cursorBlinkStateManager?.clear?.();
  addon.dispose();
}

/** The WebGL renderer of one terminal, drawn only while the budget gives it a context. */
export class WebglRendererSlot {
  /** Whether the terminal is on a screen now. Set by the view; a visible terminal keeps its context. */
  visible = false;
  private state: "none" | "loading" | "attached" = "none";
  private addon: WebglAddon | null = null;
  private disposed = false;

  constructor(
    private readonly terminal: Terminal,
    private readonly budget: WebglBudget,
    readonly id: string,
    /** Called when the renderer changes: the DOM and WebGL renderers measure a cell differently, so the size must be refit. */
    private readonly onRenderer: () => void,
  ) {}

  /** Asks for a context. A terminal that holds one keeps it; otherwise it takes one when the budget gives it one. */
  async show(): Promise<void> {
    if (this.disposed) return;
    if (this.state !== "none") {
      this.budget.claim(this);
      return;
    }
    if (!this.budget.claim(this)) return;
    this.state = "loading";

    // Loaded on first use, so the startup bundle does not carry the addon.
    const { WebglAddon: Addon } = await import("@xterm/addon-webgl");
    // Released while the module was loading, so there is nothing to attach.
    if (this.state !== "loading") return;

    let addon: WebglAddon | undefined;
    try {
      addon = new Addon();
      this.terminal.loadAddon(addon);
    } catch {
      if (addon) disposeAddon(addon);
      this.forget();
      return;
    }
    // Browsers drop contexts under memory pressure: forget the addon, keep the DOM renderer.
    const loaded = addon;
    loaded.onContextLoss(() => {
      disposeAddon(loaded);
      if (this.addon !== loaded) return;
      this.addon = null;
      this.forget();
      this.onRenderer();
    });
    this.addon = loaded;
    this.state = "attached";
    this.onRenderer();
  }

  /** Whether the terminal holds its context now: a context it holds comes back without waiting its turn. */
  get attached(): boolean {
    return this.state === "attached";
  }

  setVisible(visible: boolean): void {
    this.visible = visible;
    if (visible) this.budget.markSeen(this.id, performance.now());
  }

  /** Gives the context back: the terminal falls back to the DOM renderer. */
  release(): void {
    const addon = this.addon;
    this.addon = null;
    this.forget();
    if (!addon) return;
    disposeAddon(addon);
    this.onRenderer();
  }

  dispose(): void {
    this.disposed = true;
    this.release();
    this.budget.forgetSeen(this.id);
  }

  private forget(): void {
    this.state = "none";
    this.budget.forget(this);
  }
}

/** The budget the app uses: WebGL for the six terminals seen on a screen most recently. */
export const webglBudget = new WebglBudget(6);
