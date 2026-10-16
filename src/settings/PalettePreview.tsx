import type { ResolvedTheme } from "@/terminal/terminal-color-schemes";
import { previewColours } from "@/terminal/terminal-color-schemes";

/** The colours the chosen scheme gives text and a prompt, shown before any terminal is open. */
export function PalettePreview({ theme }: { theme: ResolvedTheme }) {
  return (
    <div
      aria-hidden
      className="flex items-center gap-1.5 rounded-md border border-border p-3"
      style={{ backgroundColor: theme.background }}
    >
      <span
        className="size-4 rounded-sm border border-border"
        style={{ backgroundColor: theme.foreground }}
      />
      {previewColours(theme).map((colour) => (
        <span
          key={colour}
          className="size-4 rounded-sm"
          style={{ backgroundColor: colour }}
        />
      ))}
    </div>
  );
}
