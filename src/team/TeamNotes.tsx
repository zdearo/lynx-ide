import { useMemo } from "react";

import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** The notes the team shares in the worktree on the sidebar, by name. Nothing is shown when there are none. */
export function TeamNotes() {
  const notes = useTeamStore((state) => state.notes);
  const worktree = useWorkspaceStore((state) => state.selectedWorktreePath);
  const shown = useMemo(
    () =>
      notes
        .filter((note) => note.worktree === worktree)
        .sort((first, second) => first.name.localeCompare(second.name)),
    [notes, worktree],
  );
  if (shown.length === 0) return null;

  return (
    <section
      aria-label={copy.team.teamNotes.title}
      className="max-h-56 shrink-0 overflow-y-auto border-t px-3 py-2"
    >
      <h2 className="text-meta font-medium text-muted-foreground">
        {copy.team.teamNotes.title} · {copy.team.teamNotes.count(shown.length)}
      </h2>
      <ul className="mt-1.5 grid grid-cols-1 gap-2">
        {shown.map((note) => (
          <li key={note.name} className="grid grid-cols-1 gap-0.5">
            <h3 className="truncate text-nested font-medium">{note.name}</h3>
            <p className="whitespace-pre-wrap break-words text-nested">
              {note.body}
            </p>
          </li>
        ))}
      </ul>
    </section>
  );
}
