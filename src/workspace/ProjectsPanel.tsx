import type { KeyboardEvent } from "react";
import { FolderPlus } from "lucide-react";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ScrollArea } from "@/components/ui/scroll-area";
import { copy } from "@/ui-copy";
import { openRepositoryFromPicker } from "@/workspace/open-repository";
import { RepoSection } from "@/workspace/RepoSection";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Moves focus between repo headers and worktree rows with the arrow keys. */
function moveFocus(event: KeyboardEvent<HTMLElement>) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const items = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>("[data-nav-item]"),
  );
  const current = items.indexOf(document.activeElement as HTMLElement);
  if (current === -1) return;

  event.preventDefault();
  const next = event.key === "ArrowDown" ? current + 1 : current - 1;
  items[Math.max(0, Math.min(items.length - 1, next))]?.focus();
}

export function ProjectsPanel({ onNavigate }: { onNavigate?: () => void }) {
  const repos = useWorkspaceStore((state) => state.repos);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const pending = useWorkspaceStore((state) => state.pending);
  const collapsedRepoIds = useWorkspaceStore((state) => state.collapsedRepoIds);
  const selectedPath = useWorkspaceStore((state) => state.selectedWorktreePath);
  const selectWorktree = useWorkspaceStore((state) => state.selectWorktree);

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div className="flex h-9 shrink-0 items-center justify-between pr-2 pl-3">
        <h2 className="text-xs font-medium text-muted-foreground">
          {copy.sidebar.title}
        </h2>
        <TooltipIconButton
          label={copy.sidebar.openRepository}
          onClick={() => void openRepositoryFromPicker()}
        >
          <FolderPlus />
        </TooltipIconButton>
      </div>

      {repos.length === 0 ? (
        <div className="px-3 pt-1">
          <p className="text-ui text-sidebar-foreground">
            {copy.sidebar.emptyTitle}
          </p>
          <p className="mt-1 text-meta text-muted-foreground">
            {copy.sidebar.emptyBody}
          </p>
        </div>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <ul className="px-2 pb-2" onKeyDown={moveFocus}>
            {repos.map((repo) => (
              <RepoSection
                key={repo.id}
                repo={repo}
                worktrees={worktrees[repo.id] ?? []}
                pending={pending.filter((item) => item.repoId === repo.id)}
                collapsed={collapsedRepoIds[repo.id] ?? false}
                selectedPath={selectedPath}
                onSelectWorktree={(path) => {
                  selectWorktree(path);
                  onNavigate?.();
                }}
              />
            ))}
          </ul>
        </ScrollArea>
      )}
    </div>
  );
}
