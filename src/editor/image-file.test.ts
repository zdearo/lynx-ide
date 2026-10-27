import { describe, expect, it } from "vitest";

import { isImagePath } from "@/editor/image-file";

describe("isImagePath", () => {
  it("accepts raster formats in any case", () => {
    expect(isImagePath("assets/logo.png")).toBe(true);
    expect(isImagePath("photo.JPEG")).toBe(true);
  });

  it("leaves svg and other files to the text editor", () => {
    expect(isImagePath("logo.svg")).toBe(false);
    expect(isImagePath("png")).toBe(false);
    expect(isImagePath("Makefile")).toBe(false);
  });
});
