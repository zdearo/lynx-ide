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

type Target = NonNullable<
  ReturnType<typeof useDialogStore.getState>["removeWorktreeTarget"]
>;

export function RemoveWorktreeDialog() {
  const target = useDialogStore((state) => state.removeWorktreeTarget);
  const askRemoveWorktree = useDialogStore((state) => state.askRemoveWorktree);

  return (
    <AlertDialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) askRemoveWorktree(null);
      }}
    >
      {target && (
        <RemoveWorktreeBody
          key={target.worktree.path}
          target={target}
          onDone={() => askRemoveWorktree(null)}
        />
      )}
    </AlertDialog>
  );
}

function RemoveWorktreeBody({
  target,
  onDone,
}: {
  target: Target;
  onDone: () => void;
}) {
  const [phase, setPhase] = useState<"confirm" | "changes">("confirm");
  const [busy, setBusy] = useState(false);
  const label = worktreeLabel(target.worktree).text;

  async function remove(force: boolean) {
    setBusy(true);
    try {
      const result = await useWorkspaceStore
        .getState()
        .removeWorktree(target.repoId, target.worktree.path, force);
      await useTerminalStore.getState().closeAllIn(target.worktree.path);
      if (result.branch && !result.branchDeleted) {
        toast.info(
          copy.removeWorktree.branchKept(
            result.branch,
            result.branchKeptReason ?? "",
          ),
        );
      }
      onDone();
    } catch (error) {
      const hasChanges =
        error instanceof IpcRequestError && error.kind === "worktreeHasChanges";
      if (hasChanges && !force) {
        // Ask again, now that we know discarding work is what it would take.
        setPhase("changes");
      } else {
        toast.error(copy.removeWorktree.failed, {
          description: errorMessage(error),
        });
        onDone();
      }
    } finally {
      setBusy(false);
    }
  }

  const needsForce = phase === "changes";

  return (
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>
          {needsForce
            ? copy.removeWorktree.changesTitle
            : copy.removeWorktree.title(label)}
        </AlertDialogTitle>
        <AlertDialogDescription>
          {needsForce
            ? copy.removeWorktree.changesBody
            : copy.removeWorktree.body}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel disabled={busy}>
          {copy.removeWorktree.cancel}
        </AlertDialogCancel>
        <AlertDialogAction
          variant="destructive"
          disabled={busy}
          onClick={(event) => {
            // Stay open: a refusal switches this dialog to the "discard changes" step.
            event.preventDefault();
            void remove(needsForce);
          }}
        >
          {needsForce
            ? copy.removeWorktree.confirmForce
            : copy.removeWorktree.confirm}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  );
}
