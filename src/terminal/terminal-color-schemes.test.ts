import { describe, expect, it } from "vitest";

import type { ColorScheme, TerminalConfig } from "@/bindings";
import { DEFAULT_TERMINAL_CONFIG } from "@/config/config-store";
import { resolveTerminalTheme } from "@/terminal/terminal-color-schemes";

const THEMES: ColorScheme[] = [
  "default",
  "light",
  "solarizedDark",
  "solarizedLight",
  "dracula",
  "nord",
  "catppuccinLatte",
  "catppuccinFrappe",
  "catppuccinMacchiato",
  "catppuccinMocha",
  "gruvboxDark",
  "oneDark",
  "tokyoNight",
  "monokai",
];

function config(overrides: Partial<TerminalConfig>): TerminalConfig {
  return { ...DEFAULT_TERMINAL_CONFIG, ...overrides };
}

describe("resolveTerminalTheme", () => {
  it("keeps the colours the default theme has always used", () => {
    const theme = resolveTerminalTheme(config({}));
    expect(theme.background).toBe("#0a0a0a");
    expect(theme.foreground).toBe("#fafafa");
    expect(theme.red).toBe("#cc0000");
  });

  it("takes the background from the theme and the text palette from the same theme by default", () => {
    const theme = resolveTerminalTheme(config({ colorScheme: "dracula" }));
    expect(theme.background).toBe("#282a36");
    expect(theme.foreground).toBe("#f8f8f2");
    expect(theme.red).toBe("#ff5555");
  });

  it("uses the whole text palette of another theme while keeping the background", () => {
    const theme = resolveTerminalTheme(
      config({ colorScheme: "light", textColorScheme: "dracula" }),
    );
    expect(theme.background).toBe("#fafafa");
    expect(theme.foreground).toBe("#f8f8f2");
    expect(theme.red).toBe("#ff5555");
    expect(theme.brightBlue).toBe("#d6acff");
  });

  it("keeps the cursor on the text colour of the text palette", () => {
    const theme = resolveTerminalTheme(
      config({ colorScheme: "nord", textColorScheme: "catppuccinMocha" }),
    );
    expect(theme.cursor).toBe("#cdd6f4");
  });

  it("uses dark scrollbars on light backgrounds and light ones on dark backgrounds", () => {
    expect(
      resolveTerminalTheme(config({ colorScheme: "light" }))
        .scrollbarSliderBackground,
    ).toMatch(/^rgba\(0, 0, 0/);
    expect(
      resolveTerminalTheme(
        config({ colorScheme: "nord", textColorScheme: "light" }),
      ).scrollbarSliderBackground,
    ).toMatch(/^rgba\(255, 255, 255/);
  });

  it("gives every theme a background, a text colour and a red", () => {
    for (const colorScheme of THEMES) {
      const theme = resolveTerminalTheme(config({ colorScheme }));
      expect(theme.background, colorScheme).toMatch(/^#[0-9a-f]{6}$/i);
      expect(theme.foreground, colorScheme).toMatch(/^#[0-9a-f]{6}$/i);
      expect(theme.red, colorScheme).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});
