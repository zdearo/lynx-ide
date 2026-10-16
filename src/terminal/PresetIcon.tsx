import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { SquareTerminalGlyph18 } from "@/components/icons/SquareTerminalGlyph18";

import type { TerminalPreset } from "@/terminal/agent-presets";

/** Agents get the warm glyph; a plain shell stays neutral. */
export function PresetIcon({ kind }: { kind: TerminalPreset["kind"] }) {
  return kind === "agent" ? (
    <SparkleGlyph18 aria-hidden className="text-agent" />
  ) : (
    <SquareTerminalGlyph18 aria-hidden />
  );
}
