import { useState } from "react";

import { toast } from "sonner";

import { commands, type RunningCommand, type WorkerRow } from "@/bindings";
import { selectedBotOf, useBotsStore } from "@/bots/bots-store";
import { DetailSection } from "@/bots/DetailSection";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { XIcon } from "@/components/icons/line-icons";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { errorMessage } from "@/lib/error-message";
import { WIDE_LAYOUT_QUERY } from "@/hooks/layout-queries";
import { useMediaQuery } from "@/hooks/use-media-query";
import { unwrap } from "@/lib/ipc-result";
import { useRefreshWhileVisible } from "@/lib/use-refresh-while-visible";
import { cn } from "@/lib/utils";
import { useNavigationStore } from "@/navigation/navigation-store";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** How often the list is read again while the tab is open, in milliseconds. */
const REFRESH_MS = 3000;

/**
 * The side panel's Agents tab: the agents the picked bot started, and whether each one still runs.
 * Opening one takes the person to its worktree and terminal in Projects.
 */
export function BotAgentsPanel() {
  const bot = useBotsStore(selectedBotOf);
  const botId = bot?.id ?? null;
  // The list is kept with the bot it belongs to, so the panel never shows another bot's agents.
  const [loaded, setLoaded] = useState<{
    id: string;
    rows: WorkerRow[];
    running: RunningCommand[];
  } | null>(null);
  const [failed, setFailed] = useState(false);
  const wide = useMediaQuery(WIDE_LAYOUT_QUERY);
  const text = copy.bots.agents;
  const mine = loaded && loaded.id === botId ? loaded : null;
  const rows = mine?.rows ?? null;
  const running = mine?.running ?? [];

  useRefreshWhileVisible(
    botId,
    (id) => {
      Promise.all([
        unwrap(commands.botsWorkers(id)),
        unwrap(commands.botsRunningCommands(id)),
      ])
        .then(([found, left]) => {
          setFailed(false);
          setLoaded({ id, rows: found, running: left });
        })
        .catch(() => setFailed(true));
    },
    REFRESH_MS,
  );

  function open(row: WorkerRow) {
    // A worker's terminal is listed only while it has one; a stopped worker has none to open.
    if (row.sessionId === null) return;
    useNavigationStore.getState().selectView("projects", wide);
    useWorkspaceStore.getState().selectWorktree(row.worktreePath);
    useTerminalStore.getState().setActive(row.worktreePath, row.sessionId);
    useTerminalStore.getState().requestFocus(row.sessionId);
  }

  async function stop(session: string) {
    if (!botId) return;
    try {
      await unwrap(commands.botsStopCommand(botId, session));
      setLoaded((now) =>
        now
          ? {
              ...now,
              running: now.running.map((one) =>
                one.session === session ? { ...one, running: false } : one,
              ),
            }
          : now,
      );
    } catch (error) {
      toast.error(copy.bots.agents.commands.stopFailed, {
        description: errorMessage(error),
      });
    }
  }

  if (rows === null) {
    return (
      <p className="p-4 text-ui text-muted-foreground">
        {failed ? text.loadFailed : null}
      </p>
    );
  }
  return (
    <div className="grid gap-4 p-2">
      {rows.length === 0 ? (
        <p className="px-2 pt-2 text-ui text-muted-foreground">{text.none}</p>
      ) : (
        <ul
          role="list"
          aria-label={text.tab}
          className="grid grid-cols-1 gap-1"
        >
          {rows.map((row) => (
            <li key={row.memberId}>
              <button
                type="button"
                disabled={row.sessionId === null}
                aria-label={text.open(row.task)}
                onClick={() => open(row)}
                className="flex w-full min-w-0 items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
              >
                <AgentBrandIcon
                  harness={row.harness}
                  className="size-4 shrink-0"
                />
                <span className="min-w-0 flex-1 truncate text-ui">
                  {row.task}
                </span>
                <span
                  aria-hidden="true"
                  className={cn(
                    "size-2 shrink-0 rounded-full",
                    row.running ? "bg-live" : "border border-muted-foreground",
                  )}
                />
                <span className="shrink-0 text-meta text-muted-foreground">
                  {statusLabel(row, text)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <CommandsList
        running={running}
        onStop={(session) => void stop(session)}
      />
    </div>
  );
}

/** The commands the bot left running, each with the end of what it printed and a way to stop it. */
function CommandsList({
  running,
  onStop,
}: {
  running: RunningCommand[];
  onStop: (session: string) => void;
}) {
  const text = copy.bots.agents.commands;
  return (
    <DetailSection title={text.title}>
      {running.length === 0 ? (
        <p className="text-ui text-muted-foreground">{text.none}</p>
      ) : (
        <ul
          role="list"
          aria-label={text.title}
          className="grid grid-cols-1 gap-1"
        >
          {running.map((one) => (
            <li
              key={one.session}
              className="grid gap-1 rounded-md bg-muted/40 py-1.5 pr-1 pl-2.5"
            >
              <div className="flex min-w-0 items-center gap-2">
                <span
                  aria-hidden="true"
                  className={cn(
                    "size-2 shrink-0 rounded-full",
                    one.running ? "bg-live" : "border border-muted-foreground",
                  )}
                />
                <span className="min-w-0 flex-1 truncate font-mono text-ui">
                  {one.command}
                </span>
                <span className="shrink-0 text-meta text-muted-foreground">
                  {one.running ? text.running : text.ended}
                </span>
                {one.running && (
                  <TooltipIconButton
                    label={text.stop(one.command)}
                    onClick={() => onStop(one.session)}
                  >
                    <XIcon />
                  </TooltipIconButton>
                )}
              </div>
              <details className="pr-1.5">
                <summary className="cursor-pointer text-meta text-muted-foreground">
                  {text.output}
                </summary>
                <pre className="mt-1 max-h-48 overflow-auto font-mono text-meta whitespace-pre-wrap break-all">
                  {one.output || text.noOutput}
                </pre>
              </details>
            </li>
          ))}
        </ul>
      )}
    </DetailSection>
  );
}

/** What the row says about its worker: its agent's status while it runs, or that it ended. */
function statusLabel(row: WorkerRow, text: typeof copy.bots.agents): string {
  if (!row.running) return text.ended;
  const status = row.agentStatus;
  if (status !== null && status in text.status) {
    return text.status[status as keyof typeof text.status];
  }
  return text.running;
}
