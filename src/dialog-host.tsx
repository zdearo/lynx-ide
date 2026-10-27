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
  import("@/workspace/projects/AddProjectDialog").then((m) => ({
    default: m.AddProjectDialog,
  })),
);
const BotWizard = lazy(() =>
  import("@/bots/BotWizard").then((m) => ({ default: m.BotWizard })),
);
const CloseTerminalDialog = lazy(() =>
  import("@/terminal/CloseTerminalDialog").then((m) => ({
    default: m.CloseTerminalDialog,
  })),
);
const DiscardDialog = lazy(() =>
  import("@/changes/DiscardDialog").then((m) => ({
    default: m.DiscardDialog,
  })),
);
const CreateRemoteDialog = lazy(() =>
  import("@/pull-request/CreateRemoteDialog").then((m) => ({
    default: m.CreateRemoteDialog,
  })),
);
const CloseFileDialog = lazy(() =>
  import("@/editor/CloseFileDialog").then((m) => ({
    default: m.CloseFileDialog,
  })),
);
const CustomCommandDialog = lazy(() =>
  import("@/terminal/CustomCommandDialog").then((m) => ({
    default: m.CustomCommandDialog,
  })),
);
const FolderDialog = lazy(() =>
  import("@/workspace/projects/FolderDialog").then((m) => ({
    default: m.FolderDialog,
  })),
);
const MoveDialog = lazy(() =>
  import("@/workspace/projects/MoveDialog").then((m) => ({
    default: m.MoveDialog,
  })),
);
const QuitDialog = lazy(() =>
  import("@/terminal/QuitDialog").then((m) => ({ default: m.QuitDialog })),
);
const DeleteFolderDialog = lazy(() =>
  import("@/workspace/projects/DeleteFolderDialog").then((m) => ({
    default: m.DeleteFolderDialog,
  })),
);
const UnlinkFolderDialog = lazy(() =>
  import("@/workspace/projects/UnlinkFolderDialog").then((m) => ({
    default: m.UnlinkFolderDialog,
  })),
);
const NewWorktreeDialog = lazy(() =>
  import("@/workspace/worktrees/NewWorktreeDialog").then((m) => ({
    default: m.NewWorktreeDialog,
  })),
);
const CleanUpMergedDialog = lazy(() =>
  import("@/pull-request/CleanUpMergedDialog").then((m) => ({
    default: m.CleanUpMergedDialog,
  })),
);
const RemoveWorktreeDialog = lazy(() =>
  import("@/workspace/worktrees/RemoveWorktreeDialog").then((m) => ({
    default: m.RemoveWorktreeDialog,
  })),
);
const RemoveRepoDialog = lazy(() =>
  import("@/workspace/projects/RemoveRepoDialog").then((m) => ({
    default: m.RemoveRepoDialog,
  })),
);

// Once the app is idle, fetch the dialogs' code, so the first open does not wait for it.
const DIALOG_MODULES = [
  () => import("@/settings/SettingsDialog"),
  () => import("@/workspace/projects/AddProjectDialog"),
  () => import("@/terminal/CloseTerminalDialog"),
  () => import("@/editor/CloseFileDialog"),
  () => import("@/pull-request/CreateRemoteDialog"),
  () => import("@/changes/DiscardDialog"),
  () => import("@/terminal/CustomCommandDialog"),
  () => import("@/workspace/projects/FolderDialog"),
  () => import("@/workspace/projects/MoveDialog"),
  () => import("@/workspace/projects/DeleteFolderDialog"),
  () => import("@/workspace/projects/UnlinkFolderDialog"),
  () => import("@/workspace/worktrees/NewWorktreeDialog"),
  () => import("@/workspace/worktrees/RemoveWorktreeDialog"),
  () => import("@/workspace/projects/RemoveRepoDialog"),
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
  const settingsSection = useNavigationStore((state) => state.settingsSection);
  const addProjectOpen = useDialogStore((state) => state.addProjectOpen);
  const botWizardOpen = useDialogStore((state) => state.botWizardOpen);
  const closeTerminalId = useDialogStore((state) => state.closeTerminalId);
  const closeFileTarget = useDialogStore((state) => state.closeFileTarget);
  const createRemoteFor = useDialogStore((state) => state.createRemoteFor);
  const discardTarget = useDialogStore((state) => state.discardTarget);
  const customCommandFor = useDialogStore((state) => state.customCommandFor);
  const folderDialog = useDialogStore((state) => state.folderDialog);
  const moveTarget = useDialogStore((state) => state.moveTarget);
  const quitSessions = useDialogStore((state) => state.quitSessions);
  const deleteFolderTarget = useDialogStore(
    (state) => state.deleteFolderTarget,
  );
  const unlinkFolderTarget = useDialogStore(
    (state) => state.unlinkFolderTarget,
  );
  const removeWorktreeTarget = useDialogStore(
    (state) => state.removeWorktreeTarget,
  );
  const cleanUpTarget = useDialogStore((state) => state.cleanUpTarget);
  const removeRepoTarget = useDialogStore((state) => state.removeRepoTarget);
  const newWorktreeDraft = useWorkspaceStore((state) => state.draft);

  return (
    <Suspense fallback={null}>
      {settingsOpen && <SettingsDialog key={settingsSection ?? "default"} />}
      {addProjectOpen && <AddProjectDialog />}
      {botWizardOpen && <BotWizard />}
      {closeTerminalId !== null && <CloseTerminalDialog />}
      {closeFileTarget !== null && <CloseFileDialog />}
      {createRemoteFor !== null && <CreateRemoteDialog />}
      {discardTarget !== null && <DiscardDialog />}
      {customCommandFor !== null && <CustomCommandDialog />}
      {folderDialog !== null && <FolderDialog />}
      {moveTarget !== null && <MoveDialog />}
      {deleteFolderTarget !== null && <DeleteFolderDialog />}
      {quitSessions !== null && <QuitDialog />}
      {unlinkFolderTarget !== null && <UnlinkFolderDialog />}
      {newWorktreeDraft !== null && <NewWorktreeDialog />}
      {removeWorktreeTarget !== null && <RemoveWorktreeDialog />}
      {cleanUpTarget !== null && <CleanUpMergedDialog />}
      {removeRepoTarget !== null && <RemoveRepoDialog />}
    </Suspense>
  );
}
