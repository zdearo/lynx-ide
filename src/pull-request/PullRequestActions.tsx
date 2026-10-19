import { useEffect, useState } from "react";
import { toast } from "sonner";

import {
  commands,
  type MergeMethod,
  type PullRequest,
  type PullRequestAction,
} from "@/bindings";
import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { usePullRequestStore } from "@/pull-request/pull-request-store";
import { copy } from "@/ui-copy";

const text = copy.pullRequest.actions;

/** What can be done to the pull request now: merge, auto-merge, draft state, close or reopen. */
export function PullRequestActions({
  worktreePath,
  pullRequest,
}: {
  worktreePath: string;
  pullRequest: PullRequest;
}) {
  const [methods, setMethods] = useState<MergeMethod[]>([]);
  const [method, setMethod] = useState<MergeMethod | null>(null);
  const [confirmMerge, setConfirmMerge] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isOpen = pullRequest.state === "open";
  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    unwrap(commands.getMergeMethods(worktreePath))
      .then((allowed) => {
        if (cancelled) return;
        setMethods(allowed);
        setMethod((current) => current ?? allowed[0] ?? null);
      })
      .catch(() => {
        // The merge button then offers squash, which most repositories allow; GitHub says if not.
        if (!cancelled) {
          setMethods(["squash"]);
          setMethod((current) => current ?? "squash");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [worktreePath, isOpen]);

  async function run(action: PullRequestAction, done: string) {
    setBusy(true);
    setError(null);
    try {
      await unwrap(
        commands.runPullRequestAction(worktreePath, pullRequest.number, action),
      );
      setConfirmMerge(false);
      toast.success(done);
      await Promise.all([
        usePullRequestStore.getState().refresh(worktreePath),
        refreshWorktreeGit(worktreePath),
      ]);
    } catch (caught) {
      setError(`${text.failed}: ${errorMessage(caught)}`);
    } finally {
      setBusy(false);
    }
  }

  if (pullRequest.state === "merged") return null;

  if (pullRequest.state === "closed") {
    return (
      <div className="space-y-1.5">
        <Button
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={() => void run({ kind: "reopen" }, text.reopened)}
        >
          {text.reopen}
        </Button>
        {error && (
          <p role="alert" className="text-meta break-words text-destructive">
            {error}
          </p>
        )}
      </div>
    );
  }

  const canMerge = !pullRequest.isDraft && method !== null;
  const waiting = pullRequest.mergeState !== "clean";

  return (
    <div className="space-y-2">
      <p className="text-meta text-muted-foreground">
        {text.mergeState[pullRequest.mergeState](pullRequest.baseBranch)}
      </p>

      {pullRequest.isDraft ? (
        <Button
          size="sm"
          disabled={busy}
          onClick={() => void run({ kind: "markReady" }, text.markedReady)}
        >
          {text.markReady}
        </Button>
      ) : (
        <div className="flex items-center gap-1.5">
          <Select
            value={method ?? ""}
            onValueChange={(value) => {
              setMethod(methods.find((name) => name === value) ?? null);
              setConfirmMerge(false);
            }}
            disabled={busy || methods.length === 0}
          >
            <SelectTrigger
              aria-label={text.methodLabel}
              className="min-w-0 flex-1"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {methods.map((name) => (
                <SelectItem key={name} value={name}>
                  {text.methods[name]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            size="sm"
            disabled={busy || !canMerge}
            onClick={() => {
              if (!confirmMerge) {
                setConfirmMerge(true);
                return;
              }
              if (method) {
                void run({ kind: "merge", method, auto: false }, text.merged);
              }
            }}
          >
            {confirmMerge ? text.confirmMerge : text.merge}
          </Button>
        </div>
      )}

      {confirmMerge && (
        <p role="status" className="text-meta break-words text-diff-modified">
          {text.mergeWarning(pullRequest.number, pullRequest.baseBranch)}
        </p>
      )}

      {!pullRequest.isDraft && waiting && (
        <div>
          {pullRequest.autoMerge ? (
            <p className="text-meta text-muted-foreground">
              {text.autoMergeOn}
            </p>
          ) : (
            <Button
              variant="outline"
              size="sm"
              disabled={busy || method === null}
              onClick={() =>
                method &&
                void run(
                  { kind: "merge", method, auto: true },
                  text.autoMergeEnabled,
                )
              }
            >
              {text.autoMerge}
            </Button>
          )}
        </div>
      )}

      <div className="flex flex-wrap gap-1.5">
        {!pullRequest.isDraft && (
          <Button
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => void run({ kind: "markDraft" }, text.markedDraft)}
          >
            {text.markDraft}
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={() => void run({ kind: "close" }, text.closed)}
        >
          {text.close}
        </Button>
      </div>

      {error && (
        <p role="alert" className="text-meta break-words text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
