export const workspaceCopy = {
  summary: {
    projects: (n: number) => (n === 1 ? "1 project" : `${n} projects`),
    worktrees: (n: number) => (n === 1 ? "1 worktree" : `${n} worktrees`),
    running: (n: number) =>
      n === 1 ? "1 agent running" : `${n} agents running`,
    needsYou: (n: number) =>
      n === 1 ? "1 agent needs you" : `${n} agents need you`,
    empty: "Nothing inside",
  },

  folders: {
    newFolder: "New folder",
    rename: "Rename…",
    moveTo: "Move to folder…",
    delete: "Delete folder…",
    actions: (name: string) => `Actions for folder ${name}`,
    collapse: (name: string) => `Collapse ${name}`,
    expand: (name: string) => `Expand ${name}`,
    empty: "Empty folder",
    createTitle: "New folder",
    renameTitle: "Rename folder",
    formDescription:
      "Folders group projects and other folders. They only change how the sidebar looks.",
    nameLabel: "Name",
    namePlaceholder: "Work",
    create: "Create folder",
    save: "Save",
    cancel: "Cancel",
    deleteTitle: (name: string) => `Delete ${name}?`,
    deleteBody:
      "Its folders and projects move up one level. Nothing is removed from disk.",
    deleteConfirm: "Delete folder",
    failed: "Could not save the folder",
    deleteLinkedBody:
      "The folder stops following its directory on disk. Its folders and projects move up one level. Nothing is removed from disk.",
    linkLabel: "Link to a folder on disk",
    linkHint:
      "Subfolders become folders here, and the first git repository on each path becomes a project. Nothing on disk changes.",
    linkChoose: "Choose folder…",
    linkChange: "Change folder…",
    linkClear: "Don't link",
    linkNone: "Not linked",
    linkChecking: "Looking at the folder…",
    linkPreview: (projects: number, folders: number, truncated: boolean) =>
      `${projects === 1 ? "1 project" : `${projects} projects`} in ${folders === 1 ? "1 folder" : `${folders} folders`}${truncated ? " or more" : ""}`,
    linkNothingYet: "No projects there yet. New ones appear on their own.",
    linkedTo: (path: string) => `Linked to ${path}`,
    notFound: "Not found",
    notFoundHint: (path: string) => `${path} was not found on disk`,
    rescan: "Rescan folder",
    rescanned: "Folder is up to date",
    rescanFound: "Found changes on disk",
    unlink: "Unlink from disk…",
    unlinkTitle: (name: string) => `Unlink ${name}?`,
    unlinkBody:
      "It stops following its directory. The folders and projects it made stay here as ordinary ones. Nothing is removed from disk.",
    unlinkConfirm: "Unlink",
    unlinkFailed: "Could not unlink the folder",
    syncFailed: "Could not read the linked folders",
    createLinked: "Create and link",
  },

  move: {
    title: (name: string) => `Move ${name}`,
    description: "Choose where it goes in the sidebar.",
    topLevel: "Top level",
    current: "current",
    cannotMoveInside: "inside itself",
    confirm: "Move here",
    failed: "Could not move it",
  },

  sidebar: {
    title: "Projects",
    addProject: "Add project",
    toggle: "Toggle sidebar",
    emptyTitle: "No projects yet",
    emptyBody: "Add a project to see its worktrees.",
    loadError: "Could not load your repositories",
    filterLabel: "Which projects",
    filterAll: "All",
    filterActive: "Active",
    activeEmpty: "No project has a running terminal.",
  },

  addProject: {
    title: "Add project",
    description:
      "Start from a git repository already on this computer, or create an empty one.",
    browse: "Browse project",
    browseDescription:
      "Pick a git repository that already exists on this computer.",
    create: "Create new project",
    createDescription: "Start an empty git repository in a folder you choose.",
    createSummary: "Name the project and choose the folder it is created in.",
    nameLabel: "Project name",
    namePlaceholder: "my-project",
    locationLabel: "Location",
    noLocation: "No folder chosen",
    chooseLocation: "Choose folder…",
    locationHint: "The project folder is created inside this location.",
    back: "Back",
    submit: "Create project",
  },

  repo: {
    moveToFolder: "Move to folder…",
    newWorktree: "New worktree",
    refresh: "Refresh worktrees",
    removeFromList: "Remove from list",
    actions: (name: string) => `Actions for ${name}`,
    collapse: (name: string) => `Collapse ${name}`,
    expand: (name: string) => `Expand ${name}`,
    missing: "Not found",
    missingHint: (path: string) => `${path} was not found on disk`,
  },

  agent: {
    title: "Agents",
    status: {
      working: "working",
      needsYou: "needs you",
      done: "done",
      idle: "idle",
    },
    group: (count: number, running: number) => {
      const agents = count === 1 ? "1 agent" : `${count} agents`;
      return running > 0 ? `${agents}, ${running} running` : agents;
    },
    exited: (code: number | null) =>
      code === null ? "exited" : `exited with code ${code}`,
    label: (title: string, status: string) => `${title}, ${status}`,
    close: (title: string) => `Close ${title}`,
  },

  worktree: {
    primary: "primary",
    detached: "detached",
    creating: (name: string) => `Creating ${name}…`,
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
    searchBranches: "Search branches…",
    noBranches: "No branches match.",
    moreBranches: (shown: number, total: number) =>
      `Showing ${shown} of ${total}. Type to narrow the list.`,
    create: "Create worktree",
    cancel: "Cancel",
  },

  cleanUp: {
    merged: "This pull request is merged.",
    button: "Clean up worktree",
    title: (name: string) => `Clean up ${name}?`,
    body: (branch: string) =>
      `The pull request is merged, so this removes the worktree folder and its branch ${branch}, and closes its terminals. It stops if there are uncommitted changes.`,
    confirm: "Remove worktree and branch",
    cancel: "Cancel",
    cleaned: (name: string) => `${name} was cleaned up.`,
    branchKept: (branch: string, reason: string) =>
      `Branch ${branch} was kept: ${reason}.`,
    hasChanges:
      "This worktree has uncommitted changes, so it was not removed. Commit or discard them, or remove it from the sidebar.",
    failed: "Could not clean up the worktree",
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
    linkedBody:
      "Nothing is deleted from disk. It was found by a linked folder, so it stays out of the list until you unlink that folder.",
    confirm: "Remove from list",
    cancel: "Cancel",
  },

  emptyWorktree: {
    title: (name: string) => `Nothing is running in ${name}`,
    body: "Start an agent or open a shell.",
  },

  welcome: {
    title: "Add a project to get started",
    body: "Each task gets its own git worktree, with its agents and terminals next to it.",
  },

  resumable: {
    ended: "Ended",
    resume: "Resume this agent",
    failed: "Could not resume the agent.",
    remove: "Remove from the list",
    removeFailed: "Could not remove it from the list.",
  },
} as const;
