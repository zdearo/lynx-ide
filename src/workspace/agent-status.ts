import type { AgentStatus } from "@/bindings";
import { copy } from "@/ui-copy";

/** What an agent row says about its session, for sighted users and screen readers alike. */
export function agentStatusText(
  running: boolean,
  status: AgentStatus | null,
  exitCode: number | null,
): string {
  if (!running) return copy.agent.exited(exitCode);
  return copy.agent.status[status ?? "idle"];
}
