import { describe, expect, it, vi } from "vitest";

import { createEventHub } from "@/terminal/terminal-event-hub";

function setup() {
  let deliver: (event: {
    payload: { id: string; n: number };
  }) => void = () => {};
  const listen = vi.fn((handler: typeof deliver) => {
    deliver = handler;
    return Promise.resolve(() => {});
  });
  return {
    hub: createEventHub(listen),
    listen,
    send: (id: string, n = 0) => deliver({ payload: { id, n } }),
  };
}

describe("createEventHub", () => {
  it("listens once, however many sessions subscribe", async () => {
    const { hub, listen } = setup();
    await Promise.all(
      ["a", "b", "c"].map((id) => hub.subscribe(id, () => {}).ready),
    );
    expect(listen).toHaveBeenCalledTimes(1);
  });

  it("delivers an event only to the session it belongs to", async () => {
    const { hub, send } = setup();
    const a = vi.fn();
    const b = vi.fn();
    await hub.subscribe("a", a).ready;
    await hub.subscribe("b", b).ready;
    send("a", 1);
    expect(a).toHaveBeenCalledWith({ id: "a", n: 1 });
    expect(b).not.toHaveBeenCalled();
  });

  it("stops delivering after unsubscribe", async () => {
    const { hub, send } = setup();
    const a = vi.fn();
    const subscription = hub.subscribe("a", a);
    await subscription.ready;
    subscription.unsubscribe();
    send("a");
    expect(a).not.toHaveBeenCalled();
  });
});
