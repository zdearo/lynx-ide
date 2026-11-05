import type { DeleteImpact, ImpactMember } from "@/bindings";
import { copy } from "@/ui-copy";

type ImpactCopy = typeof copy.bots.details.impact;

/** The lines the delete dialog lists for what deleting a bot takes with it. Both lists are empty when nothing else goes. */
export function deleteImpactLines(
  impact: DeleteImpact,
  text: ImpactCopy,
): { members: string[]; notes: string[] } {
  const notes: string[] = [];
  if (impact.runningWorkers > 0) {
    notes.push(text.runningWorkers(impact.runningWorkers));
  }
  return {
    members: impact.members.map((member) => memberLine(member, text)),
    notes,
  };
}

/** A member's line: its role, then how many members sit under it when there are any. */
function memberLine(member: ImpactMember, text: ImpactCopy): string {
  const parts = [member.role];
  if (member.under > 0) parts.push(text.under(member.under));
  return parts.join(" · ");
}
