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
import { discardFiles } from "@/changes/change-actions";
import { useDialogStore } from "@/dialog-store";
import { copy } from "@/ui-copy";

/** Asks before unstaged changes are thrown away, since they cannot be recovered. */
export function DiscardDialog() {
  const target = useDialogStore((state) => state.discardTarget);
  const askDiscard = useDialogStore((state) => state.askDiscard);

  return (
    <AlertDialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) askDiscard(null);
      }}
    >
      {target && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {copy.changes.discard.title(target.paths.length)}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {copy.changes.discard.body}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{copy.changes.discard.cancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() =>
                void discardFiles(target.worktreePath, target.paths)
              }
            >
              {copy.changes.discard.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
