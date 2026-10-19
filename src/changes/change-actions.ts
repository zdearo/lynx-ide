import { toast } from "sonner";

import { commands, type ChangedFile } from "@/bindings";
import { useChangesStore } from "@/changes/changes-store";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** Puts a changed file into the index, or reports why it could not. */
export async function stageFile(worktreePath: string, file: ChangedFile) {
  try {
    await unwrap(commands.stageChange(worktreePath, file.path));
  } catch (error) {
    toast.error(copy.changes.stageFailed, { description: errorMessage(error) });
  }
  await useChangesStore.getState().refresh(worktreePath);
}

/** Puts every listed file into the index in one step, as a section's "stage all" does. */
export async function stageFiles(worktreePath: string, paths: string[]) {
  try {
    await unwrap(commands.stageChanges(worktreePath, paths));
  } catch (error) {
    toast.error(copy.changes.stageFailed, { description: errorMessage(error) });
  }
  await useChangesStore.getState().refresh(worktreePath);
}

/** Takes a file out of the index; the working tree keeps its content. */
export async function unstageFile(worktreePath: string, file: ChangedFile) {
  try {
    await unwrap(
      commands.unstageChange(worktreePath, file.path, file.originalPath),
    );
  } catch (error) {
    toast.error(copy.changes.unstageFailed, {
      description: errorMessage(error),
    });
  }
  await useChangesStore.getState().refresh(worktreePath);
}
