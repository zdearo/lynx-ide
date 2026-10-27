export const shellCopy = {
  navigation: {
    label: "Primary navigation",
    projects: "Projects",
    bots: "Bots",
    settings: "Settings",
  },

  rightPanel: {
    label: "Side panel",
    tabs: "Side panel tabs",
    toggle: "Toggle side panel",
  },

  shortcuts: {
    newTerminal: "New terminal",
    closeTerminal: "Close terminal",
    newWorktree: "New worktree",
    toggleSidebar: "Toggle sidebar",
    openSettings: "Open settings",
    toggleFilesPanel: "Toggle files panel",
  },

  errors: {
    notAGitRepo: (path: string) => `${path} is not a git repository.`,
    generic: "Something went wrong",
  },

  quit: {
    title: "Close Lynx?",
    body: (sessions: number) =>
      sessions === 1
        ? "One terminal is still running in the background."
        : `${sessions} terminals are still running in the background.`,
    keep: "Keep them running",
    end: "End them",
    cancel: "Cancel",
  },
} as const;
