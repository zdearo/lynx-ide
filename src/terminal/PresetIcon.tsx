import { Sparkles, SquareTerminal } from "lucide-react";

import type { TerminalPreset } from "@/terminal/agent-presets";

/** Agents get the warm glyph; a plain shell stays neutral. */
export function PresetIcon({ kind }: { kind: TerminalPreset["kind"] }) {
  return kind === "agent" ? (
    <Sparkles aria-hidden className="text-agent" />
  ) : (
    <SquareTerminal aria-hidden />
  );
}
