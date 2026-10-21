import { describe, expect, it } from "vitest";

import type { NotificationsConfig, UpdateStatus } from "@/bindings";
import { updateAnnouncement } from "@/updates/update-notification";
import { copy } from "@/ui-copy";

const ON: NotificationsConfig = {
  onDone: true,
  onNeedsYou: true,
  onUpdate: true,
};

function status(phase: UpdateStatus["phase"]): UpdateStatus {
  return { currentVersion: "0.1.0", checkedAt: null, phase };
}

const available = status({
  kind: "available",
  version: "0.1.1",
  notes: null,
  date: null,
});

describe("updateAnnouncement", () => {
  it("announces a new version in the app and in the system when the window is not in front", () => {
    const result = updateAnnouncement({
      status: available,
      announced: null,
      config: ON,
      windowActive: false,
    });

    expect(result?.version).toBe("0.1.1");
    expect(result?.system).toEqual({
      title: copy.notifications.updateAvailable("0.1.1"),
      body: copy.notifications.updateBody,
    });
  });

  it("keeps the system quiet while the window is in front", () => {
    const result = updateAnnouncement({
      status: available,
      announced: null,
      config: ON,
      windowActive: true,
    });

    expect(result?.version).toBe("0.1.1");
    expect(result?.system).toBeNull();
  });

  it("leaves the system notice off when that kind is turned off", () => {
    const result = updateAnnouncement({
      status: available,
      announced: null,
      config: { ...ON, onUpdate: false },
      windowActive: false,
    });

    expect(result?.version).toBe("0.1.1");
    expect(result?.system).toBeNull();
  });

  it("says nothing about a version already announced", () => {
    expect(
      updateAnnouncement({
        status: available,
        announced: "0.1.1",
        config: ON,
        windowActive: false,
      }),
    ).toBeNull();
  });

  it("says nothing unless an update is available", () => {
    for (const phase of [
      { kind: "idle" },
      { kind: "checking" },
      { kind: "upToDate" },
      { kind: "failed" },
    ] as const) {
      expect(
        updateAnnouncement({
          status: status(phase),
          announced: null,
          config: ON,
          windowActive: false,
        }),
      ).toBeNull();
    }
  });

  it("announces the next version even after an earlier one", () => {
    const next = status({
      kind: "available",
      version: "0.1.2",
      notes: null,
      date: null,
    });

    expect(
      updateAnnouncement({
        status: next,
        announced: "0.1.1",
        config: ON,
        windowActive: false,
      })?.version,
    ).toBe("0.1.2");
  });
});
