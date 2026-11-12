import { commands, events, type NewTeamMember } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

/** The team's commands. Each one resolves to its value, or rejects with the message the backend gives. */
export const teamBackend = {
  snapshot: () => unwrap(commands.teamSnapshot()),
  /** An agent comes with its worktree and the session started in it; a bot comes with its worktree only. */
  addMember: (member: NewTeamMember) => unwrap(commands.teamAddMember(member)),
  setParent: (memberId: string, parentId: string | null) =>
    unwrap(commands.teamSetParent(memberId, parentId)),
  setOrchestrator: (memberId: string, orchestrator: boolean) =>
    unwrap(commands.teamSetOrchestrator(memberId, orchestrator)),
  setSession: (memberId: string, sessionId: string | null) =>
    unwrap(commands.teamSetSession(memberId, sessionId)),
  /** Removes the member. Returns the terminal session it ran in, so the caller can close it. */
  dismissMember: (memberId: string): Promise<string | null> =>
    unwrap(commands.teamDismissMember(memberId)),
  setLinked: (a: string, b: string, linked: boolean) =>
    unwrap(commands.teamSetLinked(a, b, linked)),
  /** Opens or closes a member's branch in the team tree. The flag comes back with the team. */
  setExpanded: (memberId: string, expanded: boolean) =>
    unwrap(commands.teamSetExpanded(memberId, expanded)),
};

/** Calls `handler` each time the backend changed the team. Returns the function that stops listening. */
export function onTeamChanged(handler: () => void): () => void {
  const listening = events.teamChanged.listen(() => handler());
  return () => void listening.then((unlisten) => unlisten());
}
