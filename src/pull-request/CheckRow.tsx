import { useState } from "react";
import { toast } from "sonner";

import { commands, type CheckLog, type PullRequestCheck } from "@/bindings";
import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleXmarkGlyph18 } from "@/components/icons/CircleXmarkGlyph18";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { hasJobLog } from "@/pull-request/pull-request-view";
import { copy } from "@/ui-copy";

/** A shape per state, next to the word, so the state never rests on colour alone. */
function CheckMark({ state }: { state: PullRequestCheck["state"] }) {
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
          className="size-2.5 shrink-0 rounded-full bg-muted-foreground/40"
        />
      );
  }
}

const linkButton =
  "shrink-0 rounded-sm text-meta text-muted-foreground underline underline-offset-2 outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60";

/** One check: its state in words, a log to read inline, and its page on GitHub. */
export function CheckRow({
  worktreePath,
  check,
}: {
  worktreePath: string;
  check: PullRequestCheck;
}) {
  const [log, setLog] = useState<
    | { status: "closed" }
    | { status: "loading" }
    | { status: "ready"; log: CheckLog }
    | { status: "error"; message: string }
  >({ status: "closed" });
  const url = check.detailsUrl;

  async function toggleLog() {
    if (log.status !== "closed") {
      setLog({ status: "closed" });
      return;
    }
    if (!url) return;
    setLog({ status: "loading" });
    try {
      const read = await unwrap(
        commands.getCheckLog(worktreePath, url, check.state === "failed"),
      );
      setLog({ status: "ready", log: read });
    } catch (error) {
      setLog({ status: "error", message: errorMessage(error) });
    }
  }

  async function openPage() {
    if (!url) return;
    try {
      await unwrap(commands.openPullRequestUrl(url));
    } catch (error) {
      toast.error(copy.pullRequest.openFailed, {
        description: errorMessage(error),
      });
    }
  }

  return (
    <li className="grid min-w-0 gap-1">
      <div className="flex min-w-0 items-center gap-1.5 text-ui">
        <CheckMark state={check.state} />
        <span className="min-w-0 flex-1 truncate" title={check.name}>
          {check.name}
        </span>
        <span className="shrink-0 text-meta text-muted-foreground">
          {copy.pullRequest.checks[check.state]}
        </span>
        {hasJobLog(check) && (
          <button
            type="button"
            aria-expanded={log.status !== "closed"}
            aria-label={copy.pullRequest.checks.logOf(check.name)}
            onClick={() => void toggleLog()}
            className={linkButton}
          >
            {log.status === "closed"
              ? copy.pullRequest.checks.log
              : copy.pullRequest.checks.hideLog}
          </button>
        )}
        {url && (
          <button
            type="button"
            aria-label={copy.pullRequest.checks.openOf(check.name)}
            onClick={() => void openPage()}
            className={linkButton}
          >
            {copy.pullRequest.checks.open}
          </button>
        )}
      </div>
      {log.status === "loading" && (
        <p className="pl-5 text-meta text-muted-foreground" aria-busy="true">
          {copy.pullRequest.checks.logLoading}
        </p>
      )}
      {log.status === "error" && (
        <p role="alert" className="pl-5 text-meta break-words text-destructive">
          {copy.pullRequest.checks.logFailed}: {log.message}
        </p>
      )}
      {log.status === "ready" && (
        <div className="grid gap-1 pl-5">
          {log.log.truncated && (
            <p className="text-meta text-muted-foreground">
              {copy.pullRequest.checks.logTruncated}
            </p>
          )}
          <pre
            tabIndex={0}
            aria-label={copy.pullRequest.checks.logText(check.name)}
            className="max-h-64 overflow-auto rounded-md bg-muted p-2 font-mono text-meta break-words whitespace-pre-wrap"
          >
            {log.log.text || copy.pullRequest.checks.logEmpty}
          </pre>
        </div>
      )}
    </li>
  );
}
