import { describe, expect, it } from "vitest";

import {
  describeEngineProgress,
  engineFraction,
} from "@/browser/engine-progress";

describe("describeEngineProgress", () => {
  it("says it is preparing before the first report", () => {
    expect(describeEngineProgress(null)).toBe("Preparing…");
  });

  it("names the megabytes while downloading", () => {
    expect(
      describeEngineProgress({
        phase: "downloading",
        received: 45_400_000,
        total: 139_480_956,
      }),
    ).toBe("Downloading 45 MB of 139 MB");
  });

  it("names the later steps", () => {
    const at = (phase: "verifying" | "unpacking" | "done") => ({
      phase,
      received: 1,
      total: 1,
    });
    expect(describeEngineProgress(at("verifying"))).toBe(
      "Checking the download…",
    );
    expect(describeEngineProgress(at("unpacking"))).toBe("Unpacking…");
    expect(describeEngineProgress(at("done"))).toBe("Installed");
  });
});

describe("engineFraction", () => {
  it("is the share downloaded, capped at one", () => {
    expect(
      engineFraction({ phase: "downloading", received: 50, total: 200 }),
    ).toBe(0.25);
    expect(
      engineFraction({ phase: "downloading", received: 300, total: 200 }),
    ).toBe(1);
  });

  it("cannot be told when the size is unknown or the step has no length", () => {
    expect(engineFraction(null)).toBeNull();
    expect(
      engineFraction({ phase: "downloading", received: 5, total: 0 }),
    ).toBeNull();
    expect(
      engineFraction({ phase: "unpacking", received: 0, total: 0 }),
    ).toBeNull();
  });
});
