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
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";
import { useWorkspaceStore } from "@/workspace/workspace-store";

export function RemoveRepoDialog() {
  const repo = useDialogStore((state) => state.removeRepoTarget);
  const askRemoveRepo = useDialogStore((state) => state.askRemoveRepo);

  async function remove() {
    if (!repo) return;
    const paths = (useWorkspaceStore.getState().worktrees[repo.id] ?? []).map(
      (worktree) => worktree.path,
    );
    try {
      await useWorkspaceStore.getState().removeRepo(repo.id);
      // Their terminals would keep running with no way to reach them.
      await Promise.all(
        paths.map((path) => useTerminalStore.getState().closeAllIn(path)),
      );
    } catch (error) {
      toast.error(copy.errors.generic, { description: errorMessage(error) });
    }
  }

  return (
    <AlertDialog
      open={repo !== null}
      onOpenChange={(open) => {
        if (!open) askRemoveRepo(null);
      }}
    >
      {repo && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {copy.removeRepo.title(repo.name)}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {repo.linkedRootId
                ? copy.removeRepo.linkedBody
                : copy.removeRepo.body}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{copy.removeRepo.cancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void remove()}
            >
              {copy.removeRepo.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
