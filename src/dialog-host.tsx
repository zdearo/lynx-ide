import { lazy, Suspense, useEffect } from "react";

import { useDialogStore } from "@/dialog-store";
import { useNavigationStore } from "@/navigation/navigation-store";
import { useWorkspaceStore } from "@/workspace/workspace-store";

// Each dialog's code loads the first time it opens, not at startup.
const SettingsDialog = lazy(() =>
  import("@/settings/SettingsDialog").then((m) => ({
    default: m.SettingsDialog,
  })),
);
const AddProjectDialog = lazy(() =>
  import("@/workspace/AddProjectDialog").then((m) => ({
    default: m.AddProjectDialog,
  })),
);
const CloseTerminalDialog = lazy(() =>
  import("@/terminal/CloseTerminalDialog").then((m) => ({
    default: m.CloseTerminalDialog,
  })),
);
const CustomCommandDialog = lazy(() =>
  import("@/terminal/CustomCommandDialog").then((m) => ({
    default: m.CustomCommandDialog,
  })),
);
const FolderDialog = lazy(() =>
  import("@/workspace/FolderDialog").then((m) => ({ default: m.FolderDialog })),
);
const MoveDialog = lazy(() =>
  import("@/workspace/MoveDialog").then((m) => ({ default: m.MoveDialog })),
);
const DeleteFolderDialog = lazy(() =>
  import("@/workspace/DeleteFolderDialog").then((m) => ({
    default: m.DeleteFolderDialog,
  })),
);
const NewWorktreeDialog = lazy(() =>
  import("@/workspace/NewWorktreeDialog").then((m) => ({
    default: m.NewWorktreeDialog,
  })),
);
const RemoveWorktreeDialog = lazy(() =>
  import("@/workspace/RemoveWorktreeDialog").then((m) => ({
    default: m.RemoveWorktreeDialog,
  })),
);
const RemoveRepoDialog = lazy(() =>
  import("@/workspace/RemoveRepoDialog").then((m) => ({
    default: m.RemoveRepoDialog,
  })),
);

// Once the app is idle, fetch the dialogs' code, so the first open does not wait for it.
const DIALOG_MODULES = [
  () => import("@/settings/SettingsDialog"),
  () => import("@/workspace/AddProjectDialog"),
  () => import("@/terminal/CloseTerminalDialog"),
  () => import("@/terminal/CustomCommandDialog"),
  () => import("@/workspace/FolderDialog"),
  () => import("@/workspace/MoveDialog"),
  () => import("@/workspace/DeleteFolderDialog"),
  () => import("@/workspace/NewWorktreeDialog"),
  () => import("@/workspace/RemoveWorktreeDialog"),
  () => import("@/workspace/RemoveRepoDialog"),
];

function prefetchDialogs(): void {
  const load = () => {
    void Promise.all(DIALOG_MODULES.map((load) => load())).catch(
      () => undefined,
    );
  };
  if ("requestIdleCallback" in window) window.requestIdleCallback(load);
  else setTimeout(load, 1500);
}

/**
 * Mounts only the dialogs that are open. Each one mounts afresh on every open, so
 * it starts from its own initial state, and a closed dialog's code is never loaded.
 */
export function DialogHost() {
  useEffect(prefetchDialogs, []);

  const settingsOpen = useNavigationStore((state) => state.settingsOpen);
  const addProjectOpen = useDialogStore((state) => state.addProjectOpen);
  const closeTerminalId = useDialogStore((state) => state.closeTerminalId);
  const customCommandFor = useDialogStore((state) => state.customCommandFor);
  const folderDialog = useDialogStore((state) => state.folderDialog);
  const moveTarget = useDialogStore((state) => state.moveTarget);
  const deleteFolderTarget = useDialogStore(
    (state) => state.deleteFolderTarget,
  );
  const removeWorktreeTarget = useDialogStore(
    (state) => state.removeWorktreeTarget,
  );
  const removeRepoTarget = useDialogStore((state) => state.removeRepoTarget);
  const newWorktreeDraft = useWorkspaceStore((state) => state.draft);

  return (
    <Suspense fallback={null}>
      {settingsOpen && <SettingsDialog />}
      {addProjectOpen && <AddProjectDialog />}
      {closeTerminalId !== null && <CloseTerminalDialog />}
      {customCommandFor !== null && <CustomCommandDialog />}
      {folderDialog !== null && <FolderDialog />}
      {moveTarget !== null && <MoveDialog />}
      {deleteFolderTarget !== null && <DeleteFolderDialog />}
      {newWorktreeDraft !== null && <NewWorktreeDialog />}
      {removeWorktreeTarget !== null && <RemoveWorktreeDialog />}
      {removeRepoTarget !== null && <RemoveRepoDialog />}
    </Suspense>
  );
}
