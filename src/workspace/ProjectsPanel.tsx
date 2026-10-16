import { useMemo, type KeyboardEvent } from "react";
import { Folder2Glyph18 } from "@/components/icons/Folder2Glyph18";
import { FolderPlusGlyph18 } from "@/components/icons/FolderPlusGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useDialogStore } from "@/dialog-store";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { agentsByWorktree } from "@/workspace/collapsed-summary";
import { buildProjectTree } from "@/workspace/project-tree";
import { ProjectTree } from "@/workspace/ProjectTree";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Moves focus between folder headers, repo headers and worktree rows with the arrow keys. */
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
  const folders = useWorkspaceStore((state) => state.folders);
  const worktrees = useWorkspaceStore((state) => state.worktrees);
  const pending = useWorkspaceStore((state) => state.pending);
  const collapsedRepoIds = useWorkspaceStore((state) => state.collapsedRepoIds);
  const selectedPath = useWorkspaceStore((state) => state.selectedWorktreePath);
  const selectWorktree = useWorkspaceStore((state) => state.selectWorktree);
  const askFolderDialog = useDialogStore((state) => state.askFolderDialog);
  const askAddProject = useDialogStore((state) => state.askAddProject);
  const sessions = useTerminalStore((state) => state.sessions);
  const agents = useMemo(() => agentsByWorktree(sessions), [sessions]);

  const empty = repos.length === 0 && folders.length === 0;

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div className="flex h-9 shrink-0 items-center justify-between pr-2 pl-3">
        <h2 className="text-xs font-medium text-muted-foreground">
          {copy.sidebar.title}
        </h2>
        <div className="flex items-center">
          <TooltipIconButton
            label={copy.folders.newFolder}
            onClick={() => askFolderDialog({ mode: "create", parentId: null })}
          >
            <FolderPlusGlyph18 />
          </TooltipIconButton>
          <TooltipIconButton
            label={copy.sidebar.addProject}
            onClick={() => askAddProject(true)}
          >
            <Folder2Glyph18 />
          </TooltipIconButton>
        </div>
      </div>

      {empty ? (
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
            <ProjectTree
              nodes={buildProjectTree(folders, repos)}
              context={{
                worktrees,
                pending,
                collapsedRepoIds,
                agents,
                selectedPath,
                onSelectWorktree: (path) => {
                  selectWorktree(path);
                  onNavigate?.();
                },
              }}
            />
          </ul>
        </ScrollArea>
      )}
    </div>
  );
}
