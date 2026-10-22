import { describe, expect, it } from "vitest";

import { daemonSummary } from "@/status-bar/daemon-status-summary";

describe("daemonSummary", () => {
  it("shows nothing while the setting is off", () => {
    expect(
      daemonSummary({ enabled: false, state: "connected", sessions: 3 }),
    ).toBeNull();
  });

  it("says when the process has not been needed yet", () => {
    expect(daemonSummary({ enabled: true, state: "idle", sessions: 0 })).toBe(
      "Terminal process: starts with the first terminal",
    );
  });

  it("counts the sessions it holds", () => {
    expect(
      daemonSummary({ enabled: true, state: "connected", sessions: 1 }),
    ).toBe("Terminal process: 1 session");
    expect(
      daemonSummary({ enabled: true, state: "connected", sessions: 4 }),
    ).toBe("Terminal process: 4 sessions");
  });

  it("says when terminals fell back to running inside Lynx", () => {
    expect(
      daemonSummary({ enabled: true, state: "degraded", sessions: 0 }),
    ).toBe("Terminal process unavailable: terminals run inside Lynx");
  });
});
