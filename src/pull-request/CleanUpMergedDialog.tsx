import { useState } from "react";
import { toast } from "sonner";

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
import { useDialogStore } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError } from "@/lib/ipc-result";
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { worktreeLabel } from "@/workspace/worktrees/worktree-label";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Offers to remove the worktree and branch of a pull request that has been merged. */
export function CleanUpMergedDialog() {
  const target = useDialogStore((state) => state.cleanUpTarget);
  const askCleanUp = useDialogStore((state) => state.askCleanUp);
  const [busy, setBusy] = useState(false);
  if (!target) return null;

  const label = worktreeLabel(target.worktree).text;

  async function cleanUp() {
    if (!target) return;
    setBusy(true);
    try {
      const result = await useWorkspaceStore
        .getState()
        .cleanUpMergedWorktree(target.repoId, target.worktree.path);
      await useTerminalStore.getState().closeAllIn(target.worktree.path);
      if (result.branch && !result.branchDeleted) {
        toast.info(
          copy.cleanUp.branchKept(result.branch, result.branchKeptReason ?? ""),
        );
      } else {
        toast.success(copy.cleanUp.cleaned(label));
      }
    } catch (error) {
      const hasChanges =
        error instanceof IpcRequestError && error.kind === "worktreeHasChanges";
      toast.error(copy.cleanUp.failed, {
        description: hasChanges ? copy.cleanUp.hasChanges : errorMessage(error),
      });
    } finally {
      setBusy(false);
      askCleanUp(null);
    }
  }

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) askCleanUp(null);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{copy.cleanUp.title(label)}</AlertDialogTitle>
          <AlertDialogDescription>
            {copy.cleanUp.body(target.worktree.branch ?? label)}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>
            {copy.cleanUp.cancel}
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={busy}
            onClick={(event) => {
              // Stay open until the backend answers, so a second click cannot start another run.
              event.preventDefault();
              void cleanUp();
            }}
          >
            {copy.cleanUp.confirm}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
