export const terminalCopy = {
  terminal: {
    newTerminal: "New terminal",
    shell: "Shell",
    customCommand: "Custom command…",
    close: "Close terminal",
    exited: (code: number | null) =>
      code === null ? "exited" : `exited ${code}`,
    createFailed: "Could not start the terminal",
    sleep: "Sleep terminal",
    sleeping: "sleeping",
    sleepFailed: "Could not put the terminal to sleep",
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
} as const;
