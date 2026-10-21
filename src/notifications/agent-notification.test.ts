import { describe, expect, it } from "vitest";

import { agentNotification } from "@/notifications/agent-notification";

const base = {
  previous: "working" as const,
  next: "done" as const,
  config: { onDone: true, onNeedsYou: true, onUpdate: true },
  windowActive: false,
  agentName: "Claude Code",
  place: "app · feat/auth",
};

describe("agentNotification", () => {
  it("says an agent finished, and where", () => {
    expect(agentNotification(base)).toEqual({
      title: "Claude Code finished",
      body: "app · feat/auth",
    });
  });

  it("says an agent needs the user", () => {
    expect(agentNotification({ ...base, next: "needsYou" })?.title).toBe(
      "Claude Code needs you",
    );
  });

  it("stays quiet while the window is in front", () => {
    expect(agentNotification({ ...base, windowActive: true })).toBeNull();
  });

  it("stays quiet for a kind the user turned off, and keeps the other", () => {
    const config = { onDone: false, onNeedsYou: true, onUpdate: true };
    expect(agentNotification({ ...base, config })).toBeNull();
    expect(
      agentNotification({ ...base, config, next: "needsYou" }),
    ).not.toBeNull();
  });

  it("stays quiet for statuses that need no attention, and for repeats", () => {
    expect(agentNotification({ ...base, next: "working" })).toBeNull();
    expect(agentNotification({ ...base, next: "idle" })).toBeNull();
    expect(agentNotification({ ...base, next: null })).toBeNull();
    expect(agentNotification({ ...base, previous: "done" })).toBeNull();
  });
});
