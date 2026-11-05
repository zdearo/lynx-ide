import { describe, expect, it } from "vitest";

import type { DeleteImpact } from "@/bindings";
import { deleteImpactLines } from "@/bots/bot-delete-impact";
import { copy } from "@/ui-copy";

const text = copy.bots.details.impact;
const none: DeleteImpact = { members: [], runningWorkers: 0 };

describe("deleteImpactLines", () => {
  it("lists nothing when nothing else goes with the bot", () => {
    expect(deleteImpactLines(none, text)).toEqual({ members: [], notes: [] });
  });

  it("describes each member by its role and the members under it", () => {
    const lines = deleteImpactLines(
      {
        ...none,
        members: [
          { role: "Reviewer", under: 2 },
          { role: "Scout", under: 0 },
        ],
      },
      text,
    );

    expect(lines.members).toEqual(["Reviewer · +2 under it", "Scout"]);
  });

  it.each([
    [1, "1 running worker will be stopped"],
    [3, "3 running workers will be stopped"],
  ])("says how many running workers stop (%i)", (count, line) => {
    expect(
      deleteImpactLines({ ...none, runningWorkers: count }, text).notes,
    ).toEqual([line]);
  });
});
