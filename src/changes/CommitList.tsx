import { ScrollArea } from "@/components/ui/scroll-area";
import { useBranchSyncStore } from "@/changes/branch-sync-store";
import { relativeTime } from "@/changes/commit-time";
import { useCommitsStore } from "@/changes/commits-store";
import { copy } from "@/ui-copy";

/** The commits already made on the branch, newest first, with the ones the remote lacks marked. */
export function CommitList({ worktreePath }: { worktreePath: string }) {
  const read = useCommitsStore((state) => state.byWorktree[worktreePath]);
  // With no remote there is nothing to be "not pushed" to.
  const hasRemote = useBranchSyncStore(
    (state) => state.byWorktree[worktreePath]?.remote != null,
  );

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

  return (
    <ScrollArea className="min-h-0 flex-1">
      <ul
        aria-label={copy.commits.list}
        className="grid grid-cols-1 px-1.5 pb-2"
      >
        {read.commits.map((commit) => (
          <li
            key={commit.hash}
            title={`${commit.hash}\n${commit.subject}`}
            className="grid min-w-0 gap-0.5 rounded-md px-1.5 py-1 hover:bg-sidebar-accent/50"
          >
            <span className="truncate text-ui">{commit.subject}</span>
            <span className="flex min-w-0 items-center gap-1.5 text-meta text-muted-foreground">
              <span className="shrink-0 font-mono">{commit.shortHash}</span>
              <span className="min-w-0 truncate">
                {commit.author} · {relativeTime(commit.timestamp, read.readAt)}
              </span>
              {hasRemote && !commit.pushed && (
                <span className="ml-auto shrink-0 font-medium text-diff-modified">
                  {copy.commits.notPushed}
                </span>
              )}
            </span>
          </li>
        ))}
      </ul>
    </ScrollArea>
  );
}
