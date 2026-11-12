import { useState } from "react";

import { CaretRightIcon } from "@/components/icons/line-icons";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useBranchSyncStore } from "@/changes/branch-sync-store";
import { ChangeMark } from "@/changes/ChangeMark";
import { relativeTime } from "@/changes/commits/commit-time";
import {
  commitFilesKey,
  useCommitFilesStore,
} from "@/changes/commits/commit-files-store";
import { useCommitsStore } from "@/changes/commits/commits-store";
import type { CommitEntry } from "@/bindings";
import { commitFileId } from "@/editor/commit-file";
import { useEditorStore } from "@/editor/editor-store";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

/** The files a commit touched. Clicking one opens its diff against the commit's parent. */
function CommitFiles({
  worktreePath,
  hash,
}: {
  worktreePath: string;
  hash: string;
}) {
  const view = useCommitFilesStore(
    (state) => state.byCommit[commitFilesKey(worktreePath, hash)],
  );
  if (!view || view.status === "loading") {
    return (
      <p className="py-1 pl-5 text-meta text-muted-foreground" aria-busy="true">
        {copy.commits.files.loading}
      </p>
    );
  }
  if (view.status === "error") {
    return (
      <p role="alert" className="py-1 pl-5 text-meta text-destructive">
        {copy.commits.files.failed}
      </p>
    );
  }
  if (view.files.length === 0) {
    return (
      <p className="py-1 pl-5 text-meta text-muted-foreground">
        {copy.commits.files.none}
      </p>
    );
  }
  return (
    <ul
      aria-label={copy.commits.files.list(hash)}
      className="grid grid-cols-1 pb-1 pl-3"
    >
      {view.files.map((file) => {
        const slash = file.path.lastIndexOf("/");
        const name = slash >= 0 ? file.path.slice(slash + 1) : file.path;
        const folder = slash >= 0 ? file.path.slice(0, slash) : "";
        return (
          <li key={file.path}>
            <button
              type="button"
              title={file.path}
              onClick={() =>
                void useEditorStore
                  .getState()
                  .open(worktreePath, commitFileId(hash, file.path), {
                    preview: true,
                    originalPath: file.originalPath,
                    commit: { hash, path: file.path },
                  })
              }
              className="flex h-6 w-full min-w-0 items-center gap-1.5 rounded-md px-1.5 text-left text-ui outline-none hover:bg-sidebar-accent/50 focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <span className="min-w-0 truncate">
                {name}
                {folder && (
                  <span className="ml-1.5 text-meta text-muted-foreground">
                    {folder}
                  </span>
                )}
              </span>
              <span className="ml-auto flex shrink-0 items-center gap-1.5">
                {file.lines &&
                  (file.lines.added > 0 || file.lines.removed > 0) && (
                    <span className="text-meta tabular-nums">
                      {file.lines.added > 0 && (
                        <span className="text-diff-added">
                          +{file.lines.added}
                        </span>
                      )}
                      {file.lines.added > 0 && file.lines.removed > 0 && " "}
                      {file.lines.removed > 0 && (
                        <span className="text-diff-removed">
                          −{file.lines.removed}
                        </span>
                      )}
                    </span>
                  )}
                <ChangeMark kind={file.kind} />
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function CommitRow({
  worktreePath,
  commit,
  readAt,
  hasRemote,
  open,
  onToggle,
}: {
  worktreePath: string;
  commit: CommitEntry;
  readAt: number;
  hasRemote: boolean;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <li
      title={`${commit.hash}\n${commit.subject}`}
      className="min-w-0 rounded-md hover:bg-sidebar-accent/30"
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-full min-w-0 items-start gap-1 rounded-md px-1 py-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <CaretRightIcon
          aria-hidden
          className={cn(
            "mt-0.5 size-3.5 shrink-0 text-muted-foreground transition-transform",
            open && "rotate-90",
          )}
        />
        <span className="grid min-w-0 flex-1 gap-0.5">
          <span className="truncate text-ui">{commit.subject}</span>
          <span className="flex min-w-0 items-center gap-1.5 text-meta text-muted-foreground">
            <span className="shrink-0 font-mono">{commit.shortHash}</span>
            <span className="min-w-0 truncate">
              {commit.author} · {relativeTime(commit.timestamp, readAt)}
            </span>
            {hasRemote && !commit.pushed && (
              <span className="ml-auto shrink-0 font-medium text-diff-modified">
                {copy.commits.notPushed}
              </span>
            )}
          </span>
        </span>
      </button>
      {open && <CommitFiles worktreePath={worktreePath} hash={commit.hash} />}
    </li>
  );
}

/** The commits already made on the branch, newest first, with the ones the remote lacks marked. */
export function CommitList({ worktreePath }: { worktreePath: string }) {
  const read = useCommitsStore((state) => state.byWorktree[worktreePath]);
  // With no remote there is nothing to be "not pushed" to.
  const hasRemote = useBranchSyncStore(
    (state) => state.byWorktree[worktreePath]?.remote != null,
  );
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  if (!read) {
    return (
      <p className="px-3 pt-1 text-ui text-muted-foreground" aria-busy="true">
        {copy.commits.loading}
      </p>
    );
  }
  if (read.commits.length === 0) {
    return (
      <p className="px-3 pt-1 text-ui text-muted-foreground">
        {copy.commits.empty}
      </p>
    );
  }

  function toggle(hash: string) {
    const next = new Set(expanded);
    if (next.has(hash)) next.delete(hash);
    else {
      next.add(hash);
      void useCommitFilesStore.getState().load(worktreePath, hash);
    }
    setExpanded(next);
  }

  return (
    <ScrollArea className="min-h-0 flex-1">
      <ul
        aria-label={copy.commits.list}
        className="grid grid-cols-1 px-1.5 pb-2"
      >
        {read.commits.map((commit) => (
          <CommitRow
            key={commit.hash}
            worktreePath={worktreePath}
            commit={commit}
            readAt={read.readAt}
            hasRemote={hasRemote}
            open={expanded.has(commit.hash)}
            onToggle={() => toggle(commit.hash)}
          />
        ))}
      </ul>
    </ScrollArea>
  );
}
