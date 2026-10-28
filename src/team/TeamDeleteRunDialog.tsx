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
import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

const text = copy.team.runs;

/** Asks before the run on screen is deleted. A refused delete keeps the dialog open, and the store says why. */
export function TeamDeleteRunDialog() {
  const open = useTeamStore((state) => state.deleteRunOpen);
  const runId = useTeamStore((state) => state.runId);
  const run = useTeamStore((state) =>
    state.runs.find((item) => item.id === state.runId),
  );

  return (
    <AlertDialog
      open={open && run !== undefined}
      onOpenChange={(next) => useTeamStore.getState().setDeleteRunOpen(next)}
    >
      {run && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{text.deleteTitle(run.title)}</AlertDialogTitle>
            <AlertDialogDescription>{text.deleteBody}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{text.deleteCancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (runId) void useTeamStore.getState().deleteRun(runId);
              }}
            >
              {text.deleteConfirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
