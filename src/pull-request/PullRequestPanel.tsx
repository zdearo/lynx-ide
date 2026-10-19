import { toast } from "sonner";

import { commands, type PullRequest } from "@/bindings";
import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleXmarkGlyph18 } from "@/components/icons/CircleXmarkGlyph18";
import { CodePullRequestGlyph18 } from "@/components/icons/CodePullRequestGlyph18";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useDialogStore } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";
import { cn } from "@/lib/utils";
import {
  usePullRequestStore,
  type PullRequestView,
} from "@/pull-request/pull-request-store";
import {
  badgeOf,
  countChecks,
  sortChecks,
  type PullRequestBadge,
} from "@/pull-request/pull-request-view";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/use-selected-worktree";
import { worktreeLabel } from "@/workspace/worktree-label";

/** GitHub's own numbers move slowly; a minute is fresh enough without hammering the API. */
const POLL_MS = 60_000;

const BADGE_TEXT: Record<PullRequestBadge, string> = {
  draft: copy.pullRequest.badge.draft,
  open: copy.pullRequest.badge.open,
  merged: copy.pullRequest.badge.merged,
  closed: copy.pullRequest.badge.closed,
};

/** What a failed read says, by the kind the backend named. */
function errorText(
  view: Extract<PullRequestView, { status: "error" }>,
): string {
  switch (view.kind) {
    case "ghNotInstalled":
      return copy.pullRequest.error.notInstalled;
    case "ghNotLoggedIn":
      return copy.pullRequest.error.notLoggedIn;
    case "ghNoRemote":
      return copy.pullRequest.error.noRemote;
    case "pullRequest":
      return view.message;
    default:
      return copy.pullRequest.error.internal;
  }
}

/** The pull request of the selected worktree's branch, with its checks. */
export function PullRequestPanel() {
  const selected = useSelectedWorktree();
  const worktreePath = selected?.worktree.path ?? null;
  const view = usePullRequestStore((state) =>
    worktreePath ? state.byWorktree[worktreePath] : undefined,
  );
  useRefreshWhileVisible(
    worktreePath,
    (path) => void usePullRequestStore.getState().refresh(path),
    POLL_MS,
  );

  if (!selected || !worktreePath) {
    return (
      <p className="px-3 pt-2 text-ui text-muted-foreground">
        {copy.pullRequest.noWorktree}
      </p>
    );
  }

  const path = worktreePath;
  const refresh = () => void usePullRequestStore.getState().refresh(path);

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between pr-1.5 pl-3">
        <h2 className="min-w-0 truncate text-xs font-medium text-muted-foreground">
          {copy.pullRequest.title}
          <span className="font-normal">
            {" "}
            · {worktreeLabel(selected.worktree).text}
          </span>
        </h2>
        <TooltipIconButton label={copy.pullRequest.refresh} onClick={refresh}>
          <RefreshGlyph18 />
        </TooltipIconButton>
      </div>

      {!view || view.status === "loading" ? (
        <p className="px-3 pt-1 text-ui text-muted-foreground" aria-busy="true">
          {copy.pullRequest.loading}
        </p>
      ) : view.status === "error" ? (
        <div className="px-3 pt-1" role="alert">
          <p className="text-ui text-destructive">
            {copy.pullRequest.error.title}
          </p>
          <p className="mt-1 text-meta break-words text-muted-foreground">
            {errorText(view)}
          </p>
          <div className="mt-2 flex gap-2">
            <Button variant="outline" size="sm" onClick={refresh}>
              {copy.pullRequest.retry}
            </Button>
            {view.kind === "ghNoRemote" && (
              <Button
                size="sm"
                onClick={() => useDialogStore.getState().askCreateRemote(path)}
              >
                {copy.pullRequest.createRemote.button}
              </Button>
            )}
          </div>
        </div>
      ) : view.pullRequest === null ? (
        <p className="px-3 pt-1 text-ui text-muted-foreground">
          {copy.pullRequest.none}
        </p>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <PullRequestDetails pullRequest={view.pullRequest} />
        </ScrollArea>
      )}
    </div>
  );
}

function PullRequestDetails({ pullRequest }: { pullRequest: PullRequest }) {
  const badge = badgeOf(pullRequest);
  const counts = countChecks(pullRequest.checks);
  const review = pullRequest.review;
  const reviewText =
    review === "approved"
      ? copy.pullRequest.review.approved
      : review === "changesRequested"
        ? copy.pullRequest.review.changesRequested
        : review === "reviewRequired"
          ? copy.pullRequest.review.reviewRequired
          : copy.pullRequest.review.none;

  async function openOnGitHub() {
    try {
      await unwrap(commands.openPullRequestUrl(pullRequest.url));
    } catch (error) {
      toast.error(copy.pullRequest.openFailed, {
        description: errorMessage(error),
      });
    }
  }

  return (
    <div className="space-y-3 px-3 pt-1 pb-3">
      <div className="min-w-0 space-y-1">
        <p className="flex items-center gap-1.5 text-meta font-medium text-muted-foreground uppercase">
          <CodePullRequestGlyph18 aria-hidden className="size-3.5 shrink-0" />
          {BADGE_TEXT[badge]}
        </p>
        <h3 className="text-ui font-medium break-words">
          #{pullRequest.number} {pullRequest.title}
        </h3>
        <p className="text-meta text-muted-foreground">
          {copy.pullRequest.branch(pullRequest.headBranch)}
        </p>
        <p className="text-meta text-muted-foreground">{reviewText}</p>
      </div>

      <Button variant="outline" size="sm" onClick={() => void openOnGitHub()}>
        {copy.pullRequest.openOnGitHub}
      </Button>

      <section aria-labelledby="pull-request-checks" className="space-y-1">
        <h4
          id="pull-request-checks"
          className="text-meta font-medium tracking-wide text-muted-foreground uppercase"
        >
          {copy.pullRequest.checks.title}
        </h4>
        {pullRequest.checks.length === 0 ? (
          <p className="text-meta text-muted-foreground">
            {copy.pullRequest.checks.none}
          </p>
        ) : (
          <>
            <p className="text-meta text-muted-foreground tabular-nums">
              {copy.pullRequest.checks.summary(
                counts.passed,
                counts.failed,
                counts.pending,
              )}
            </p>
            <ul className="grid grid-cols-1 gap-0.5">
              {sortChecks(pullRequest.checks).map((check, index) => (
                <li
                  key={`${check.name}-${index}`}
                  className="flex min-w-0 items-center gap-1.5 text-ui"
                >
                  <CheckMark state={check.state} />
                  <span className="min-w-0 flex-1 truncate">{check.name}</span>
                  <span className="shrink-0 text-meta text-muted-foreground">
                    {copy.pullRequest.checks[check.state]}
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>
    </div>
  );
}

/** A shape per state, next to the word, so the state never rests on colour alone. */
function CheckMark({
  state,
}: {
  state: PullRequest["checks"][number]["state"];
}) {
  switch (state) {
    case "passed":
      return (
        <CircleCheckGlyph18
          aria-hidden
          className="size-3.5 shrink-0 text-diff-added"
        />
      );
    case "failed":
      return (
        <CircleXmarkGlyph18
          aria-hidden
          className="size-3.5 shrink-0 text-diff-removed"
        />
      );
    case "pending":
      return (
        <span
          aria-hidden
          className="size-2.5 shrink-0 rounded-full border border-muted-foreground"
        />
      );
    case "skipped":
      return (
        <span
          aria-hidden
          className={cn(
            "size-2.5 shrink-0 rounded-full bg-muted-foreground/40",
          )}
        />
      );
  }
}
