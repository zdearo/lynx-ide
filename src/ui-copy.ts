/** Every user-facing string lives here so the interface can be translated later. */
export const copy = {
  appName: "agent-ade",

  sidebar: {
    title: "Repositories",
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
  },

  errors: {
    notAGitRepo: (path: string) => `${path} is not a git repository.`,
    generic: "Something went wrong",
  },
} as const;
