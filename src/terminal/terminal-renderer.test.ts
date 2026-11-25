import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Terminal } from "@xterm/xterm";

import {
  WebglBudget,
  type WebglRendererSlot,
} from "@/terminal/terminal-renderer";

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
  // A slot for a fake terminal, under its own id; the budget only needs the id, the addon is faked.
  const slotIn = (budget: WebglBudget, id: string): WebglRendererSlot =>
    budget.slotFor(fakeTerminal().terminal, id);

  it("attaches WebGL when a terminal is shown and gives it back when released", async () => {
    const { terminal, loadAddon } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal, "a");

    await slot.show();
    const [addon] = fake.created;
    expect(loadAddon).toHaveBeenCalledWith(addon);
    expect(addon.dispose).not.toHaveBeenCalled();

    slot.release();
    expect(addon.dispose).toHaveBeenCalledOnce();
  });

  it("attaches once however often it is shown", async () => {
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal, "a");

    await slot.show();
    await slot.show();
    expect(fake.created).toHaveLength(1);
  });

  it("keeps a terminal's context while the budget has room, whether or not it is on a screen", async () => {
    const budget = new WebglBudget(6);
    const first = slotIn(budget, "a");
    const second = slotIn(budget, "b");

    await first.show();
    await second.show();

    expect(fake.created[0].dispose).not.toHaveBeenCalled();
  });

  it("gives a context to a terminal seen more recently, taking it from the one seen least recently", async () => {
    const budget = new WebglBudget(2);
    const [a, b, c] = ["a", "b", "c"].map((id) => slotIn(budget, id));
    budget.markSeen("a", 1);
    budget.markSeen("b", 2);
    budget.markSeen("c", 3);

    await a.show();
    await b.show();
    await c.show();

    expect(fake.created[0].dispose).toHaveBeenCalledOnce();
    expect(fake.created[1].dispose).not.toHaveBeenCalled();
    expect(fake.created[2].dispose).not.toHaveBeenCalled();
  });

  it("keeps the context of a terminal that was seen recently, even when it is off the screen now", async () => {
    // Eight terminals open on one canvas: the six seen most recently hold a context. F left the camera earlier and is
    // still among those six, so it keeps its context; A and B, seen least recently, lose theirs.
    const budget = new WebglBudget(6);
    const ids = ["a", "b", "c", "d", "e", "f", "g", "h"];
    const slots = ids.map((id) => slotIn(budget, id));
    ids.forEach((id, index) => budget.markSeen(id, index + 1));

    for (const slot of slots) await slot.show();

    expect(fake.created[0].dispose).toHaveBeenCalledOnce();
    expect(fake.created[1].dispose).toHaveBeenCalledOnce();
    expect(fake.created[5].dispose).not.toHaveBeenCalled();
    expect(budget.holding()).toBe(6);
  });

  it("makes a terminal wait when every holder was seen at least as recently", async () => {
    const budget = new WebglBudget(1);
    const [a, b] = ["a", "b"].map((id) => slotIn(budget, id));
    budget.markSeen("a", 5);
    budget.markSeen("b", 2);

    await a.show();
    await b.show();

    expect(fake.created).toHaveLength(1);
    expect(fake.created[0].dispose).not.toHaveBeenCalled();
  });

  it("attaches a fresh context when an evicted terminal is shown again, once it is seen more recently", async () => {
    const budget = new WebglBudget(1);
    const first = slotIn(budget, "a");
    const second = slotIn(budget, "b");
    budget.markSeen("a", 1);
    budget.markSeen("b", 2);

    await first.show();
    await second.show();
    budget.markSeen("a", 3);
    await first.show();

    expect(fake.created).toHaveLength(3);
    expect(fake.created[2].dispose).not.toHaveBeenCalled();
  });

  it("forgets a lost context, so the next showing attaches a fresh one", async () => {
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal, "a");

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
    const slot = new WebglBudget(6).slotFor(terminal, "a");

    const pending = slot.show();
    slot.release();
    await pending;

    expect(fake.created).toHaveLength(0);
  });

  it("keeps the DOM renderer without throwing when WebGL cannot load", async () => {
    fake.loadError = new Error("WebGL2 not supported");
    const { terminal } = fakeTerminal();
    const slot = new WebglBudget(6).slotFor(terminal, "a");

    await expect(slot.show()).resolves.toBeUndefined();
    expect(fake.created[0].dispose).toHaveBeenCalledOnce();
  });
});
