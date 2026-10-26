import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";

import { copy } from "@/ui-copy";
import { errorMessage } from "@/lib/error-message";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Asks for a folder, then registers it as a repository. */
export async function openRepositoryFromPicker(): Promise<void> {
  const chosen = await open({ directory: true, multiple: false });
  if (typeof chosen !== "string") return;

  try {
    await useWorkspaceStore.getState().addRepo(chosen);
  } catch (error) {
    toast.error(copy.errors.notAGitRepo(chosen), {
      description: errorMessage(error),
    });
  }
}
