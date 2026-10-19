import { useRef, useState } from "react";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { toast } from "sonner";

import { commands } from "@/bindings";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import { useConfigStore } from "@/config/config-store";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { cn } from "@/lib/utils";
import { useNavigationStore } from "@/navigation/navigation-store";
import { ConfigFileNotice } from "@/settings/ConfigFileNotice";
import { SETTINGS_SECTIONS } from "@/settings/settings-sections";
import { copy } from "@/ui-copy";

async function openConfigFile() {
  try {
    await unwrap(commands.openConfigFile());
  } catch (error) {
    toast.error(copy.settings.file.openFailed, {
      description: errorMessage(error),
    });
  }
}

/** Large centered settings modal: sections on the left, the chosen one on the right. */
export function SettingsDialog() {
  const open = useNavigationStore((state) => state.settingsOpen);
  const closeSettings = useNavigationStore((state) => state.closeSettings);
  const configPath = useConfigStore((state) => state.snapshot?.path);
  const contentRef = useRef<HTMLDivElement>(null);
  const [sectionId, setSectionId] = useState(SETTINGS_SECTIONS[0].id);
  const section =
    SETTINGS_SECTIONS.find((candidate) => candidate.id === sectionId) ??
    SETTINGS_SECTIONS[0];
  const Panel = section.panel;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) closeSettings();
      }}
    >
      <DialogContent
        ref={contentRef}
        showCloseButton={false}
        // Landing on the close button would open its tooltip, and then Esc would close that first.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          contentRef.current?.focus();
        }}
        className="h-[min(44rem,calc(100dvh-4rem))] w-[min(60rem,calc(100vw-4rem))] max-w-none grid-rows-[auto_1fr] gap-0 overflow-hidden p-0 sm:max-w-none"
      >
        <header className="flex h-9 items-center justify-between border-b border-border pr-1.5 pl-3">
          <DialogTitle className="text-ui font-medium">
            {copy.settings.title}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {copy.settings.description}
          </DialogDescription>
          <DialogClose asChild>
            <TooltipIconButton label={copy.settings.close} shortcut="Esc">
              <XmarkGlyph18 />
            </TooltipIconButton>
          </DialogClose>
        </header>

        <div className="flex min-h-0">
          <div
            role="tablist"
            aria-orientation="vertical"
            aria-label={copy.settings.sections}
            className="flex w-48 shrink-0 flex-col gap-0.5 border-r border-border p-2"
          >
            {SETTINGS_SECTIONS.map((candidate) => (
              <button
                key={candidate.id}
                type="button"
                role="tab"
                id={`settings-tab-${candidate.id}`}
                aria-selected={candidate.id === section.id}
                aria-controls="settings-panel"
                onClick={() => setSectionId(candidate.id)}
                className={cn(
                  "flex h-7 items-center gap-2 rounded-md px-2 text-left text-ui",
                  candidate.id === section.id
                    ? "bg-sidebar-accent text-foreground"
                    : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground",
                )}
              >
                <candidate.icon aria-hidden className="size-3.5 shrink-0" />
                {candidate.label}
              </button>
            ))}
          </div>

          <div className="flex min-w-0 flex-1 flex-col">
            <div
              role="tabpanel"
              id="settings-panel"
              aria-labelledby={`settings-tab-${section.id}`}
              className="min-h-0 flex-1 overflow-y-auto p-6"
            >
              <h2 className="mb-5 text-sm font-medium">{section.label}</h2>
              {/* Every page fills the width and spaces its fields the same way. */}
              <div className="grid gap-5">
                <ConfigFileNotice />
                <Panel />
              </div>
            </div>
            <footer className="flex h-11 shrink-0 items-center justify-between gap-3 border-t border-border px-6">
              <p className="min-w-0 truncate text-meta text-muted-foreground">
                {copy.settings.file.label}:{" "}
                <span data-selectable title={configPath}>
                  {configPath}
                </span>
              </p>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => void openConfigFile()}
              >
                {copy.settings.file.open}
              </Button>
            </footer>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
