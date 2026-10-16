import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Terminal } from "@xterm/xterm";

import { loadWebglRenderer } from "@/terminal/terminal-renderer";

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

describe("loadWebglRenderer", () => {
  it("loads WebGL and disposes it when the browser drops the context", () => {
    const { terminal, loadAddon } = fakeTerminal();

    loadWebglRenderer(terminal);

    const [addon] = fake.created;
    expect(loadAddon).toHaveBeenCalledWith(addon);
    expect(addon.dispose).not.toHaveBeenCalled();

    addon.loseContext();
    expect(addon.dispose).toHaveBeenCalledOnce();
  });

  it("keeps the DOM renderer without throwing when WebGL cannot load", () => {
    fake.loadError = new Error("WebGL2 not supported");
    const { terminal } = fakeTerminal();

    expect(() => loadWebglRenderer(terminal)).not.toThrow();
    expect(fake.created[0].dispose).toHaveBeenCalledOnce();
  });
});
