import { commands, events, type NewTeamMember } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

/** The team's commands. Each one resolves to its value, or rejects with the message the backend gives. */
export const teamBackend = {
  snapshot: () => unwrap(commands.teamSnapshot()),
  createRun: (title: string) => unwrap(commands.teamCreateRun(title, null)),
  deleteRun: (runId: string) => unwrap(commands.teamDeleteRun(runId)),
  addMember: (runId: string, member: NewTeamMember) =>
    unwrap(commands.teamAddMember(runId, member)),
  setParent: (memberId: string, parentId: string | null) =>
    unwrap(commands.teamSetParent(memberId, parentId)),
  setOrchestrator: (memberId: string, orchestrator: boolean) =>
    unwrap(commands.teamSetOrchestrator(memberId, orchestrator)),
  setRole: (memberId: string, role: string) =>
    unwrap(commands.teamSetRole(memberId, role)),
  setTask: (memberId: string, task: string, acceptance: string) =>
    unwrap(commands.teamSetTask(memberId, task, acceptance)),
  dismissMember: (memberId: string) =>
    unwrap(commands.teamDismissMember(memberId)),
  setLinked: (a: string, b: string, linked: boolean) =>
    unwrap(commands.teamSetLinked(a, b, linked)),
};

/** Calls `handler` each time the backend changed the team. Returns the function that stops listening. */
export function onTeamChanged(handler: () => void): () => void {
  const listening = events.teamChanged.listen(() => handler());
  return () => void listening.then((unlisten) => unlisten());
}
