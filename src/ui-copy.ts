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
      tabs: { label: "Agent settings", setup: "Setup" },
      intro:
        "Turn an agent on to let Lynx follow its sessions, offer it in the new terminal menu and give it the browser tools. Turning it on adds Lynx's own entries to the agent's settings, and turning it off removes them.",
      detected: "On this computer",
      undetected: "Other supported agents",
      notFound: "Not found on this computer",
      refresh: "Check again",
      use: (name: string) => `Use ${name} in Lynx`,
      failed: "Could not update the agent settings",
      partlyDone: "The agent is on, but one part did not go in",
      launchMode: {
        label: "Launch mode",
        hint: "Manual keeps each agent's permission prompts. YOLO starts every agent with the flag that skips them, after you confirm.",
        manual: "Manual",
        yolo: "YOLO",
        cancel: "Cancel",
        confirmTitle: "Start agents in YOLO mode?",
        confirmBody:
          "Each agent you have turned on starts without asking before it runs tools. Terminals opened from now on use it; sessions already running do not change.",
        confirm: "Turn on YOLO",
      },
      launchArguments: {
        toggle: (name: string) => `Launch arguments for ${name}`,
        hint: "Typed after the agent's command each time a new terminal starts it.",
        text: (name: string) => `Arguments for ${name}`,
        placeholder: "For example: --model opus",
        yoloFlag: (flag: string) => `YOLO mode adds ${flag} before them.`,
        noYoloFlag:
          "This agent has no known YOLO flag, so it starts in manual mode.",
      },
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
    browser: {
      title: "Browser",
      engine: {
        label: "Browser engine",
        hint: "The browser runs as a separate program, downloaded once. It is not part of the app.",
        missing: "Not installed",
        ready: (version: string | null) =>
          version ? `Installed (${version})` : "Installed",
        outdated: "Installed, but made for another version of Lynx",
        install: "Download",
        update: "Update",
        reinstall: "Download again",
        remove: "Remove",
        removing: "Removing…",
        failed: "The engine could not be changed",
      },
      access: {
        label: "What agents can do in the browser",
        hint: "Agents you have turned on get the browser tools. Each agent works in its own tab, listed in the Browser panel. Read only lets them open and read pages but not click, type or choose.",
        off: "Nothing",
        readOnly: "Read only",
        full: "Full control",
      },
      sites: {
        label: "Sites agents may visit",
        hint: "One site per line, like example.com. A site includes its subdomains. Leave empty to allow any site.",
        placeholder: "example.com",
      },
    },
    notifications: {
      title: "Notifications",
      intro:
        "Lynx tells you through the system when an agent needs you or finishes, but only while its window is not in front. Your system must allow notifications for Lynx.",
      done: {
        label: "Agent finished",
        hint: "Notify when an agent finishes its work.",
      },
      needsYou: {
        label: "Agent needs you",
        hint: "Notify when an agent waits for an answer or a permission.",
      },
    },
    usage: {
      title: "Plan limits",
      intro:
        "How much of each AI plan you have used, shown in the footer. Lynx reads the numbers from the CLI you already use and sends nothing to a model.",
      visible: {
        label: "Show in the footer",
        hint: "Off also stops Lynx from reading the limits.",
      },
      showNames: {
        label: "Show plan names",
        hint: "Off leaves the mark, the bar and the number. The name stays in the details.",
      },
      providers: {
        label: "Plans",
        hint: "A plan shows when its CLI is installed and signed in.",
        claude: "Claude Code",
        claudeHint: "Its 5-hour and weekly windows.",
        codex: "Codex",
        codexHint: "Its plan window.",
      },
      display: {
        label: "Percentages",
        hint: "Show how much is used, or how much is left.",
        used: "Used",
        remaining: "Left",
      },
      layout: {
        label: "Footer layout",
        hint: "Compact shows one number per plan. Detailed shows every window with its bar.",
        compact: "Compact",
        detailed: "Detailed",
      },
      warnAt: {
        label: "Getting close at",
        hint: "From this share used, the bar warns.",
      },
      criticalAt: {
        label: "Almost used up at",
        hint: "From this share used, the bar shows its strongest warning.",
      },
      levels: (warnAt: number, criticalAt: number) =>
        `Calm below ${warnAt}%, getting close from ${warnAt}%, almost used up from ${criticalAt}%.`,
    },
    git: {
      title: "Git",
      agent: {
        label: "Commit message agent",
        hint: "The agent CLI that writes a commit message from the staged changes. Automatic uses the first one installed: Claude Code, then Codex, then OpenCode.",
        auto: "Automatic",
        claude: "Claude Code",
        codex: "Codex",
        opencode: "OpenCode",
      },
      style: {
        label: "Commit message style",
        hint: "How the message is written. Lynx's output rules always apply after this, so the message can still be read.",
        conventional: "Conventional Commits",
        plain: "Plain",
        custom: "Custom",
      },
      instructions: {
        label: "Custom instructions",
        hint: "Used when the style is Custom. Describe the format you want; Lynx adds its own output rules after your text.",
        placeholder:
          "e.g. Write the subject in Portuguese, in the imperative mood.",
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

  browser: {
    toggle: "Browser",
    tab: "Browser",
    panel: {
      title: "Browser",
      newTab: "New tab",
      count: (count: number) => (count === 1 ? "1 tab" : `${count} tabs`),
      empty: {
        title: "No browser tabs in this worktree",
        body: "Tabs you open here, and the ones agents open for it, appear in this list.",
      },
      loading: "Loading",
      loaded: "Loaded",
      untitled: "New tab",
      close: (title: string) => `Close ${title}`,
      working: (agent: string) => `${agent} is working`,
      inWindow: "In its own window",
    },
    agentNames: {
      claude: "Claude Code",
      codex: "Codex",
      opencode: "OpenCode",
      unknown: "Agent",
    },
    noTab: {
      title: "No tab is open",
      body: "Open a tab from the Browser panel, or start a new one here.",
      address: "Search or enter an address",
    },
    label: "Browser",
    address: "Web address",
    addressPlaceholder: "Search or enter an address",
    back: "Back",
    forward: "Forward",
    reload: "Reload",
    stop: "Stop loading",
    starting: "Starting the browser…",
    retry: "Try again",
    engine: {
      missingTitle: "The browser needs its engine",
      missingBody:
        "The browser runs as a separate program that is downloaded once and kept on this computer.",
      outdatedTitle: "The browser engine needs an update",
      outdatedBody:
        "The installed engine was made for another version of Lynx. Download the one that matches.",
      install: "Download the engine",
      update: "Update the engine",
      retry: "Try the download again",
      installing: "Downloading the engine",
      preparing: "Preparing…",
      downloading: (received: string, total: string) =>
        `Downloading ${received} of ${total}`,
      verifying: "Checking the download…",
      unpacking: "Unpacking…",
      done: "Installed",
      developing: "Building Lynx from source? Run:",
      developingCommand: "bun run browser:engine",
      failedTitle: "The engine could not be installed",
    },
    failedTitle: "The browser could not start",
    stopped: "The browser engine stopped.",
    detach: "Open in a separate window",
    attach: "Return to the main window",
    detachedTitle: "The browser is in its own window",
    detachedBody:
      "Close that window, or bring the browser back here, to see it in this one.",
    bringBack: "Bring it back",
    inControl: (agent: string) => `${agent} is in control`,
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
    containsChanges: "contains changed files",
  },

  editor: {
    tabs: "Open files",
    preview: "preview",
    toolbar: "File view",
    viewEdit: "Edit",
    viewDiff: "Diff",
    viewGroup: "View",
    modified: "modified",
    label: (name: string) => `Editor for ${name}`,
    close: "Close file",
    closeNamed: (name: string) => `Close ${name}`,
    unsaved: "unsaved changes",
    loading: "Opening file…",
    saving: "Saving…",
    saveFailed: "Could not save the file",
    openFailed: "Could not open the file",
    tooLarge: "This file is too large to edit here.",
    binary: "This file is not text, so it cannot be edited here.",
    notAFile: "This is not a regular file.",
    retry: "Retry",
    conflictTitle: "This file changed on disk",
    conflictBody: "Saving now would replace the newer version.",
    overwrite: "Overwrite",
    reload: "Reload from disk",
    closeDialog: {
      title: (name: string) => `Close ${name}?`,
      body: "Its unsaved changes will be lost.",
      keep: "Keep editing",
      confirm: "Close without saving",
    },
  },

  diff: {
    label: (name: string) => `Changes in ${name}`,
    loading: "Reading changes…",
    noChanges: "No changes against HEAD.",
    binary: "This file is not text, so its changes cannot be shown.",
    tooLarge: "This file is too large to compare here.",
    failed: "Could not read the changes",
    retry: "Retry",
    compareGroup: "Compare with",
    compare: {
      head: "HEAD",
      staged: "Staged",
      unstaged: "Unstaged",
      base: "Base",
    },
    compareHints: {
      head: "Everything changed since the last commit",
      staged: "What the next commit would hold",
      unstaged: "What is not staged yet",
      base: "Everything since the branch left the default branch",
    },
    commitTab: (hash: string) => `Commit ${hash.slice(0, 7)}`,
    modeGroup: "Diff layout",
    unified: "Unified",
    split: "Split",
    // CodeMirror puts the number of hidden lines where the $ is.
    unchanged: "$ unchanged lines",
  },

  commits: {
    list: "Commits on this branch",
    loading: "Reading commits…",
    empty: "No commits on this branch yet.",
    notPushed: "Not pushed",
    files: {
      loading: "Reading the files…",
      none: "No files in this commit.",
      failed: "Could not read the files of this commit",
      list: (hash: string) => `Files in commit ${hash.slice(0, 7)}`,
    },
    ago: {
      now: "just now",
      minutes: (count: number) => `${count} min ago`,
      hours: (count: number) => `${count} h ago`,
      days: (count: number) => `${count} d ago`,
    },
  },

  pullRequest: {
    tab: "Pull request",
    title: "Pull request",
    noWorktree: "Select a worktree to see its pull request.",
    loading: "Looking for the pull request…",
    refresh: "Refresh",
    retry: "Retry",
    branch: (name: string) => `Branch ${name}`,
    openOnGitHub: "Open on GitHub",
    openFailed: "Could not open the link",
    badge: {
      draft: "Draft",
      open: "Open",
      merged: "Merged",
      closed: "Closed",
    },
    review: {
      none: "No review yet",
      approved: "Approved",
      changesRequested: "Changes requested",
      reviewRequired: "Review required",
    },
    checks: {
      title: "Checks",
      none: "No checks on this pull request.",
      summary: (passed: number, failed: number, pending: number) =>
        `${passed} passed · ${failed} failed · ${pending} pending`,
      passed: "Passed",
      failed: "Failed",
      pending: "Pending",
      skipped: "Skipped",
      log: "Log",
      hideLog: "Hide log",
      open: "Open",
      logOf: (name: string) => `Log of ${name}`,
      openOf: (name: string) => `Open ${name} on GitHub`,
      logText: (name: string) => `Log text of ${name}`,
      logLoading: "Reading the log…",
      logFailed: "Could not read the log",
      logTruncated: "Only the end of the log is shown.",
      logEmpty: "The log is empty.",
    },
    error: {
      title: "Could not read the pull request",
      notInstalled: "Install the GitHub CLI (gh) to see pull requests here.",
      notLoggedIn: "Run gh auth login in a terminal, then refresh.",
      noRemote: "This repository has no GitHub remote.",
      internal: "Something went wrong while reading the pull request.",
    },
    create: {
      heading: "Open a pull request",
      base: "Base branch",
      titleLabel: "Pull request title",
      titlePlaceholder: "Title",
      bodyLabel: "Pull request description",
      bodyPlaceholder: "Description",
      draft: "Draft",
      generate: "Generate title and description with AI",
      generating: "Generating a title and description…",
      draftedWith: (agent: string) =>
        `Drafted with ${agent}. Review it before creating.`,
      submit: "Create pull request",
      pushAndSubmit: "Push and create",
      creating: "Creating…",
      pushHint: (branch: string, remote: string) =>
        `${branch} is pushed to ${remote} first.`,
      sameBranch: "Pick a base other than this branch.",
      noOtherBase: (branch: string) =>
        `${branch} is the only branch on the remote. Create a branch from it to open a pull request.`,
      created: "Pull request created",
      loadFailed: "Could not prepare the pull request",
      error: {
        notInstalled: "Install the GitHub CLI (gh) to create pull requests.",
        notLoggedIn: "Run gh auth login in a terminal, then try again.",
        invalid: "The title, description or base branch is not valid.",
        exists: "This branch already has a pull request. Refresh to see it.",
        noDifference: "This branch has no commits beyond its base.",
        failed: "Could not create the pull request",
        generateFailed: "Could not generate the text",
      },
    },
    discussion: {
      checksTab: "Checks",
      tab: "Discussion",
      viewGroup: "Show",
      composerLabel: "Add a comment",
      composerPlaceholder: "Leave a comment",
      send: "Comment",
      sending: "Sending…",
      sent: "Comment posted",
      cancel: "Cancel",
      failed: "Could not post the comment",
      loading: "Reading the conversation…",
      loadFailed: "Could not read the conversation",
      empty: "No comments yet.",
      threads: "Inline threads",
      conversation: "Conversation",
      reply: "Reply",
      replyLabel: (path: string) => `Reply in ${path}`,
      resolveThread: "Resolve thread",
      reopenThread: "Reopen thread",
      resolved: "Resolved",
      outdated: "Outdated",
      showMore: "Show more",
      showLess: "Show less",
      verdict: {
        approved: "approved",
        changesRequested: "requested changes",
        commented: "reviewed",
        dismissed: "had a review dismissed",
        pending: "has a pending review",
      },
    },
    actions: {
      mergeState: {
        clean: (base: string) => `Ready to merge into ${base}`,
        blocked: (base: string) =>
          `Merging into ${base} is blocked: a required check or review is missing`,
        conflicting: (base: string) => `Has conflicts with ${base}`,
        unstable: (base: string) =>
          `Merging into ${base}: checks are failing or still running`,
        unknown: (base: string) => `Merging into ${base}`,
      },
      methodLabel: "Merge method",
      methods: {
        squash: "Squash and merge",
        merge: "Create a merge commit",
        rebase: "Rebase and merge",
      },
      merge: "Merge",
      confirmMerge: "Confirm merge",
      mergeWarning: (number: number, base: string) =>
        `Merges #${number} into ${base}. This cannot be undone from here.`,
      autoMerge: "Enable auto-merge",
      autoMergeOn:
        "Auto-merge is on: GitHub merges it once the requirements pass.",
      autoMergeEnabled: "Auto-merge enabled",
      merged: "Pull request merged",
      markReady: "Ready for review",
      markedReady: "Marked ready for review",
      markDraft: "Convert to draft",
      markedDraft: "Converted to draft",
      close: "Close pull request",
      closed: "Pull request closed",
      reopen: "Reopen pull request",
      reopened: "Pull request reopened",
      failed: "Could not complete the action",
    },
    createRemote: {
      button: "Create remote",
      title: "Create a GitHub remote",
      description:
        "Creates a repository on GitHub and connects this repository to it as origin.",
      account: "Account",
      name: "Repository name",
      nameHint:
        "Letters, digits, hyphens, underscores and dots. The folder name is a good start.",
      visibility: "Visibility",
      visibilityOptions: {
        private: "Private",
        public: "Public",
      },
      visibilityHints: {
        private: "Only people you invite can see the code.",
        public: "Anyone on the internet can see the code.",
      },
      push: "Push this branch",
      pushHint:
        "Sends the current branch to the new repository, so its pull request can be found.",
      cancel: "Cancel",
      confirm: "Create repository",
      creating: "Creating…",
      created: "Remote created",
      loadFailed: "Could not read the GitHub account",
      error: {
        notInstalled: "Install the GitHub CLI (gh) to create repositories.",
        notLoggedIn: "Run gh auth login in a terminal, then try again.",
        invalidName:
          "The account or repository name has characters GitHub does not allow.",
        nameTaken:
          "A repository with that name already exists on this account.",
        remoteExists: "This repository already has a remote named origin.",
        failed: "Could not create the repository",
      },
    },
  },

  changes: {
    title: "Changes",
    list: "Changed files in the selected worktree",
    noWorktree: "Select a worktree to see its changes.",
    loading: "Reading changes…",
    loadFailed: "Could not read the changes",
    retry: "Retry",
    refresh: "Refresh",
    clean: "No changes in this worktree.",
    sections: {
      conflicts: "Merge conflicts",
      staged: "Staged changes",
      changes: "Changes",
      untracked: "Untracked files",
    },
    kinds: {
      added: "added",
      modified: "modified",
      deleted: "deleted",
      renamed: "renamed",
      copied: "copied",
      typeChanged: "type changed",
      untracked: "untracked",
      conflicted: "conflict",
    },
    renamedFrom: (path: string) => `renamed from ${path}`,
    stage: (name: string) => `Stage ${name}`,
    unstage: (name: string) => `Unstage ${name}`,
    stageFailed: "Could not stage the changes",
    stageAll: (section: string) => `Stage all ${section.toLowerCase()}`,
    unstageFailed: "Could not unstage the file",
    summaryLabel: "Commit summary",
    summaryPlaceholder: "Summary",
    descriptionLabel: "Commit description",
    descriptionPlaceholder: "Description",
    commit: "Commit",
    committing: "Committing…",
    commitFailed: "Could not commit",
    nothingStaged: "Stage a file to commit.",
    views: { label: "Show", changes: "Changes", commits: "Commits" },
    lines: (added: number, removed: number) =>
      `${added} lines added, ${removed} lines removed`,
    discard: {
      file: (name: string) => `Discard ${name}`,
      all: (section: string) => `Discard all ${section.toLowerCase()}`,
      title: (count: number) =>
        count === 1 ? "Discard this change?" : `Discard ${count} changes?`,
      body: "Changed files go back to their last staged or committed version, and untracked files are deleted. This cannot be undone.",
      confirm: "Discard",
      cancel: "Cancel",
      failed: "Could not discard the changes",
    },
    pull: "Pull",
    pulling: "Pulling…",
    pullFailed: "Could not pull",
    pulled: (remote: string) => `Pulled from ${remote}.`,
    commitsToPull: (count: number, remote: string) =>
      `${count} ${count === 1 ? "commit" : "commits"} to pull from ${remote}`,
    sync: "Sync",
    syncing: "Syncing…",
    syncFailed: "Could not sync",
    synced: (remote: string) => `Synced with ${remote}.`,
    divergedHint: (ahead: number, behind: number, remote: string) =>
      `${ahead} to push, ${behind} to pull. Sync puts your commits on top of ${remote}'s.`,
    forcePush: "Force push",
    forcePushAnyway: "Force push anyway",
    forcePushing: "Pushing…",
    forcePushFailed: "Could not force push",
    forcePushed: (remote: string) => `Replaced the branch on ${remote}.`,
    forcePushHint: (remote: string) =>
      `${remote} holds old copies of commits you rewrote. A plain push is refused.`,
    forcePushWarning: (count: number, remote: string) =>
      `This replaces ${count} ${count === 1 ? "commit" : "commits"} on ${remote} with your rewritten history. If someone pushed in the meantime, git refuses it.`,
    push: "Push",
    pushing: "Pushing…",
    pushFailed: "Could not push",
    pushed: (remote: string) => `Pushed to ${remote}.`,
    commitsToPush: (count: number, remote: string) =>
      `${count} ${count === 1 ? "commit" : "commits"} to push to ${remote}`,
    publishTo: (remote: string) =>
      `This branch is not on ${remote} yet. Push creates it there.`,
    amend: {
      label: "Amend last commit",
      button: "Amend",
      anyway: "Amend anyway",
      pushedWarning: (remote: string) =>
        `That commit is already on ${remote}. Amending rewrites it, and the branch will need a force push.`,
    },
    generate: "Generate message with AI",
    generating: "Generating a message…",
    generateNeedsStaged:
      "Stage the files you want to commit, then generate the message.",
    generateFailed: "Could not generate a message",
    draftedWith: (agent: string) =>
      `Drafted with ${agent}. Review it before committing.`,
  },

  notifications: {
    done: (agent: string) => `${agent} finished`,
    needsYou: (agent: string) => `${agent} needs you`,
  },

  fixWithAi: {
    button: "Fix with AI",
    hint: "Starts an agent in this worktree with the error and what to do.",
    noAgent:
      "Turn on Claude Code, Codex or OpenCode in Agents settings to fix this with an agent.",
    started: (agent: string) => `${agent} is on it. Follow it in the new tab.`,
    failed: "Could not start the agent",
    fixChecks: "Fix broken checks",
  },

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

  planLimits: {
    title: "Plan limits",
    refresh: "Refresh plan limits",
    retry: "Try again",
    signIn: "Sign in",
    unavailable: "Unavailable",
    unavailableTitle: "Limits could not be read",
    signInDetail: (name: string) =>
      `Sign in to ${name} to see its plan limits.`,
    signInHint: {
      claude: "Run claude in a terminal, then /login.",
      codex: "Run codex login in a terminal.",
    },
    unavailableSummary: (name: string) => `${name}: plan limits unavailable`,
    summary: (name: string, shown: string, window: string, reset: string) =>
      `${name}: ${shown}, ${window}. ${reset}.`,
    percentUsed: (percent: number) => `${percent}% used`,
    percentLeft: (percent: number) => `${percent}% left`,
    pace: {
      runsOut: (duration: string) => `At this pace: full in ${duration}`,
      holds: "At this pace: lasts until the reset",
    },
    readAt: (ago: string) => `Read ${ago}`,
    source: {
      claude: "Read from Claude Code. No tokens spent.",
      codex: "Read from the Codex CLI. No tokens spent.",
    },
    resets: {
      unknown: "Reset time unknown",
      now: "Resets now",
      in: (duration: string) => `Resets in ${duration}`,
    },
    duration: {
      minutes: (minutes: number) => `${minutes}m`,
      hours: (hours: number, minutes: number) => `${hours}h ${minutes}m`,
      days: (days: number, hours: number) => `${days}d ${hours}h`,
      exactDays: (days: number) => `${days}d`,
    },
    window: {
      session: "5-hour window",
      weekly: "Weekly window",
      monthly: "Monthly window",
      other: (minutes: number | null) =>
        minutes === null
          ? "Other window"
          : `${Math.round(minutes / 1440)}-day window`,
    },
    windowShort: {
      session: "5h",
      weekly: "wk",
      monthly: "mo",
      other: (minutes: number | null) =>
        minutes === null ? "?" : `${Math.round(minutes / 1440)}d`,
    },
    tone: {
      close: "Getting close",
      full: "Almost used up",
    },
    ago: {
      now: "just now",
      minutes: (minutes: number) => `${minutes} min ago`,
      hours: (hours: number) => `${hours} h ago`,
    },
  },

  statusBar: {
    label: "Status bar",
    lsp: {
      none: "No language server running",
      running: (list: string) => `Language servers: ${list}`,
      state: {
        starting: "starting",
        indexing: "indexing",
        ready: "ready",
        stopping: "stopping",
      },
    },
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
