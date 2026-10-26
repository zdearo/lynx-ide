export const usageCopy = {
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
    daemon: {
      idle: "Terminal process: starts with the first terminal",
      connected: (sessions: number) =>
        sessions === 1
          ? "Terminal process: 1 session"
          : `Terminal process: ${sessions} sessions`,
      degraded: "Terminal process unavailable: terminals run inside Lynx",
      memory: (mib: number) => `Terminals hold about ${mib} MB of memory`,
    },
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
} as const;
