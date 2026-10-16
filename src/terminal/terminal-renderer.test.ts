import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Terminal } from "@xterm/xterm";

import { WebglBudget } from "@/terminal/terminal-renderer";

// Stands in for the GPU addon: it records each instance and can fire context loss.
const fake = vi.hoisted(() => ({
  loadError: null as Error | null,
  created: [] as Array<{ dispose: () => void; loseContext: () => void }>,
}));

vi.mock("@xterm/addon-webgl", () => ({
  WebglAddon: class {
    private readonly listeners: Array<() => void> = [];
    readonly dispose = vi.fn();
    readonly onContextLoss = (listener: () => void) => {
      this.listeners.push(listener);
    };
    loseContext() {
      this.listeners.forEach((listener) => listener());
    }
    constructor() {
      fake.created.push(this);
    }
  },
}));

function fakeTerminal() {
  const loadAddon = vi.fn(() => {
    if (fake.loadError) throw fake.loadError;
  });
  return { terminal: { loadAddon } as unknown as Terminal, loadAddon };
}

beforeEach(() => {
  fake.created.length = 0;
  fake.loadError = null;
});

describe("WebglBudget", () => {
  it("attaches WebGL when a terminal is shown and gives it back when released", async () => {
    const { terminal, loadAddon } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal);

    await slot.show();
    const [addon] = fake.created;
    expect(loadAddon).toHaveBeenCalledWith(addon);
    expect(addon.dispose).not.toHaveBeenCalled();

    slot.release();
    expect(addon.dispose).toHaveBeenCalledOnce();
  });

  it("attaches once however often it is shown", async () => {
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal);

    await slot.show();
    await slot.show();
    expect(fake.created).toHaveLength(1);
  });

  it("keeps a terminal's context while it is hidden and the budget has room", async () => {
    const budget = new WebglBudget(6);
    const first = budget.slotFor(fakeTerminal().terminal);
    const second = budget.slotFor(fakeTerminal().terminal);

    await first.show();
    await second.show();

    expect(fake.created[0].dispose).not.toHaveBeenCalled();
  });

  it("releases the least recently shown terminal when the budget is exceeded", async () => {
    const budget = new WebglBudget(2);
    const slots = [1, 2, 3].map(() => budget.slotFor(fakeTerminal().terminal));

    await slots[0].show();
    await slots[1].show();
    await slots[2].show();

    expect(fake.created[0].dispose).toHaveBeenCalledOnce();
    expect(fake.created[1].dispose).not.toHaveBeenCalled();
    expect(fake.created[2].dispose).not.toHaveBeenCalled();
  });

  it("attaches a fresh context when an evicted terminal is shown again", async () => {
    const budget = new WebglBudget(1);
    const first = budget.slotFor(fakeTerminal().terminal);
    const second = budget.slotFor(fakeTerminal().terminal);

    await first.show();
    await second.show();
    await first.show();

    expect(fake.created).toHaveLength(3);
    expect(fake.created[2].dispose).not.toHaveBeenCalled();
  });

  it("forgets a lost context, so the next showing attaches a fresh one", async () => {
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal);

    await slot.show();
    const [lost] = fake.created;
    lost.loseContext();
    expect(lost.dispose).toHaveBeenCalledOnce();

    await slot.show();
    expect(fake.created).toHaveLength(2);
    expect(fake.created[1].dispose).not.toHaveBeenCalled();
  });

  it("does not leave an addon attached when released while it was loading", async () => {
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal);

    const pending = slot.show();
    slot.release();
    await pending;

    expect(fake.created).toHaveLength(0);
  });

  it("keeps the DOM renderer without throwing when WebGL cannot load", async () => {
    fake.loadError = new Error("WebGL2 not supported");
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal);

    await expect(slot.show()).resolves.toBeUndefined();
    expect(fake.created[0].dispose).toHaveBeenCalledOnce();
  });
});
