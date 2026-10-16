import type { ReactNode } from "react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useDialogStore } from "@/dialog-store";
import { shortcutLabel } from "@/platform-shortcuts";
import { useVisiblePresets } from "@/agents/agent-catalog-store";
import { PresetIcon } from "@/terminal/PresetIcon";
import { startTerminal } from "@/terminal/terminal-actions";
import { copy } from "@/ui-copy";

/** What to start in a worktree: a shell, a known agent, or any command. */
export function NewTerminalMenu({
  worktreePath,
  children,
}: {
  worktreePath: string;
  children: ReactNode;
}) {
  const presets = useVisiblePresets();
  const askCustomCommand = useDialogStore((state) => state.askCustomCommand);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-44">
        {presets.map((preset) => (
          <DropdownMenuItem
            key={preset.id}
            onSelect={() => void startTerminal(worktreePath, preset.command)}
          >
            <PresetIcon harness={preset.harness} />
            {preset.label}
            {preset.kind === "shell" && (
              <DropdownMenuShortcut>
                {shortcutLabel("newTerminal")}
              </DropdownMenuShortcut>
            )}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => askCustomCommand(worktreePath)}>
          {copy.terminal.customCommand}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
