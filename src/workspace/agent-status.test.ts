import { describe, expect, it } from "vitest";

import { agentStatusText } from "@/workspace/agent-status";

describe("agentStatusText", () => {
  it("names each state of a running agent", () => {
    expect(agentStatusText(true, "working", null)).toBe("working");
    expect(agentStatusText(true, "needsYou", null)).toBe("needs you");
    expect(agentStatusText(true, "done", null)).toBe("done");
    expect(agentStatusText(true, "idle", null)).toBe("idle");
  });

  it("treats a running agent that has not reported yet as idle", () => {
    expect(agentStatusText(true, null, null)).toBe("idle");
  });

  it("says how an agent that exited ended", () => {
    expect(agentStatusText(false, "working", 0)).toBe("exited with code 0");
    expect(agentStatusText(false, null, null)).toBe("exited");
  });
});
