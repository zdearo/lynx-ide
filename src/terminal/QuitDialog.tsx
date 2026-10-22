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
import { commands } from "@/bindings";
import { useDialogStore } from "@/dialog-store";
import { unwrap } from "@/lib/ipc-result";
import { closeWindow } from "@/quit-guard";
import { copy } from "@/ui-copy";

/** Asked when the window closes while the daemon holds terminals: keep them, or end them. */
export function QuitDialog() {
  const sessions = useDialogStore((state) => state.quitSessions);
  const askQuit = useDialogStore((state) => state.askQuit);
  if (sessions === null) return null;

  const decide = (keep: boolean) => {
    void unwrap(commands.prepareQuit(keep)).then(closeWindow);
  };

  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open) askQuit(null);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{copy.quit.title}</AlertDialogTitle>
          <AlertDialogDescription>
            {copy.quit.body(sessions)}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{copy.quit.cancel}</AlertDialogCancel>
          <AlertDialogAction variant="outline" onClick={() => decide(false)}>
            {copy.quit.end}
          </AlertDialogAction>
          <AlertDialogAction onClick={() => decide(true)}>
            {copy.quit.keep}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
