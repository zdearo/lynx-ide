import { useState, type KeyboardEvent } from "react";

import { commitFailurePrompt } from "@/agents/fix-prompts";
import { fixWithAgent, useHasFixAgent } from "@/agents/fix-with-agent";
import { commands } from "@/bindings";
import { useChangesStore } from "@/changes/changes-store";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { useBranchSyncStore } from "@/changes/branch-sync-store";
import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { primaryAction } from "@/changes/primary-action";
import { useCommitsStore } from "@/changes/commits/commits-store";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  draftOf,
  useCommitDraftStore,
} from "@/changes/commits/commit-draft-store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

type RemoteAction = Exclude<
  ReturnType<typeof primaryAction>,
  { kind: "commit" }
>;

/** The button's text: the action, what it is doing, or the second click a force push needs. */
function remoteLabel(
  action: RemoteAction,
  working: boolean,
  confirmingForce: boolean,
): string {
  switch (action.kind) {
    case "push":
      return working ? copy.changes.pushing : copy.changes.push;
    case "pull":
      return working ? copy.changes.pulling : copy.changes.pull;
    case "sync":
      return working ? copy.changes.syncing : copy.changes.sync;
    case "forcePush":
      return working
        ? copy.changes.forcePushing
        : confirmingForce
          ? copy.changes.forcePushAnyway
          : copy.changes.forcePush;
  }
}

/** What the status line says about the remote while nothing else is going on. */
function remoteHint(action: ReturnType<typeof primaryAction>): string | null {
  switch (action.kind) {
    case "push":
      return action.publish
        ? copy.changes.publishTo(action.remote)
        : copy.changes.commitsToPush(action.ahead, action.remote);
    case "pull":
      return copy.changes.commitsToPull(action.behind, action.remote);
    case "sync":
      return copy.changes.divergedHint(
        action.ahead,
        action.behind,
        action.remote,
      );
    case "forcePush":
      return copy.changes.forcePushHint(action.remote);
    case "commit":
      return null;
  }
}

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
  const commits = useCommitsStore(
    (state) => state.byWorktree[worktreePath]?.commits,
  );
  // An amend replaces the last commit, so Push never takes its place.
  const action = draft.amend
    ? ({ kind: "commit" } as const)
    : primaryAction(stagedCount, sync);
  const hasCommits = (commits?.length ?? 0) > 0;
  const lastIsPushed = commits?.[0]?.pushed === true && sync?.remote != null;
  const [confirmPushed, setConfirmPushed] = useState(false);
  const [confirmForce, setConfirmForce] = useState(false);
  const [committing, setCommitting] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Set only when the commit itself failed (a hook, usually), so an agent can be asked to fix it.
  const [commitFailed, setCommitFailed] = useState(false);
  const hasFixAgent = useHasFixAgent();
  const [notice, setNotice] = useState<string | null>(null);
  const canCommit =
    (draft.amend || stagedCount > 0) &&
    draft.summary.trim() !== "" &&
    !committing &&
    !pushing &&
    !generating;
  // Only a running operation turns the button off: with nothing staged it still answers a click.
  const busyElsewhere = committing || pushing || generating;

  // Fills the summary and description from the user's agent CLI. Nothing is committed.
  async function generate() {
    if (busyElsewhere) return;
    if (stagedCount === 0) {
      setError(null);
      setNotice(copy.changes.generateNeedsStaged);
      return;
    }
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

  // Turning amend on starts from the last commit's message, unless the user already typed one.
  async function toggleAmend(on: boolean) {
    setConfirmPushed(false);
    useCommitDraftStore.getState().update(worktreePath, { amend: on });
    if (!on || draft.summary.trim() !== "" || draft.description.trim() !== "") {
      return;
    }
    try {
      const last = await unwrap(commands.getLastCommitMessage(worktreePath));
      useCommitDraftStore.getState().update(worktreePath, {
        summary: last.summary,
        description: last.description,
      });
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function commit() {
    if (!canCommit) return;
    // Rewriting a commit the remote has needs a second, deliberate click.
    if (draft.amend && lastIsPushed && !confirmPushed) {
      setConfirmPushed(true);
      return;
    }
    setCommitting(true);
    setError(null);
    setCommitFailed(false);
    setNotice(null);
    try {
      await unwrap(
        commands.commitChanges(
          worktreePath,
          draft.summary,
          draft.description,
          draft.amend,
          confirmPushed,
        ),
      );
      setConfirmPushed(false);
      useCommitDraftStore.getState().clear(worktreePath);
    } catch (caught) {
      setError(errorMessage(caught));
      setCommitFailed(true);
    } finally {
      setCommitting(false);
      await refreshWorktreeGit(worktreePath);
    }
  }

  // Keeps the branch and its remote together. Only a force push needs a second, deliberate click.
  async function remoteAction() {
    if (action.kind === "commit" || pushing) return;
    if (action.kind === "forcePush" && !confirmForce) {
      setConfirmForce(true);
      return;
    }
    setPushing(true);
    setError(null);
    setNotice(null);
    setConfirmForce(false);
    const failed = {
      push: copy.changes.pushFailed,
      pull: copy.changes.pullFailed,
      sync: copy.changes.syncFailed,
      forcePush: copy.changes.forcePushFailed,
    }[action.kind];
    try {
      switch (action.kind) {
        case "push":
          await unwrap(commands.pushBranch(worktreePath));
          setNotice(copy.changes.pushed(action.remote));
          break;
        case "pull":
          await unwrap(commands.pullBranch(worktreePath, "fastForward"));
          setNotice(copy.changes.pulled(action.remote));
          break;
        case "sync":
          // Your commits go on top of the remote's, then up. A conflict stops it and is undone.
          await unwrap(commands.pullBranch(worktreePath, "rebase"));
          await unwrap(commands.pushBranch(worktreePath));
          setNotice(copy.changes.synced(action.remote));
          break;
        case "forcePush":
          await unwrap(commands.forcePushBranch(worktreePath));
          setNotice(copy.changes.forcePushed(action.remote));
          break;
      }
    } catch (caught) {
      setError(`${failed}: ${errorMessage(caught)}`);
    } finally {
      setPushing(false);
      await refreshWorktreeGit(worktreePath);
    }
  }

  function fixCommit() {
    if (error === null) return;
    const files = (
      useChangesStore.getState().byWorktree[worktreePath]?.files ?? []
    )
      .filter((file) => file.staged !== null)
      .map((file) => file.path);
    void fixWithAgent(
      worktreePath,
      commitFailurePrompt({
        error,
        summary: draft.summary,
        description: draft.description,
        files,
      }),
    );
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
        void (action.kind === "commit" ? commit() : remoteAction());
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
          disabled={busyElsewhere}
          aria-busy={generating}
          className={cn(
            "absolute top-1/2 right-1 size-6 -translate-y-1/2",
            stagedCount === 0 && "opacity-60",
          )}
          onClick={() => void generate()}
        >
          <SparkleGlyph18 />
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
      {hasCommits && (
        <div className="flex items-center gap-2">
          <Switch
            id="commit-amend"
            checked={draft.amend}
            onCheckedChange={(on) => void toggleAmend(on)}
            disabled={committing || pushing}
          />
          <Label htmlFor="commit-amend" className="text-ui">
            {copy.changes.amend.label}
          </Label>
        </div>
      )}
      <div className="flex items-center gap-1.5">
        <p
          role="status"
          className="min-w-0 flex-1 truncate text-meta text-muted-foreground"
        >
          {generating
            ? copy.changes.generating
            : (notice ??
              remoteHint(action) ??
              (stagedCount === 0 ? copy.changes.nothingStaged : null))}
        </p>
        {action.kind !== "commit" ? (
          <Button
            type="submit"
            size="sm"
            variant={action.kind === "forcePush" ? "destructive" : "default"}
            disabled={pushing || committing}
          >
            {remoteLabel(action, pushing, confirmForce)}
          </Button>
        ) : (
          <Button type="submit" size="sm" disabled={!canCommit}>
            {committing
              ? copy.changes.committing
              : draft.amend
                ? confirmPushed
                  ? copy.changes.amend.anyway
                  : copy.changes.amend.button
                : copy.changes.commit}
          </Button>
        )}
      </div>
      {confirmForce && action.kind === "forcePush" && (
        <p role="status" className="text-meta break-words text-diff-modified">
          {copy.changes.forcePushWarning(action.replaced, action.remote)}
        </p>
      )}
      {confirmPushed && (
        <p role="status" className="text-meta break-words text-diff-modified">
          {copy.changes.amend.pushedWarning(sync?.remote ?? "")}
        </p>
      )}
      {error && (
        <div className="space-y-1">
          <p role="alert" className="text-meta break-words text-destructive">
            {error}
          </p>
          {commitFailed && hasFixAgent && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              title={copy.fixWithAi.hint}
              onClick={fixCommit}
            >
              <SparkleGlyph18 aria-hidden />
              {copy.fixWithAi.button}
            </Button>
          )}
        </div>
      )}
    </form>
  );
}
