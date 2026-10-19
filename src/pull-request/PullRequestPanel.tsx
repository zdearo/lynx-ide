import { useState } from "react";
import { toast } from "sonner";

import { brokenChecksPrompt } from "@/agents/fix-prompts";
import { fixWithAgent, useHasFixAgent } from "@/agents/fix-with-agent";
import { commands, type PullRequest } from "@/bindings";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
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
import { CreatePullRequestForm } from "@/pull-request/CreatePullRequestForm";
import { CheckRow } from "@/pull-request/CheckRow";
import { PullRequestActions } from "@/pull-request/PullRequestActions";
import { PullRequestDiscussion } from "@/pull-request/PullRequestDiscussion";
import { usePullRequestDiscussionStore } from "@/pull-request/pull-request-discussion-store";
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
        <ScrollArea className="min-h-0 flex-1">
          <CreatePullRequestForm worktreePath={path} />
        </ScrollArea>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <PullRequestDetails
            worktreePath={path}
            pullRequest={view.pullRequest}
          />
        </ScrollArea>
      )}
    </div>
  );
}

function PullRequestDetails({
  worktreePath,
  pullRequest,
}: {
  worktreePath: string;
  pullRequest: PullRequest;
}) {
  const badge = badgeOf(pullRequest);
  const counts = countChecks(pullRequest.checks);
  const hasFixAgent = useHasFixAgent();
  const [subview, setSubview] = useState<"checks" | "discussion">("checks");
  const discussion = usePullRequestDiscussionStore(
    (state) => state.byWorktree[worktreePath],
  );
  const discussionCount =
    discussion?.status === "ready"
      ? discussion.discussion.comments.length +
        discussion.discussion.reviews.length +
        discussion.discussion.threads.length
      : 0;
  // The conversation moves on GitHub too: read it again every minute while the tab is open.
  useRefreshWhileVisible(
    worktreePath,
    (path) =>
      void usePullRequestDiscussionStore
        .getState()
        .refresh(path, pullRequest.number),
    POLL_MS,
  );
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

      <PullRequestActions
        worktreePath={worktreePath}
        pullRequest={pullRequest}
      />

      <div
        role="group"
        aria-label={copy.pullRequest.discussion.viewGroup}
        className="flex gap-1 border-b border-border pb-1.5"
      >
        {(["checks", "discussion"] as const).map((id) => (
          <button
            key={id}
            type="button"
            aria-pressed={subview === id}
            onClick={() => setSubview(id)}
            className={cn(
              "h-6 rounded-md px-2 text-meta font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
              subview === id
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "text-muted-foreground hover:bg-sidebar-accent/50",
            )}
          >
            {id === "checks"
              ? copy.pullRequest.discussion.checksTab
              : copy.pullRequest.discussion.tab}
            <span className="ml-1 font-normal tabular-nums">
              {id === "checks" ? pullRequest.checks.length : discussionCount}
            </span>
          </button>
        ))}
      </div>

      {subview === "discussion" && (
        <PullRequestDiscussion
          worktreePath={worktreePath}
          number={pullRequest.number}
        />
      )}

      {subview === "checks" && (
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
              {counts.failed > 0 && hasFixAgent && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  title={copy.fixWithAi.hint}
                  onClick={() =>
                    void fixWithAgent(
                      worktreePath,
                      brokenChecksPrompt({
                        number: pullRequest.number,
                        title: pullRequest.title,
                        branch: pullRequest.headBranch,
                        checks: pullRequest.checks
                          .filter((check) => check.state === "failed")
                          .map((check) => ({
                            name: check.name,
                            url: check.detailsUrl,
                          })),
                      }),
                    )
                  }
                >
                  <SparkleGlyph18 aria-hidden />
                  {copy.fixWithAi.fixChecks}
                </Button>
              )}
              <ul className="grid grid-cols-1 gap-1">
                {sortChecks(pullRequest.checks).map((check, index) => (
                  <CheckRow
                    key={`${check.name}-${index}`}
                    worktreePath={worktreePath}
                    check={check}
                  />
                ))}
              </ul>
            </>
          )}
        </section>
      )}
    </div>
  );
}
