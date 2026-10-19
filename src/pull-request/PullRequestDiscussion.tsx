import { useState } from "react";
import { toast } from "sonner";

import {
  commands,
  type DiscussionComment,
  type ReviewThread,
} from "@/bindings";
import { relativeTime } from "@/changes/commit-time";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { cn } from "@/lib/utils";
import { isoSeconds, timeline } from "@/pull-request/discussion-timeline";
import {
  usePullRequestDiscussionStore,
  type DiscussionView,
} from "@/pull-request/pull-request-discussion-store";
import { copy } from "@/ui-copy";

const text = copy.pullRequest.discussion;

/** Long bodies (a bot's analysis, a stack trace) start short, and open on request. */
function Body({ body }: { body: string }) {
  const [open, setOpen] = useState(false);
  const long = body.length > 320 || body.split("\n").length > 8;
  return (
    <div className="min-w-0">
      <p
        className={cn(
          "text-ui break-words whitespace-pre-wrap",
          long && !open && "line-clamp-6",
        )}
      >
        {body}
      </p>
      {long && (
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="mt-0.5 rounded-sm text-meta text-muted-foreground underline underline-offset-2 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          {open ? text.showLess : text.showMore}
        </button>
      )}
    </div>
  );
}

function When({ iso, readAt }: { iso: string; readAt: number }) {
  return <>{relativeTime(isoSeconds(iso), readAt)}</>;
}

function CommentView({
  comment,
  readAt,
}: {
  comment: DiscussionComment;
  readAt: number;
}) {
  return (
    <div className="grid gap-0.5">
      <p className="text-meta text-muted-foreground">
        <span className="font-medium text-foreground">{comment.author}</span> ·{" "}
        <When iso={comment.createdAt} readAt={readAt} />
      </p>
      <Body body={comment.body} />
    </div>
  );
}

/** One inline conversation: where it is, who said what, and the ways to answer it. */
function ThreadView({
  worktreePath,
  number,
  thread,
  readAt,
}: {
  worktreePath: string;
  number: number;
  thread: ReviewThread;
  readAt: number;
}) {
  const [replying, setReplying] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function act(run: () => Promise<unknown>, done: () => void) {
    setBusy(true);
    setError(null);
    try {
      await run();
      done();
      await usePullRequestDiscussionStore
        .getState()
        .refresh(worktreePath, number);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <li className="grid gap-1.5 rounded-md border border-border p-2">
      <p className="flex flex-wrap items-center gap-1.5 text-meta text-muted-foreground">
        <span className="min-w-0 truncate font-mono" title={thread.path}>
          {thread.path}
          {thread.line !== null && `:${thread.line}`}
        </span>
        {thread.resolved && (
          <span className="font-medium text-diff-added">{text.resolved}</span>
        )}
        {thread.outdated && <span>{text.outdated}</span>}
      </p>
      {thread.comments.map((comment) => (
        <CommentView key={comment.id} comment={comment} readAt={readAt} />
      ))}

      {replying && (
        <div className="grid gap-1">
          <Textarea
            aria-label={text.replyLabel(thread.path)}
            rows={3}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
          <div className="flex gap-1.5">
            <Button
              size="sm"
              disabled={busy || draft.trim() === ""}
              onClick={() =>
                void act(
                  async () =>
                    unwrap(
                      commands.replyToReviewThread(
                        worktreePath,
                        thread.id,
                        draft,
                      ),
                    ),
                  () => {
                    setDraft("");
                    setReplying(false);
                  },
                )
              }
            >
              {busy ? text.sending : text.reply}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => setReplying(false)}
            >
              {text.cancel}
            </Button>
          </div>
        </div>
      )}

      <div className="flex gap-1.5">
        {!replying && (
          <Button
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => setReplying(true)}
          >
            {text.reply}
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={() =>
            void act(
              async () =>
                unwrap(
                  commands.resolveReviewThread(
                    worktreePath,
                    thread.id,
                    !thread.resolved,
                  ),
                ),
              () => undefined,
            )
          }
        >
          {thread.resolved ? text.reopenThread : text.resolveThread}
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-meta break-words text-destructive">
          {error}
        </p>
      )}
    </li>
  );
}

/** The conversation of a pull request: a box to comment, the inline threads, and what was said. */
export function PullRequestDiscussion({
  worktreePath,
  number,
}: {
  worktreePath: string;
  number: number;
}) {
  const view = usePullRequestDiscussionStore(
    (state) => state.byWorktree[worktreePath],
  );
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function send() {
    setSending(true);
    setError(null);
    try {
      await unwrap(commands.commentOnPullRequest(worktreePath, number, draft));
      setDraft("");
      toast.success(text.sent);
      await usePullRequestDiscussionStore
        .getState()
        .refresh(worktreePath, number);
    } catch (caught) {
      setError(`${text.failed}: ${errorMessage(caught)}`);
    } finally {
      setSending(false);
    }
  }

  return (
    <div className="grid gap-3">
      <div className="grid gap-1.5">
        <Textarea
          aria-label={text.composerLabel}
          placeholder={text.composerPlaceholder}
          rows={3}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button
          size="sm"
          className="justify-self-end"
          disabled={sending || draft.trim() === ""}
          onClick={() => void send()}
        >
          {sending ? text.sending : text.send}
        </Button>
        {error && (
          <p role="alert" className="text-meta break-words text-destructive">
            {error}
          </p>
        )}
      </div>

      {!view || view.status === "loading" ? (
        <p className="text-ui text-muted-foreground" aria-busy="true">
          {text.loading}
        </p>
      ) : view.status === "error" ? (
        <p role="alert" className="text-meta break-words text-destructive">
          {text.loadFailed}: {view.message}
        </p>
      ) : (
        <DiscussionBody
          worktreePath={worktreePath}
          number={number}
          view={view}
        />
      )}
    </div>
  );
}

function DiscussionBody({
  worktreePath,
  number,
  view,
}: {
  worktreePath: string;
  number: number;
  view: Extract<DiscussionView, { status: "ready" }>;
}) {
  const { discussion, readAt } = view;
  const entries = timeline(discussion.comments, discussion.reviews);
  if (entries.length === 0 && discussion.threads.length === 0) {
    return <p className="text-ui text-muted-foreground">{text.empty}</p>;
  }
  return (
    <>
      {discussion.threads.length > 0 && (
        <section
          aria-labelledby="pull-request-threads"
          className="grid gap-1.5"
        >
          <h4
            id="pull-request-threads"
            className="text-meta font-medium tracking-wide text-muted-foreground uppercase"
          >
            {text.threads}
          </h4>
          <ul className="grid gap-2">
            {discussion.threads.map((thread) => (
              <ThreadView
                key={thread.id}
                worktreePath={worktreePath}
                number={number}
                thread={thread}
                readAt={readAt}
              />
            ))}
          </ul>
        </section>
      )}
      {entries.length > 0 && (
        <section
          aria-labelledby="pull-request-conversation"
          className="grid gap-1.5"
        >
          <h4
            id="pull-request-conversation"
            className="text-meta font-medium tracking-wide text-muted-foreground uppercase"
          >
            {text.conversation}
          </h4>
          <ul className="grid gap-2.5">
            {entries.map((entry, index) =>
              entry.kind === "comment" ? (
                <li key={`c-${entry.comment.id}-${index}`}>
                  <CommentView comment={entry.comment} readAt={readAt} />
                </li>
              ) : (
                <li key={`r-${index}`} className="grid gap-0.5">
                  <p className="text-meta text-muted-foreground">
                    <span className="font-medium text-foreground">
                      {entry.review.author}
                    </span>{" "}
                    {text.verdict[entry.review.verdict]} ·{" "}
                    <When iso={entry.review.submittedAt} readAt={readAt} />
                  </p>
                  {entry.review.body.trim() !== "" && (
                    <Body body={entry.review.body} />
                  )}
                </li>
              ),
            )}
          </ul>
        </section>
      )}
    </>
  );
}
