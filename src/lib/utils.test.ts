import { describe, expect, it } from "vitest";
import { cn } from "@/lib/utils";

describe("cn", () => {
  it("merges conflicting Tailwind classes, keeping the last one", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
  });

  it("drops falsy values", () => {
    const hidden = false;
    expect(cn("text-sm", hidden && "hidden", undefined)).toBe("text-sm");
  });

  it("keeps the app's font sizes next to a text colour", () => {
    expect(cn("text-ui", "text-muted-foreground")).toBe(
      "text-ui text-muted-foreground",
    );
    expect(cn("text-nested text-muted-foreground")).toBe(
      "text-nested text-muted-foreground",
    );
    expect(cn("text-meta", "text-muted-foreground")).toBe(
      "text-meta text-muted-foreground",
    );
  });

  it("still lets a later font size win over an earlier one", () => {
    expect(cn("text-ui", "text-nested")).toBe("text-nested");
  });
});
