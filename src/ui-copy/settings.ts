export const settingsCopy = {
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
      ptyDaemon: {
        label: "Run terminals in a separate process (experimental)",
        hint: "New terminals run in a background process instead of inside Lynx, so they can survive closing the window. Open terminals keep running where they are.",
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
      update: {
        label: "New version available",
        hint: "Notify when a new version of Lynx is out, while the window is not in front.",
      },
    },
    updates: {
      title: "Updates",
      intro:
        "Lynx looks for new versions and tells you when one is out. An update replaces the app itself. Your projects, worktrees and settings stay as they are.",
      installed: (version: string) => `Installed version ${version}`,
      attribution:
        "Based on Lynx by Adryel Dearo (https://github.com/zdearo/lynx-ide). Licensed under the AGPL-3.0.",
      lastChecked: (time: string) => `Last checked ${time}`,
      unsupported:
        "This copy of Lynx cannot update itself. On Linux, updates work for the AppImage only.",
      idle: "Lynx has not checked for updates yet.",
      checking: "Checking for updates…",
      upToDate: "Lynx is up to date.",
      available: (version: string) => `Version ${version} is available.`,
      notes: "What's new",
      downloading: (percent: number | null) =>
        percent === null
          ? "Downloading the update…"
          : `Downloading the update… ${percent}%`,
      readyToRestart: (version: string) =>
        `Version ${version} is installed. Restart Lynx to start it.`,
      failed: "The last update step failed. Try again.",
      check: "Check now",
      install: (version: string) => `Update to ${version}`,
      restart: "Restart Lynx",
      restartTitle: "Restart Lynx now?",
      restartBody:
        "Open terminals and agents will close. Files you saved stay as they are.",
      restartBodyDaemon:
        "Terminals in the background process keep running and come back when Lynx opens. Other open terminals and agents will close. Files you saved stay as they are.",
      restartConfirm: "Restart",
      cancel: "Cancel",
      checkFailed: "Could not check for updates",
      installFailed: "Could not install the update",
      restartFailed: "Could not restart Lynx",
      automatic: {
        label: "Check automatically",
        hint: "Look for a new version when Lynx starts and every few hours.",
      },
    },
    bots: {
      title: "Bots",
      intro:
        "Bots are assistants you talk to in Lynx. Lynx runs them itself and talks to each provider directly. Sign in to a provider here.",
      enabled: {
        label: "Use the ChatGPT plan",
        hint: "Bots may spend the ChatGPT plan you signed in with. Turning this on does not sign you in.",
      },
      chatgpt: {
        title: "ChatGPT",
        intro:
          "Sign in with the ChatGPT plan you already have. You sign in on OpenAI's page; Lynx never sees your password.",
        signedOut: "Not signed in.",
        signingIn: "Waiting for the sign-in in your browser…",
        signedIn: (email: string | null) =>
          email ? `Signed in as ${email}.` : "Signed in.",
        failed: "The last sign-in did not finish.",
        signIn: "Sign in with ChatGPT",
        signOut: "Sign out",
        signInFailed: "Could not start the sign-in",
        signOutFailed: "Could not sign out",
      },
      memory: {
        label: "Remember what we talk about",
        hint: "Every few turns of a conversation, the bot's CLI reads the new messages once for facts worth keeping and files them in its memory. It uses a short call of your plan each time.",
      },
      jev: {
        title: "Jev",
        intro:
          "Jev is a fast decision model from TypeSafe. With it on, a message that names nobody goes to the member Jev picks, in about a second. It needs your own key. When Jev is not sure, or does not answer in time, the first member answers.",
        enabled: {
          label: "Let Jev choose who answers",
          hint: "Only for channels, and only for messages that name nobody with @. Without a key, channels pick as they did before.",
        },
        key: {
          label: "Jev API key",
          placeholder: "Paste your key",
          saved: "A key is saved. It is never shown again.",
          none: "No key saved.",
          save: "Save key",
          remove: "Remove key",
          saveFailed: "Could not save the key",
          removeFailed: "Could not remove the key",
        },
        test: {
          action: "Test",
          ok: (ms: number) => `Jev answered in ${ms} ms.`,
          slow: (ms: number) =>
            `Jev answered in ${ms} ms. Above 1.5 s, channels go on without it.`,
          failed: (reason: string) => `Jev did not answer: ${reason}.`,
          runFailed: "Could not test the key",
        },
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
    orchestration: {
      title: "Orchestration",
      intro:
        "Before a coordinator starts workers, it asks which agent and model they use, and picks each worker's worktree.",
      enabled: {
        label: "Coordinate agents",
        hint: "A coordinator agent splits work into tasks and starts a worker for each one. While this is off, the orchestration tools refuse every call.",
      },
      release: {
        label: "Close finished workers",
        hint: "When the coordinator has heard a worker's report, the worker's terminal closes. The report stays with the coordinator; what the worker printed does not.",
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

  notifications: {
    done: (agent: string) => `${agent} finished`,
    needsYou: (agent: string) => `${agent} needs you`,
    updateAvailable: (version: string) => `Lynx ${version} is available`,
    updateBody: "Install it from Settings > Updates.",
    updateView: "View",
  },
} as const;
