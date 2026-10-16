/** Every user-facing string lives here so the interface can be translated later. */
export const copy = {
  appName: "Lynx",

  navigation: {
    label: "Primary navigation",
    projects: "Projects",
    settings: "Settings",
  },

  settings: {
    title: "Settings",
    description: "App settings",
    close: "Close settings",
    sections: "Settings sections",
    reset: "Reset to default",
    terminal: {
      title: "Terminal",
      fontFamily: {
        label: "Font",
        hint: "Comma-separated, like CSS. Tried before the bundled JetBrains Mono, so a Nerd Font here keeps your prompt icons.",
        placeholder: "JetBrains Mono (bundled)",
      },
      fontSize: { label: "Font size", hint: "From 8 to 32." },
      shell: {
        label: "Shell",
        hint: "Program for new terminals. Leave empty to use your login shell. Open terminals keep theirs.",
        placeholder: "Login shell",
      },
      scrollback: {
        label: "Scrollback",
        hint: "Lines of history kept per terminal, up to 100000.",
      },
    },
    worktrees: {
      title: "Worktrees",
      directory: {
        label: "Worktrees folder",
        hint: "New worktrees are created in <folder>/<repository>/<name>. Leave empty to create them next to each repository. Existing worktrees do not move.",
        placeholder: "Next to each repository",
      },
    },
    file: {
      label: "Config file",
      open: "Open config file",
      openFailed: "Could not open the config file",
      brokenTitle: "The config file is not valid JSON",
      brokenBody:
        "Fix it in your editor. Settings cannot be saved until then, and nothing in the file has been changed.",
      issuesTitle: "Some values in the config file were ignored",
      usingDefault: "using the default",
    },
    invalidNumber: "Enter a whole number",
  },

  rightPanel: {
    label: "Side panel",
    tabs: "Side panel tabs",
    toggle: "Toggle side panel",
  },

  files: {
    title: "Files",
    tree: "Files in the selected worktree",
    refresh: "Refresh",
    collapseAll: "Collapse all",
    noWorktree: "Select a worktree to browse its files.",
    loading: "Loading files…",
    loadFailed: "Could not read the files",
    retry: "Retry",
    actions: (name: string) => `Actions for ${name}`,
    copyPath: "Copy path",
    copyRelativePath: "Copy relative path",
    sendToTerminal: "Send path to terminal",
    openFolder: "Open containing folder",
    copied: "Path copied",
    copyFailed: "Could not copy to the clipboard",
    noTerminal: "Open a terminal in this worktree first.",
    openFolderFailed: "Could not open the folder",
    ignored: "ignored by Git",
    link: "link",
  },

  sidebar: {
    title: "Projects",
    openRepository: "Open repository",
    toggle: "Toggle sidebar",
    emptyTitle: "No repository open",
    emptyBody: "Open a git repository to see its worktrees.",
    loadError: "Could not load your repositories",
  },

  repo: {
    newWorktree: "New worktree",
    refresh: "Refresh worktrees",
    removeFromList: "Remove from list",
    actions: (name: string) => `Actions for ${name}`,
    collapse: (name: string) => `Collapse ${name}`,
    expand: (name: string) => `Expand ${name}`,
  },

  worktree: {
    primary: "primary",
    detached: "detached",
    creating: (name: string) => `Creating ${name}…`,
    from: (base: string) => `from ${base}`,
    missingDirectory: "Folder is missing",
    locked: "Locked",
    newTerminal: "New terminal",
    actions: (name: string) => `Actions for ${name}`,
    remove: "Remove worktree…",
    runningTerminals: (count: number) =>
      count === 1 ? "1 running terminal" : `${count} running terminals`,
    noTerminals: "No running terminals",
  },

  newWorktree: {
    title: "New worktree",
    description: "Creates a branch and a folder next to the repository.",
    nameLabel: "Name",
    nameHint: "Used for the branch and the folder.",
    namePlaceholder: "feature/login",
    baseLabel: "Start from",
    loadingRefs: "Loading branches…",
    create: "Create worktree",
    cancel: "Cancel",
  },

  removeWorktree: {
    title: (name: string) => `Remove ${name}?`,
    body: "This deletes the worktree folder and closes its terminals. The branch is deleted only when all of its commits are merged.",
    confirm: "Remove",
    changesTitle: "This worktree has uncommitted changes",
    changesBody: "Removing it now will discard them. This cannot be undone.",
    confirmForce: "Discard changes and remove",
    cancel: "Cancel",
    branchKept: (branch: string, reason: string) =>
      `Branch ${branch} was kept: ${reason}.`,
    failed: "Could not remove the worktree",
  },

  removeRepo: {
    title: (name: string) => `Remove ${name} from the list?`,
    body: "Nothing is deleted from disk. Its worktrees stay where they are.",
    confirm: "Remove from list",
    cancel: "Cancel",
  },

  terminal: {
    newTerminal: "New terminal",
    shell: "Shell",
    customCommand: "Custom command…",
    close: "Close terminal",
    exited: (code: number | null) =>
      code === null ? "exited" : `exited ${code}`,
    createFailed: "Could not start the terminal",
  },

  customCommand: {
    title: "Run a command",
    description: "It is typed into a new shell in this worktree.",
    label: "Command",
    placeholder: "claude --resume",
    run: "Run",
    cancel: "Cancel",
  },

  emptyWorktree: {
    title: (name: string) => `Nothing is running in ${name}`,
    body: "Start an agent or open a shell.",
  },

  welcome: {
    title: "Open a repository to get started",
    body: "Each task gets its own git worktree, with its agents and terminals next to it.",
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
} as const;
