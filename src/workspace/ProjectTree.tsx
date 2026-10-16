import type { WorktreeInfo } from "@/bindings";
import {
  summarizeChildren,
  summarizeRepo,
} from "@/workspace/collapsed-summary";
import { FolderSection } from "@/workspace/FolderSection";
import type { ProjectNode } from "@/workspace/project-tree";
import type { PendingWorktree } from "@/workspace/workspace-store";
import { RepoSection } from "@/workspace/RepoSection";

/** What every repo in the tree needs from the panel, passed down once instead of per level. */
export interface ProjectContext {
  worktrees: Record<string, WorktreeInfo[]>;
  pending: PendingWorktree[];
  collapsedRepoIds: Record<string, boolean>;
  /** Running agent sessions per worktree path, for the summary of collapsed rows. */
  runningAgents: ReadonlyMap<string, number>;
  selectedPath: string | null;
  onSelectWorktree: (path: string) => void;
}

/** Renders one level of the project tree; folders render this same component for their contents. */
export function ProjectTree({
  nodes,
  context,
  depth = 0,
}: {
  nodes: ProjectNode[];
  context: ProjectContext;
  /** 0 at the top of the sidebar; folders at depth 0 render as group labels. */
  depth?: number;
}) {
  return (
    <>
      {nodes.map((node) =>
        node.kind === "folder" ? (
          <FolderSection
            key={`folder-${node.folder.id}`}
            folder={node.folder}
            empty={node.children.length === 0}
            root={depth === 0}
            summary={summarizeChildren(
              node.children,
              context.worktrees,
              context.runningAgents,
            )}
          >
            <ProjectTree
              nodes={node.children}
              context={context}
              depth={depth + 1}
            />
          </FolderSection>
        ) : (
          <RepoSection
            key={`repo-${node.repo.id}`}
            repo={node.repo}
            worktrees={context.worktrees[node.repo.id] ?? []}
            pending={context.pending.filter(
              (item) => item.repoId === node.repo.id,
            )}
            collapsed={context.collapsedRepoIds[node.repo.id] ?? false}
            summary={summarizeRepo(
              context.worktrees[node.repo.id] ?? [],
              context.runningAgents,
            )}
            selectedPath={context.selectedPath}
            onSelectWorktree={context.onSelectWorktree}
          />
        ),
      )}
    </>
  );
}
