import { useState, type KeyboardEvent } from "react";

import { commands } from "@/bindings";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { useBranchSyncStore } from "@/changes/branch-sync-store";
import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { primaryAction } from "@/changes/primary-action";
import { draftOf, useCommitDraftStore } from "@/changes/commit-draft-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

interface CommitAreaProps {
  worktreePath: string;
  /** Files in the index: the commit holds only these. */
  stagedCount: number;
}

/** The message box under the file list. Commit takes what is staged, with what the user wrote. */
export function CommitArea({ worktreePath, stagedCount }: CommitAreaProps) {
  const draft = useCommitDraftStore((state) =>
    draftOf(state.drafts, worktreePath),
  );
  const sync = useBranchSyncStore((state) => state.byWorktree[worktreePath]);
  const action = primaryAction(stagedCount, sync);
  const [committing, setCommitting] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const canCommit =
    stagedCount > 0 &&
    draft.summary.trim() !== "" &&
    !committing &&
    !pushing &&
    !generating;
  const canGenerate = stagedCount > 0 && !committing && !pushing && !generating;

  // Fills the summary and description from the user's agent CLI. Nothing is committed.
  async function generate() {
    if (!canGenerate) return;
    setGenerating(true);
    setError(null);
    setNotice(null);
    try {
      const drafted = await unwrap(commands.draftCommitMessage(worktreePath));
      useCommitDraftStore.getState().update(worktreePath, {
        summary: drafted.summary,
        description: drafted.description,
      });
      setNotice(copy.changes.draftedWith(drafted.agentLabel));
    } catch (caught) {
      setError(`${copy.changes.generateFailed}: ${errorMessage(caught)}`);
    } finally {
      setGenerating(false);
    }
  }

  async function commit() {
    if (!canCommit) return;
    setCommitting(true);
    setError(null);
    setNotice(null);
    try {
      await unwrap(
        commands.commitChanges(worktreePath, draft.summary, draft.description),
      );
      useCommitDraftStore.getState().clear(worktreePath);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setCommitting(false);
      await refreshWorktreeGit(worktreePath);
    }
  }

  // Sends the branch to its remote, publishing it first when the remote has none. Never forces.
  async function push() {
    if (action.kind !== "push" || pushing) return;
    setPushing(true);
    setError(null);
    setNotice(null);
    try {
      await unwrap(commands.pushBranch(worktreePath));
      setNotice(copy.changes.pushed(action.remote));
    } catch (caught) {
      setError(`${copy.changes.pushFailed}: ${errorMessage(caught)}`);
    } finally {
      setPushing(false);
      await refreshWorktreeGit(worktreePath);
    }
  }

  // Ctrl or Cmd with Enter commits from the description, like most commit boxes.
  function onDescriptionKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void commit();
    }
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void (action.kind === "push" ? push() : commit());
      }}
      className="shrink-0 space-y-1.5 border-b border-sidebar-border p-2"
    >
      <div className="relative">
        <Input
          aria-label={copy.changes.summaryLabel}
          placeholder={copy.changes.summaryPlaceholder}
          value={draft.summary}
          className="pr-9"
          onChange={(event) =>
            useCommitDraftStore
              .getState()
              .update(worktreePath, { summary: event.target.value })
          }
        />
        <TooltipIconButton
          label={copy.changes.generate}
          tooltipSide="top"
          variant="outline"
          disabled={!canGenerate}
          aria-busy={generating}
          className="absolute top-1/2 right-1 size-6 -translate-y-1/2 bg-background"
          onClick={() => void generate()}
        >
          {generating ? (
            <Loader2Glyph18 className="animate-spin" />
          ) : (
            <SparkleGlyph18 />
          )}
        </TooltipIconButton>
      </div>
      <Textarea
        aria-label={copy.changes.descriptionLabel}
        placeholder={copy.changes.descriptionPlaceholder}
        rows={2}
        value={draft.description}
        onChange={(event) =>
          useCommitDraftStore
            .getState()
            .update(worktreePath, { description: event.target.value })
        }
        onKeyDown={onDescriptionKeyDown}
      />
      <div className="flex items-center gap-1.5">
        <p
          role="status"
          className="min-w-0 flex-1 truncate text-meta text-muted-foreground"
        >
          {generating
            ? copy.changes.generating
            : (notice ??
              (action.kind === "push"
                ? action.publish
                  ? copy.changes.publishTo(action.remote)
                  : copy.changes.commitsToPush(action.ahead, action.remote)
                : stagedCount === 0
                  ? copy.changes.nothingStaged
                  : null))}
        </p>
        {action.kind === "push" ? (
          <Button type="submit" size="sm" disabled={pushing || committing}>
            {pushing ? copy.changes.pushing : copy.changes.push}
          </Button>
        ) : (
          <Button type="submit" size="sm" disabled={!canCommit}>
            {committing ? copy.changes.committing : copy.changes.commit}
          </Button>
        )}
      </div>
      {error && (
        <p role="alert" className="text-meta break-words text-destructive">
          {error}
        </p>
      )}
    </form>
  );
}
