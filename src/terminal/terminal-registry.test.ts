import { describe, expect, it, vi } from "vitest";

import { TerminalHosts } from "@/terminal/terminal-registry";

// Stands in for an element: it knows the slot it is in, and a slot knows its children.
class FakeNode {
  parentNode: FakeSlot | null = null;
  remove() {
    this.parentNode?.children.delete(this);
    this.parentNode = null;
  }
}

class FakeSlot {
  readonly children = new Set<FakeNode>();
  appendChild(node: FakeNode) {
    node.remove();
    this.children.add(node);
    node.parentNode = this;
    return node;
  }
}

function hostsWithTerminals() {
  const hosts = new TerminalHosts<{ dispose(): void }, FakeNode>(
    () => new FakeNode(),
  );
  return hosts;
}

describe("TerminalHosts", () => {
  it("knows a session once it has its entry, so a session shown again is not set up again", () => {
    const hosts = hostsWithTerminals();
    expect(hosts.has("a")).toBe(false);
    hosts.place("a", {}, new FakeSlot());
    expect(hosts.has("a")).toBe(true);
    expect(hosts.has("b")).toBe(false);
  });

  it("makes one element per session and keeps it", () => {
    const hosts = hostsWithTerminals();
    expect(hosts.entryOf("a")).toBe(hosts.entryOf("a"));
    expect(hosts.entryOf("a").host).not.toBe(hosts.entryOf("b").host);
  });

  it("puts a session's element into the view that places it, and takes it away from the view before", () => {
    const hosts = hostsWithTerminals();
    const first = new FakeSlot();
    const second = new FakeSlot();
    const viewA = {};
    const viewB = {};

    const host = hosts.place("a", viewA, first).host;
    expect(first.children.has(host)).toBe(true);

    hosts.place("a", viewB, second);
    expect(first.children.has(host)).toBe(false);
    expect(second.children.has(host)).toBe(true);
    expect(hosts.entryOf("a").holder).toBe(viewB);
  });

  it("lets go of the element without disposing the terminal, and only for the view that holds it", () => {
    const hosts = hostsWithTerminals();
    const slot = new FakeSlot();
    const other = new FakeSlot();
    const viewA = {};
    const viewB = {};
    const dispose = vi.fn();
    const entry = hosts.place("a", viewA, slot);
    entry.value = { dispose };

    expect(hosts.release("a", viewB)).toBe(false);
    expect(slot.children.has(entry.host)).toBe(true);

    hosts.place("a", viewB, other);
    expect(hosts.release("a", viewA)).toBe(false);
    expect(other.children.has(entry.host)).toBe(true);

    expect(hosts.release("a", viewB)).toBe(true);
    expect(other.children.has(entry.host)).toBe(false);
    expect(dispose).not.toHaveBeenCalled();
    expect(hosts.entryOf("a").value).not.toBeNull();
  });

  it("disposes the terminals of the sessions that are gone, and keeps the others", () => {
    const hosts = hostsWithTerminals();
    const slot = new FakeSlot();
    const gone = vi.fn();
    const kept = vi.fn();
    hosts.place("a", {}, slot).value = { dispose: gone };
    hosts.place("b", {}, slot).value = { dispose: kept };

    hosts.prune(new Set(["b"]));

    expect(gone).toHaveBeenCalledOnce();
    expect(kept).not.toHaveBeenCalled();
    expect(slot.children.size).toBe(1);
  });

  it("does nothing when a view lets go of a session it never placed", () => {
    expect(hostsWithTerminals().release("missing", {})).toBe(false);
  });
});
