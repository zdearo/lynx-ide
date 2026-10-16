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
import { useTerminalStore } from "@/terminal/terminal-store";
import { copy } from "@/ui-copy";

/** Asks before closing a terminal whose process is still running. Open while a request exists. */
export function CloseTerminalDialog() {
  const id = useDialogStore((state) => state.closeTerminalId);
  const askCloseTerminal = useDialogStore((state) => state.askCloseTerminal);
  const close = useTerminalStore((state) => state.close);

  return (
    <AlertDialog
      open={id !== null}
      onOpenChange={(open) => {
        if (!open) askCloseTerminal(null);
      }}
    >
      {id && (
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {copy.terminal.closeDialog.title}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {copy.terminal.closeDialog.body}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              {copy.terminal.closeDialog.keep}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void close(id)}
            >
              {copy.terminal.closeDialog.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      )}
    </AlertDialog>
  );
}
