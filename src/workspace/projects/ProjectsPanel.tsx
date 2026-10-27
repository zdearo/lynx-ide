import { useMemo, type KeyboardEvent } from "react";
import { Folder2Glyph18 } from "@/components/icons/Folder2Glyph18";
import { FolderPlusGlyph18 } from "@/components/icons/FolderPlusGlyph18";

import {
  SegmentedControl,
  type SegmentOption,
} from "@/components/SegmentedControl";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useDialogStore } from "@/dialog-store";
import { TeamModeSwitch } from "@/team/TeamModeSwitch";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { agentsByWorktree } from "@/workspace/projects/collapsed-summary";
import {
  activeRepoIds,
  onlyActive,
  type ProjectFilter,
} from "@/workspace/projects/project-filter";
import { useProjectFilterStore } from "@/workspace/projects/project-filter-store";
import { buildProjectTree } from "@/workspace/projects/project-tree";
import { ProjectTree } from "@/workspace/projects/ProjectTree";
import { useWorkspaceStore } from "@/workspace/workspace-store";

const FILTER_OPTIONS: readonly SegmentOption<ProjectFilter>[] = [
  { value: "all", label: copy.sidebar.filterAll },
  { value: "active", label: copy.sidebar.filterActive },
];

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
  const filter = useProjectFilterStore((state) => state.filter);
  const shown = useMemo(() => {
    const nodes = buildProjectTree(folders, repos);
    return filter === "active"
      ? onlyActive(nodes, activeRepoIds(repos, worktrees, sessions))
      : nodes;
  }, [folders, repos, worktrees, sessions, filter]);

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
      {!empty && (
        <div className="flex items-center justify-between gap-2 px-2 pb-2">
          <TeamModeSwitch path={selectedPath} />
          <SegmentedControl
            label={copy.sidebar.filterLabel}
            options={FILTER_OPTIONS}
            value={filter}
            onChange={(next) =>
              useProjectFilterStore.getState().setFilter(next)
            }
          />
        </div>
      )}

      {empty ? (
        <div className="px-3 pt-1">
          <p className="text-ui text-sidebar-foreground">
            {copy.sidebar.emptyTitle}
          </p>
          <p className="mt-1 text-meta text-muted-foreground">
            {copy.sidebar.emptyBody}
          </p>
        </div>
      ) : shown.length === 0 ? (
        <ScrollArea className="min-h-0 flex-1">
          <p className="px-3 text-ui text-muted-foreground">
            {copy.sidebar.activeEmpty}
          </p>
        </ScrollArea>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <ul className="px-2 pb-2" onKeyDown={moveFocus}>
            <ProjectTree
              nodes={shown}
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
