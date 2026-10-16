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
      tabs: {
        label: "Terminal settings",
        text: "Text",
        appearance: "Appearance",
        behavior: "Behavior",
      },
      colorScheme: {
        label: "Theme",
        hint: "The terminal's background. Its colours also become the text colours, unless you pick others below.",
      },
      themes: {
        default: "Lynx dark (default)",
        light: "Light",
        solarizedDark: "Solarized dark",
        solarizedLight: "Solarized light",
        dracula: "Dracula",
        nord: "Nord",
        catppuccinLatte: "Catppuccin Latte",
        catppuccinFrappe: "Catppuccin Frappé",
        catppuccinMacchiato: "Catppuccin Macchiato",
        catppuccinMocha: "Catppuccin Mocha",
        gruvboxDark: "Gruvbox dark",
        oneDark: "One Dark",
        tokyoNight: "Tokyo Night",
        monokai: "Monokai",
      },
      textColours: {
        label: "Text colours",
        hint: "The text colour and the normal and bright ANSI colours programs print with. Follows the theme unless you pick another one.",
        sameAsTheme: "Same as the theme",
      },
      fontFamily: {
        label: "Font",
        hint: "Tried before the bundled JetBrains Mono, so a Nerd Font here keeps your prompt icons.",
        bundled: "JetBrains Mono (bundled)",
        sfMono: "SF Mono (macOS)",
        cascadia: "Cascadia Mono (Windows)",
        fira: "Fira Code",
        custom: "Custom…",
        placeholder: "Comma-separated, like CSS",
      },
      fontSize: {
        label: "Font size",
        hint: "From 8 to 32.",
      },
      scrollback: {
        label: "Scrollback",
        hint: "Lines of history kept per terminal.",
      },
      shell: {
        label: "Shell",
        hint: "Program for new terminals. Open terminals keep theirs.",
        login: "Login shell",
        custom: "Custom path…",
        placeholder: "/path/to/shell",
      },
      copyOnSelect: {
        label: "Copy selection on release",
        hint: "Copies text to the clipboard as soon as the mouse is released over it.",
      },
      bell: {
        label: "Bell",
        hint: "What happens when a program rings the terminal bell.",
        none: "Nothing",
        visual: "Flash the terminal",
      },
      optionAsMeta: {
        label: "Option key as Meta",
        hint: "macOS only. Makes Option act as Alt, which Emacs and readline shortcuts use.",
      },
      closeOnExit: {
        label: "Close tab when the program exits",
        hint: "Off keeps the last output on screen, so you can see why it stopped.",
      },
      confirmClose: {
        label: "Confirm before closing a running tab",
        hint: "Asks before closing a tab whose process is still alive, even an idle shell.",
      },
    },
    agents: {
      title: "Agents",
      intro:
        "Turn an agent on to let Lynx follow its sessions and offer it in the new terminal menu. Turning it on adds hooks to the agent's own settings, after you confirm.",
      detected: "On this computer",
      undetected: "Other supported agents",
      notFound: "Not found on this computer",
      refresh: "Check again",
      use: (name: string) => `Use ${name} in Lynx`,
      failed: "Could not update the agent settings",
      harnesses: {
        claude: { name: "Claude Code" },
        codex: { name: "Codex" },
        opencode: { name: "OpenCode" },
        antigravity: { name: "Antigravity" },
        cursor: { name: "Cursor" },
        copilot: { name: "GitHub Copilot" },
        grok: { name: "Grok" },
        droid: { name: "Droid" },
        kiro: { name: "Kiro" },
        qwen: { name: "Qwen Code" },
        pi: { name: "Pi" },
        omp: { name: "OMP" },
      },
      hooks: {
        states: {
          partial: "Partly on",
          error: "Settings cannot be read",
        },
        eventsLabel: "Events",
        connectTitle: (name: string) => `Turn on ${name}?`,
        updateTitle: (name: string) => `Update ${name}?`,
        disconnectTitle: (name: string) => `Turn off ${name}?`,
        connectBody: (path: string) =>
          `Lynx adds its entries to ${path}. Your other settings and hooks stay as they are.`,
        updateBody: (path: string) =>
          `Lynx rewrites its own entries in ${path} so they match this version. Your other settings and hooks stay as they are.`,
        disconnectBody: (path: string) =>
          `Lynx removes only its own entries from ${path}.`,
        cancel: "Cancel",
        confirmConnect: "Turn on",
        confirmUpdate: "Update",
        confirmDisconnect: "Turn off",
      },
    },
    worktrees: {
      title: "Worktrees",
      directory: {
        label: "Worktrees folder",
        hint: "New worktrees are created in <folder>/<repository>/<name>. Leave empty to create them next to each repository. Existing worktrees do not move.",
        placeholder: "Next to each repository",
        browse: "Choose folder…",
      },
    },
    decrease: "Decrease",
    increase: "Increase",
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

  summary: {
    projects: (n: number) => (n === 1 ? "1 project" : `${n} projects`),
    worktrees: (n: number) => (n === 1 ? "1 worktree" : `${n} worktrees`),
    running: (n: number) =>
      n === 1 ? "1 agent running" : `${n} agents running`,
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
    closeDialog: {
      title: "Close this terminal?",
      body: "Whatever is running in it will be stopped.",
      keep: "Keep it open",
      confirm: "Close terminal",
    },
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
    title: "Add a project to get started",
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
