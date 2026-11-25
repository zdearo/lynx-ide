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

  slotFor(terminal: Terminal, id: string): WebglRendererSlot {
    return new WebglRendererSlot(terminal, this, id);
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
    const oldest = this.holders.reduce((least, holder) =>
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

  /** How many terminals hold a WebGL context now: for measuring the budget. */
  holding(): number {
    return this.holders.length;
  }

  private lastSeen(id: string): number {
    return this.seen.get(id) ?? 0;
  }
}

/** The WebGL renderer of one terminal, drawn only while the budget gives it a context. */
export class WebglRendererSlot {
  private state: "none" | "loading" | "attached" = "none";
  private addon: WebglAddon | null = null;
  private disposed = false;

  constructor(
    private readonly terminal: Terminal,
    private readonly budget: WebglBudget,
    readonly id: string,
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

/** The budget the app uses: WebGL for the six terminals seen on a screen most recently. */
export const webglBudget = new WebglBudget(6);
