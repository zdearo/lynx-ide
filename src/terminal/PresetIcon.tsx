import type { HarnessId } from "@/bindings";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { SquareTerminalGlyph18 } from "@/components/icons/SquareTerminalGlyph18";

/** An agent shows its own mark; a plain shell stays a neutral terminal glyph. */
export function PresetIcon({ harness }: { harness: HarnessId | null }) {
  return harness ? (
    <AgentBrandIcon harness={harness} className="text-agent" />
  ) : (
    <SquareTerminalGlyph18 aria-hidden />
  );
}
