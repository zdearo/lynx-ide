import { useEffect } from "react";
import { toast } from "sonner";

import type { ResumableSession } from "@/bindings";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";
import { TERMINAL_PRESETS } from "@/agents/agent-presets";
import { launchLine, startTerminal } from "@/terminal/terminal-actions";
import { useResumableStore } from "@/workspace/resumable-store";
import { resumeCommand } from "@/workspace/resume-command";

/** Ended agent sessions of a worktree, each with a button that starts its conversation again. */
export function ResumableList({ worktreePath }: { worktreePath: string }) {
  const items = useResumableStore((state) => state.byWorktree[worktreePath]);
  const refresh = useResumableStore((state) => state.refresh);
  const forget = useResumableStore((state) => state.forget);
  const remove = useResumableStore((state) => state.remove);

  useEffect(() => {
    void refresh(worktreePath);
  }, [refresh, worktreePath]);

  if (!items?.length) return null;

  const resume = async (item: ResumableSession) => {
    const preset = TERMINAL_PRESETS.find(
      (candidate) => candidate.harness === item.harness,
    );
    const launch = preset ? await launchLine(preset) : null;
    const command = resumeCommand(launch, item.resumeArguments);
    if (!command) {
      toast.error(copy.resumable.failed);
      return;
    }
    forget(worktreePath, item.id);
    void startTerminal(worktreePath, command).then(() => refresh(worktreePath));
  };

  const take = (item: ResumableSession) => {
    remove(worktreePath, item.id).catch((error: unknown) => {
      toast.error(copy.resumable.removeFailed, {
        description: errorMessage(error),
      });
    });
  };

  return (
    <div className="pr-1.5 pb-1.5 pl-7">
      <ul className="grid grid-cols-1 gap-1">
        {items.map((item) => (
          <li
            key={item.id}
            className="group/resume flex h-7 items-center gap-1.5 rounded-md pr-1 pl-2 text-nested text-muted-foreground"
          >
            <AgentBrandIcon
              harness={item.harness}
              className="size-3 shrink-0"
            />
            <span className="min-w-0 flex-1 truncate">
              {copy.settings.agents.harnesses[item.harness].name}
            </span>
            <span className="shrink-0 text-meta text-muted-foreground">
              {copy.resumable.ended}
            </span>
            <TooltipIconButton
              label={copy.resumable.resume}
              onClick={() => void resume(item)}
            >
              <RefreshGlyph18 aria-hidden className="size-3" />
            </TooltipIconButton>
            <TooltipIconButton
              label={copy.resumable.remove}
              onClick={() => take(item)}
            >
              <XmarkGlyph18 aria-hidden className="size-3" />
            </TooltipIconButton>
          </li>
        ))}
      </ul>
    </div>
  );
}
