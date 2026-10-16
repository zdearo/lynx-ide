import "@/terminal/terminal-fonts.css";

export const TERMINAL_FONT_SIZE = 13;

// Bundled faces first, then whatever the system has for glyphs they lack.
const BUNDLED_FONT_FAMILY =
  '"JetBrains Mono", "Symbols Nerd Font Mono", ui-monospace, Menlo, Consolas, "Liberation Mono", monospace';

/** The user's fonts come first and the bundled ones still cover whatever they lack. */
export function terminalFontFamily(userFonts: string | null): string {
  return userFonts
    ? `${userFonts}, ${BUNDLED_FONT_FAMILY}`
    : BUNDLED_FONT_FAMILY;
}

let fontsReady: Promise<void> | null = null;

/**
 * xterm measures one character cell when it opens. If the bundled font has not
 * loaded yet it measures a fallback and every row and column count comes out
 * wrong, so terminals wait for the text faces before they are created.
 */
export function terminalFontsReady(): Promise<void> {
  fontsReady ??= Promise.all([
    document.fonts.load(`${TERMINAL_FONT_SIZE}px "JetBrains Mono"`),
    document.fonts.load(`bold ${TERMINAL_FONT_SIZE}px "JetBrains Mono"`),
  ]).then(
    () => undefined,
    // A missing font only costs looks; the terminal must still open.
    () => undefined,
  );
  return fontsReady;
}
