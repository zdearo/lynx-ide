import { useDialogStore } from "@/dialog-store";
import { useEditorStore } from "@/editor/editor-store";

/** Closing a file with unsaved changes asks first. */
export function requestCloseFile(worktreePath: string, path: string): void {
  const store = useEditorStore.getState();
  const file = store.files[worktreePath]?.find((entry) => entry.path === path);
  if (file?.dirty) {
    useDialogStore.getState().askCloseFile({ worktreePath, path });
  } else {
    store.close(worktreePath, path);
  }
}
