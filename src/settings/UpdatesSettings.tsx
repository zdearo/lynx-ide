import { useState } from "react";
import { toast } from "sonner";

import type { UpdatePhase } from "@/bindings";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import { SwitchSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";
import { useUpdateStore } from "@/updates/update-store";

/** What the last check or install found, in one line. */
function phaseText(phase: UpdatePhase): string {
  const text = copy.settings.updates;
  switch (phase.kind) {
    case "unsupported":
      return text.unsupported;
    case "idle":
      return text.idle;
    case "checking":
      return text.checking;
    case "upToDate":
      return text.upToDate;
    case "available":
      return text.available(phase.version);
    case "downloading":
      return text.downloading(phase.percent);
    case "readyToRestart":
      return text.readyToRestart(phase.version);
    case "failed":
      return text.failed;
  }
}

/** A check or an install is running, so the actions wait for it. */
function busy(phase: UpdatePhase): boolean {
  return (
    phase.kind === "checking" ||
    phase.kind === "downloading" ||
    phase.kind === "readyToRestart"
  );
}

async function run(action: () => Promise<void>, failure: string) {
  try {
    await action();
  } catch (error) {
    toast.error(failure, { description: errorMessage(error) });
  }
}

export function UpdatesSettings() {
  const { config, disabled, save } = useSettings();
  const status = useUpdateStore((state) => state.status);
  const check = useUpdateStore((state) => state.check);
  const install = useUpdateStore((state) => state.install);
  const restart = useUpdateStore((state) => state.restart);
  const [restartOpen, setRestartOpen] = useState(false);
  if (!config || !status) return null;

  const { phase } = status;
  const unsupported = phase.kind === "unsupported";
  const text = copy.settings.updates;

  return (
    <>
      <p className="text-nested text-muted-foreground">{text.intro}</p>
      <div className="grid gap-1.5">
        <p className="text-ui">{text.installed(status.currentVersion)}</p>
        <p role="status" className="text-nested text-muted-foreground">
          {phaseText(phase)}
        </p>
        {status.checkedAt && (
          <p className="text-meta text-muted-foreground">
            {text.lastChecked(new Date(status.checkedAt).toLocaleString())}
          </p>
        )}
        {phase.kind === "available" && phase.notes && (
          <div className="grid gap-1">
            <p className="text-meta text-muted-foreground">{text.notes}</p>
            <p className="text-nested whitespace-pre-line">{phase.notes}</p>
          </div>
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          disabled={unsupported || busy(phase)}
          onClick={() => void run(check, text.checkFailed)}
        >
          {text.check}
        </Button>
        {phase.kind === "available" && (
          <Button onClick={() => void run(install, text.installFailed)}>
            {text.install(phase.version)}
          </Button>
        )}
        {phase.kind === "readyToRestart" && (
          <Button onClick={() => setRestartOpen(true)}>{text.restart}</Button>
        )}
      </div>

      <SwitchSetting
        id="updates-automatic"
        label={text.automatic.label}
        hint={text.automatic.hint}
        checked={config.updates.checkAutomatically}
        disabled={disabled || unsupported}
        onCommit={(checkAutomatically) =>
          save({ ...config, updates: { checkAutomatically } })
        }
      />

      <AlertDialog open={restartOpen} onOpenChange={setRestartOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{text.restartTitle}</AlertDialogTitle>
            <AlertDialogDescription>{text.restartBody}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{text.cancel}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => void run(restart, text.restartFailed)}
            >
              {text.restartConfirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
