/**
 * In-memory stand-in for the Rust backend, so the interface can be opened in a
 * plain browser (`?mock=full` or `?mock=empty`). Development only: main.tsx
 * imports it behind `import.meta.env.DEV`, so it never reaches a release build.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import type {
  AgentStatus,
  AppConfig,
  BotDetail,
  BotSetup,
  BotSummary,
  BrowserPageInfo,
  BrowserState,
  ChannelSummary,
  ChatGptStatus,
  ChatMessage,
  ResumableSession,
  RunningCommand,
  ThreadSummary,
  ConfigSnapshot,
  Folder,
  HarnessId,
  NewBot,
  NewChannel,
  NewTeamMember,
  PlanUsage,
  Repo,
  TaskState,
  TeamMember as TeamMemberRecord,
  TeamSnapshot,
  TerminalInfo,
  UpdateStatus,
  WorktreeInfo,
} from "@/bindings";
import {
  TEAM_AUTH_WORKTREE,
  TEAM_DOCS_MEMBERS,
  TEAM_MEMBERS,
  TEAM_PAIRS,
  TEAM_ROOT_ID,
  type TeamMember as MockTeamMember,
} from "@/team/team-fixtures";
import { canOrchestrate } from "@/team/team-hierarchy";
import { isValidConnection, pairKey, type TeamPair } from "@/team/team-pairs";

type Args = Record<string, unknown>;

const ESC = "\x1b[";
const dim = (text: string) => `${ESC}2m${text}${ESC}0m`;
const green = (text: string) => `${ESC}32m${text}${ESC}0m`;
const orange = (text: string) => `${ESC}38;5;215m${text}${ESC}0m`;
const bold = (text: string) => `${ESC}1m${text}${ESC}0m`;

const AGENT_TRANSCRIPT = [
  `${orange("●")} ${bold("Reading")} src-tauri/src/terminal/terminal_session.rs`,
  `${orange("●")} ${bold("Reading")} src-tauri/src/terminal/terminal_manager.rs`,
  "",
  "  The reader thread and the waiter thread share `SessionShared`, so the",
  "  exit notice waits for the reader to drain before it is sent.",
  "",
  `${orange("●")} ${bold("Edit")} src-tauri/src/terminal/terminal_session.rs`,
  `  ${green("+ let survivors: Vec<i32> = groups.iter().copied().filter(|g| group_exists(*g)).collect();")}`,
  `  ${dim("- if SessionShared::lock(&shared.exit).is_none() {")}`,
  "",
  `${orange("●")} ${bold("Bash")}(cargo test)`,
  `  ${dim("running 66 tests")}`,
  `  ${green("test result: ok. 66 passed; 0 failed")}`,
  "",
  "The foreground group is now killed even when the shell exits on SIGHUP.",
  "",
  ...Array.from(
    { length: 70 },
    (_, i) =>
      `${dim(`line ${String(i + 1).padStart(2, "0")}`)} filler output to fill every row of the terminal`,
  ),
  `${orange("●")} last line must be fully visible`,
].join("\r\n");

const SHELL_TRANSCRIPT = [
  // \ue0a0 branch, \uf07c folder, \ue0b0 powerline arrow: Nerd Font glyphs.
  `${ESC}44;30m \uf07c ~/lynx-ide ${ESC}0m${ESC}34m\ue0b0${ESC}0m ${green("\ue0a0 feat/auth")} ${orange("\uf113")}`,
  `${green("dev@host")} ${dim("~/lynx-ide")} $ git status -sb`,
  "## feat/auth",
  " M src/workspace/Sidebar.tsx",
  "?? src/dev/mock-tauri.ts",
  `${green("dev@host")} ${dim("~/lynx-ide")} $ `,
].join("\r\n");

function worktree(
  repoPath: string,
  name: string,
  branch: string,
  baseRef: string | null,
  isMain = false,
): WorktreeInfo {
  return {
    path: isMain ? repoPath : `${repoPath}-worktrees/${name}`,
    name: isMain ? (repoPath.split("/").at(-1) ?? name) : name,
    branch,
    head: "9f2c1a7b8d",
    baseRef,
    isMain,
    locked: false,
    prunable: false,
  };
}

function seedRepos(): {
  repos: Repo[];
  worktrees: Record<string, WorktreeInfo[]>;
} {
  const repos: Repo[] = [
    {
      id: "r1",
      name: "lynx-ide",
      path: "/home/dev/lynx-ide",
      folderId: null,
      linkedRootId: null,
      missing: false,
    },
    {
      id: "r2",
      name: "api-server",
      path: "/home/dev/api-server",
      folderId: "f-client",
      linkedRootId: null,
      missing: false,
    },
  ];
  return {
    repos,
    worktrees: {
      r1: [
        worktree(repos[0].path, "lynx-ide", "main", null, true),
        worktree(repos[0].path, "feat-auth", "feat/auth", "origin/main"),
        worktree(repos[0].path, "fix-pty", "fix/pty", "origin/main"),
      ],
      r2: [
        worktree(repos[1].path, "api-server", "main", null, true),
        worktree(repos[1].path, "refactor-db", "refactor-db", "origin/main"),
        worktree(repos[1].path, "docs-readme", "docs/readme", "origin/main"),
        worktree(
          repos[1].path,
          "very-long",
          "feature/very-long-branch-name-that-must-truncate-cleanly-123",
          "origin/release",
        ),
      ],
    },
  };
}

type MockEntry = {
  name: string;
  kind: "directory" | "file";
  symlink?: boolean;
  ignored?: boolean;
};

const dir = (name: string, ignored = false): MockEntry => ({
  name,
  kind: "directory",
  ignored,
});
const file = (name: string, ignored = false): MockEntry => ({
  name,
  kind: "file",
  ignored,
});

/** The same sample project under every worktree, keyed by folder. */
const MOCK_FILES: Record<string, MockEntry[]> = {
  "": [
    dir(".github"),
    dir(".vscode"),
    dir("docs"),
    dir("node_modules", true),
    dir("src"),
    dir("src-tauri"),
    dir("empty-folder"),
    file(".env.local"),
    file(".gitignore"),
    file(".prettierrc.json"),
    file("bundle.zip"),
    file("Cargo.lock"),
    file("Cargo.toml"),
    file("debug.log", true),
    file("Dockerfile"),
    file("LICENSE"),
    file("logo.png"),
    file("Makefile"),
    file("notes.pdf"),
    file("package.json"),
    file("bun.lock"),
    file("README.md"),
    file("run.sh"),
    file("tsconfig.json"),
    file("vite.config.ts"),
    file("a file with spaces.md"),
    file("data.sqlite"),
    file("track.mp3"),
    file("Procfile"),
    { name: "link-to-src", kind: "directory", symlink: true },
  ],
  ".github": [dir("workflows")],
  ".vscode": [file("settings.json")],
  ".github/workflows": [file("ci.yml")],
  docs: [file("architecture.md"), file("setup.md")],
  node_modules: [dir("react", true), dir("vite", true)],
  src: [dir("ui"), file("App.tsx"), file("main.tsx"), file("index.css")],
  "src/ui": [file("Button.tsx"), file("Dialog.tsx")],
  "src-tauri": [dir("src"), file("Cargo.toml")],
  "src-tauri/src": [file("lib.rs"), file("main.rs")],
  "empty-folder": [],
  "link-to-src": [file("App.tsx")],
};

/** What the sample files contain; every other file is a short line of text. */
const MOCK_CONTENT: Record<string, string> = {
  "src/App.tsx": `import { Button } from "./ui/Button";

// The sample app of the mock backend.
export function App() {
  const count = 3;
  return <Button label={"Clicked " + count} />;
}
`,
  "package.json": `{\n  "name": "sample",\n  "version": "1.0.0"\n}\n`,
  "README.md": "# Sample\n\nA project for the mock backend.\n",
};
const MOCK_BINARY = new Set([
  "bundle.zip",
  "data.sqlite",
  "track.mp3",
  "notes.pdf",
]);

/** What `git add` does to one mock file: its unstaged change moves into the index. */
function stageMockChange(path: string): void {
  const entry = MOCK_CHANGES.find((change) => change.path === path);
  if (entry?.unstaged) {
    entry.staged = entry.unstaged === "untracked" ? "added" : entry.unstaged;
    entry.unstaged = null;
  }
}

/** The conversation of the mock pull request #42: comments, reviews and one inline thread. */
const mockDiscussion = {
  comments: [
    {
      id: "IC_1",
      author: "ana",
      body: "Could we also cover the refresh path in a test?",
      createdAt: new Date(Date.now() - 3 * 3600 * 1000).toISOString(),
      url: "https://github.com/acme/app/pull/42#issuecomment-1",
    },
  ],
  reviews: [
    {
      author: "bruno",
      verdict: "changesRequested",
      body: "The token should be refreshed before the request, not after.",
      submittedAt: new Date(Date.now() - 2 * 3600 * 1000).toISOString(),
    },
  ],
  threads: [
    {
      id: "PRRT_mock1",
      path: "src/App.tsx",
      line: 12,
      resolved: false,
      outdated: false,
      comments: [
        {
          id: "C_1",
          author: "bruno",
          body: "Why is this call made twice?",
          createdAt: new Date(Date.now() - 2 * 3600 * 1000).toISOString(),
          url: "https://github.com/acme/app/pull/42#discussion_r1",
        },
      ],
    },
  ],
};

/** What GitHub holds for each mock pull request: an action on it changes what is read back. */
/** The plan limits the mock reports. The e2e replaces them through `__lynxMock.setPlanUsage`. */
let mockPlanUsage: PlanUsage[] = [
  {
    harness: "codex",
    plan: "free",
    state: "ready",
    reason: null,
    windows: [
      {
        kind: "monthly",
        minutes: 43_200,
        usedPercent: 0,
        // An hour past 29 days, so the countdown still reads "29d" a while after the page loads.
        resetsAt: Math.floor(Date.now() / 1000) + 29 * 86_400 + 3_600,
      },
    ],
    observedAt: Math.floor(Date.now() / 1000),
  },
];

const mockPrStates = new Map<
  number,
  { state: string; isDraft: boolean; autoMerge: boolean }
>([[42, { state: "open", isDraft: false, autoMerge: false }]]);

/** Pull requests created from the form during this session, by worktree. */
const createdPullRequests = new Map<
  string,
  { title: string; draft: boolean }
>();

/** Worktrees whose repository got a GitHub remote from the dialog during this session. */
const remoteCreated = new Set<string>();

/** Commits only the remote has, and whether they are old copies of commits rewritten here. */
let mockBehind = 0;
let mockRewritten = false;

const NOW_SECONDS = Math.floor(Date.now() / 1000);

/** The mock branch's commits, newest first. A commit adds one the remote lacks; a push sends them. */
const mockCommits = [
  {
    subject: "feat(auth): add the login form",
    author: "Ana Souza",
    ago: 3 * 3600,
    pushed: true,
  },
  {
    subject: "chore: bump dependencies",
    author: "Ana Souza",
    ago: 2 * 86400,
    pushed: true,
  },
  {
    subject: "docs(readme): describe the setup",
    author: "Bruno Lima",
    ago: 9 * 86400,
    pushed: true,
  },
];

interface MockLines {
  added: number;
  removed: number;
}

interface MockChange {
  path: string;
  originalPath: string | null;
  staged: string | null;
  unstaged: string | null;
  stagedLines: MockLines | null;
  unstagedLines: MockLines | null;
}

/** The changed files of every mock worktree, as `git status` would report them. */
let MOCK_CHANGES: MockChange[] = [
  {
    path: "README.md",
    originalPath: null,
    staged: "modified",
    unstaged: "modified",
    stagedLines: { added: 1, removed: 0 },
    unstagedLines: { added: 2, removed: 1 },
  },
  {
    path: "docs/setup.md",
    originalPath: null,
    staged: null,
    unstaged: "untracked",
    stagedLines: null,
    unstagedLines: { added: 12, removed: 0 },
  },
  {
    path: "package.json",
    originalPath: null,
    staged: "modified",
    unstaged: null,
    stagedLines: { added: 1, removed: 1 },
    unstagedLines: null,
  },
  {
    path: "src/App.tsx",
    originalPath: null,
    staged: null,
    unstaged: "modified",
    stagedLines: null,
    unstagedLines: { added: 3, removed: 1 },
  },
  {
    path: "src/ui/Button.tsx",
    originalPath: "src/ui/Btn.tsx",
    staged: "renamed",
    unstaged: null,
    stagedLines: null,
    unstagedLines: null,
  },
  {
    path: "src/ui/Legacy.tsx",
    originalPath: null,
    staged: null,
    unstaged: "deleted",
    stagedLines: null,
    unstagedLines: { added: 0, removed: 8 },
  },
];
/** What HEAD holds for the files above that exist there. */
const MOCK_ORIGINAL: Record<string, string> = {
  "README.md": "# Sample\n\nA project.\n",
  "package.json": `{\n  "name": "sample"\n}\n`,
  "src/App.tsx": `import { Button } from "./ui/Button";\n\nexport function App() {\n  return <Button label="Click" />;\n}\n`,
  "src/ui/Btn.tsx": "export const Btn = 1;\n",
  "src/ui/Legacy.tsx": "export const legacy = true;\n",
};

const DEFAULT_CONFIG: AppConfig = {
  terminal: {
    fontFamily: null,
    fontSize: 13,
    shell: null,
    scrollback: 5000,
    copyOnSelect: false,
    ptyDaemon: false,
    bell: "visual",
    macOptionIsMeta: false,
    closeOnExit: false,
    confirmClose: false,
    colorScheme: "default",
    textColorScheme: null,
  },
  worktrees: { directory: null },
  agents: { launchMode: "manual", launchArguments: {} },
  notifications: { onDone: true, onNeedsYou: true, onUpdate: true },
  updates: { checkAutomatically: true },
  bots: {
    chatgpt: { enabled: false },
    decider: { enabled: false },
    memory: { capture: true },
  },
  browser: { agentAccess: "full", allowedSites: [] },
  usage: {
    visible: true,
    providers: ["claude", "codex"],
    showNames: true,
    display: "used",
    layout: "compact",
    warnAt: 75,
    criticalAt: 90,
  },
  git: {
    commitMessageAgent: "auto",
    commitMessageStyle: "conventional",
    commitMessageInstructions: null,
  },
  orchestration: { enabled: false, releaseFinishedWorkers: true },
};

/** The same rules as the Rust validation, close enough for the interface to react to them. */
function configIssue(config: AppConfig): string | null {
  const { terminal, worktrees } = config;
  if (terminal.fontSize < 8 || terminal.fontSize > 32) {
    return "terminal.fontSize: must be between 8 and 32";
  }
  if (terminal.scrollback > 100000)
    return "terminal.scrollback: must be at most 100000";
  if (terminal.fontFamily && /[;{}]/.test(terminal.fontFamily)) {
    return "terminal.fontFamily: use a comma-separated list of font names, like in CSS";
  }
  const directory = worktrees.directory;
  if (directory && !directory.startsWith("/") && !directory.startsWith("~")) {
    return "worktrees.directory: must be an absolute path, or start with ~";
  }
  return null;
}

/** A 1x1 PNG: the picture each mock tab gets, so the list has something to show. */
const MOCK_PICTURE =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/** A big workspace for `e2e/perf-bench.mjs`: many repos, and one folder with thousands of files. */
function seedBenchmark(seeded: ReturnType<typeof seedRepos>): void {
  for (let i = 3; i <= 40; i++) {
    const repo: Repo = {
      id: `r${i}`,
      name: `service-${i}`,
      path: `/home/dev/service-${i}`,
      folderId: null,
      linkedRootId: null,
      missing: false,
    };
    seeded.repos.push(repo);
    seeded.worktrees[repo.id] = [
      worktree(repo.path, repo.name, "main", null, true),
      ...[1, 2, 3].map((n) =>
        worktree(repo.path, `task-${n}`, `task/${n}`, "origin/main"),
      ),
    ];
  }
  MOCK_FILES[""].push(dir("big"));
  MOCK_FILES["big"] = Array.from({ length: 5000 }, (_, i) =>
    file(`module-${String(i).padStart(4, "0")}.ts`),
  );
}

export function installMockTauri(scenario: string): void {
  const seeded =
    scenario === "empty" ? { repos: [], worktrees: {} } : seedRepos();
  if (scenario === "bench") seedBenchmark(seeded);
  let configSnapshot: ConfigSnapshot = {
    config: structuredClone(DEFAULT_CONFIG),
    issues:
      scenario === "broken"
        ? [
            {
              key: "",
              message: "not valid JSON: expected value at line 1 column 15",
            },
          ]
        : [],
    path: "/home/dev/.lynx/config.json",
    fileExists: scenario === "broken",
    fileBroken: scenario === "broken",
  };
  const fileContents = new Map<string, { content: string; hash: string }>();
  let writeCounter = 0;
  // The ChatGPT sign-in in miniature: the browser step is skipped, and the answer comes a moment later.
  // The bots in miniature: the list starts empty, as a new install does, and makes are kept here. The full
  // mock has one bot, the Team's Lead in the docs team.
  let mockBots: BotSummary[] =
    scenario === "full"
      ? [
          {
            id: "lead",
            name: "Lead",
            color: "indigo",
            provider: "claude",
            model: "sonnet",
            team: null,
          },
        ]
      : [];
  let mockChannels: ChannelSummary[] = [];
  const mockChannelConversations = new Map<string, ChatMessage[]>();
  // The instructions and the conversation of each mock bot, by id, as the backend keeps them.
  const mockInstructions = new Map<string, string>();
  // A conversation is kept by its thread's id.
  const mockConversations = new Map<string, ChatMessage[]>();
  const mockThreads = new Map<string, ThreadSummary[]>();
  // The bots writing a slow reply, and the ones the person asked to stop.
  const mockSlow = new Set<string>();
  const mockStopRequested = new Set<string>();
  const newMockThread = (): ThreadSummary => {
    const now = Math.floor(Date.now() / 1000);
    return {
      id: crypto.randomUUID().replaceAll("-", ""),
      title: "",
      createdAt: now,
      updatedAt: now,
    };
  };
  const threadsOfMockBot = (bot: string): ThreadSummary[] => {
    let list = mockThreads.get(bot);
    if (!list) {
      list = [newMockThread()];
      mockThreads.set(bot, list);
    }
    return list;
  };
  const mockGrants = new Map<string, { id: string; name: string }[]>();
  let mockJevKey = false;
  const mockCommands = new Map<string, string[]>();
  const mockWorkerGrants = new Map<string, { id: string; name: string }[]>();
  const mockRunning = new Map<string, RunningCommand[]>();
  const mockSkills = new Map<string, { name: string; description: string }[]>();
  const mockMcp = new Map<
    string,
    { name: string; kind: "command" | "url"; target: string }[]
  >();
  let chatGptStatus: ChatGptStatus = {
    phase: "signedOut",
    email: null,
    detail: null,
  };
  async function finishMockSignIn() {
    await new Promise((resolve) => setTimeout(resolve, 1200));
    chatGptStatus = {
      phase: "signedIn",
      email: "dev@example.com",
      detail: null,
    };
    void emit("chat-gpt-auth-changed", { status: chatGptStatus });
  }
  // The update flow in miniature: a check finds 0.1.1, and an install reports its download.
  let updateStatus: UpdateStatus = {
    currentVersion: "0.1.0",
    checkedAt: null,
    phase: { kind: "idle" },
  };
  function publishUpdate(phase: UpdateStatus["phase"]): UpdateStatus {
    updateStatus = { ...updateStatus, phase };
    void emit("update-status-changed", {
      status: structuredClone(updateStatus),
    });
    return structuredClone(updateStatus);
  }
  async function checkMockUpdate(): Promise<UpdateStatus> {
    publishUpdate({ kind: "checking" });
    await new Promise((resolve) => setTimeout(resolve, 800));
    updateStatus = { ...updateStatus, checkedAt: new Date().toISOString() };
    return publishUpdate({
      kind: "available",
      version: "0.1.1",
      notes: "Faster terminal scrolling and fixes for the file explorer.",
      date: new Date().toISOString(),
    });
  }
  async function installMockUpdate(): Promise<UpdateStatus> {
    for (const percent of [25, 60, 100]) {
      publishUpdate({ kind: "downloading", version: "0.1.1", percent });
      await new Promise((resolve) => setTimeout(resolve, 400));
    }
    return publishUpdate({ kind: "readyToRestart", version: "0.1.1" });
  }
  const probe: { configFileOpened: boolean; revealed: string | null } = {
    configFileOpened: false,
    revealed: null,
  };
  const repos: Repo[] = [...seeded.repos];
  // Work > Client X, with api-server inside Client X: the shape a real user would build.
  const folders: Folder[] =
    scenario === "empty"
      ? []
      : [
          {
            id: "f-work",
            name: "Work",
            parentId: null,
            collapsed: false,
            sourcePath: null,
            linkedRootId: null,
            missing: false,
          },
          {
            id: "f-client",
            name: "Client X",
            parentId: "f-work",
            collapsed: false,
            sourcePath: null,
            linkedRootId: null,
            missing: false,
          },
        ];
  let nextFolderId = 0;
  // Repos "cloned" into a linked folder show up on the next scan; a flag asks for one without a clone.
  const clonedIntoLinked: string[] = [];
  let linkedChanged = false;
  const descendantsOf = (id: string): Set<string> => {
    const ids = new Set([id]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const folder of folders) {
        if (
          folder.parentId &&
          ids.has(folder.parentId) &&
          !ids.has(folder.id)
        ) {
          ids.add(folder.id);
          grew = true;
        }
      }
    }
    return ids;
  };
  const findFolder = (id: unknown) => {
    const folder = folders.find((f) => f.id === id);
    if (!folder)
      throw {
        kind: "folderNotFound",
        message: `folder not found: ${String(id)}`,
      };
    return folder;
  };
  const worktrees: Record<string, WorktreeInfo[]> = { ...seeded.worktrees };

  const addMockRepo = (path: string): Repo => {
    const repo: Repo = {
      id: `r${repos.length + 1}`,
      name: path.split("/").at(-1) ?? path,
      path,
      folderId: null,
      linkedRootId: null,
      missing: false,
    };
    repos.push(repo);
    worktrees[repo.id] = [worktree(path, repo.name, "main", null, true)];
    return repo;
  };

  let nextSessionId = 0;
  const sessions: TerminalInfo[] = [];
  // Ended agent sessions per worktree, as the resumable list shows them. The login tests agent's session ended here.
  const resumable: Record<string, ResumableSession[]> =
    scenario === "full"
      ? {
          [TEAM_AUTH_WORKTREE]: [
            {
              id: "ended-login-tests",
              harness: "claude",
              resumeArguments: ["--resume", "7c1f0e2a-login-tests"],
              endedAt: Math.floor(Date.now() / 1000) - 1800,
            },
          ],
        }
      : {};
  const transcripts = new Map<string, string>();
  const seq = new Map<string, number>();

  // The program each agent's CLI runs as, which is how a typed command names its agent.
  const programs: Record<string, HarnessId> = {
    claude: "claude",
    codex: "codex",
    opencode: "opencode",
    omp: "omp",
  };
  // Which agent a typed command starts, as the backend decides it.
  function harnessOf(command: string | null) {
    const program = command?.trim().split(/\s+/)[0];
    return program ? (programs[program] ?? null) : null;
  }

  // Mirrors the backend's hook state per harness so the Agents settings can be exercised in dev.
  const hookStates: Record<string, "notInstalled" | "installed"> = {};
  const browserToolsOn = new Set<string>();
  // `?engine=missing` starts without the browser engine, to try the download.
  let engineInstalled =
    new URLSearchParams(window.location.search).get("engine") !== "missing";
  const engineStatus = () => ({
    state: engineInstalled ? "ready" : "missing",
    version: engineInstalled ? "154.0.34-p2" : null,
    path: "/home/dev/.lynx/engine/current/lynx-browser-host",
  });
  /** A download that reports a few steps, as the backend does. */
  async function installMockEngine() {
    const steps = [
      { phase: "downloading", received: 35_000_000, total: 139_000_000 },
      { phase: "downloading", received: 100_000_000, total: 139_000_000 },
      { phase: "verifying", received: 139_000_000, total: 139_000_000 },
      { phase: "unpacking", received: 0, total: 0 },
    ];
    for (const progress of steps) {
      void emit("browser-engine-progress", { progress });
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    engineInstalled = true;
    return engineStatus();
  }
  let agentReleaseTimer: ReturnType<typeof setTimeout> | undefined;
  let browserState: BrowserState = {
    status: "stopped",
    url: "",
    title: "",
    loading: false,
    canGoBack: false,
    canGoForward: false,
    error: null,
    detached: false,
    page: null,
  };
  // Every tab of the browser, whatever its owner. The full mock opens with the first one shown.
  const BROWSER_WORKTREE = "/home/dev/lynx-ide-worktrees/feat-auth";
  const mockPages: BrowserPageInfo[] =
    scenario === "full"
      ? [
          {
            id: 1,
            owner: { kind: "workspace", worktreePath: BROWSER_WORKTREE },
            url: "http://localhost:5173/",
            title: "Dashboard · Vite",
            loading: false,
            agent: null,
            detached: false,
            agentPage: false,
          },
          {
            id: 2,
            owner: { kind: "workspace", worktreePath: BROWSER_WORKTREE },
            url: "https://github.com/zdearo/lynx-ide/pull/4",
            title: "Pull request #4 · lynx-ide",
            loading: true,
            agent: null,
            detached: false,
            agentPage: false,
          },
          {
            id: 3,
            owner: { kind: "workspace", worktreePath: BROWSER_WORKTREE },
            url: "https://docs.example.com/api/tokens",
            title: "API tokens · Docs",
            loading: false,
            agent: "claude",
            detached: false,
            agentPage: true,
          },
        ]
      : [];
  const mockThumbnails = new Map<number, string>();
  for (const page of mockPages) mockThumbnails.set(page.id, MOCK_PICTURE);
  let nextPageId = mockPages.length + 1;
  let shownPage: number | null = null;

  /** Shows a tab (or none), and puts its address and title in the browser state, as the engine does. */
  const setShown = (id: number | null) => {
    shownPage = id;
    const page = mockPages.find((item) => item.id === id);
    browserState = {
      ...browserState,
      page: id,
      url: page?.url ?? "",
      title: page?.title ?? "",
      loading: page?.loading ?? false,
      detached: page?.detached ?? false,
    };
  };
  // With no tab shown there is nothing to go to, as in the backend.
  const navigateShown = (input: string) => {
    const page = mockPages.find((item) => item.id === shownPage);
    if (!page) return;
    page.url = input;
    page.title = input;
    browserState = { ...browserState, url: input, title: input };
  };
  /** Tells the interface the tabs or the shown tab changed, as the backend does. */
  const pagesChanged = () => {
    void emit("browser-pages-changed", { pages: structuredClone(mockPages) });
    void emit("browser-state-changed", {
      state: structuredClone(browserState),
    });
  };
  setShown(scenario === "full" ? 1 : null);
  const hookFiles: Record<string, { path: string; events: string[] }> = {
    claude: {
      path: "/home/dev/.claude/settings.json",
      events: ["SessionStart", "UserPromptSubmit", "Stop"],
    },
    codex: {
      path: "/home/dev/.codex/hooks.json",
      events: ["SessionStart", "SessionEnd", "UserPromptSubmit", "Stop"],
    },
    opencode: {
      path: "/home/dev/.config/opencode/plugins/lynx-agent-status.js",
      events: ["session.created", "session.idle", "permission.asked"],
    },
    antigravity: {
      path: "/home/dev/.gemini/config/hooks.json",
      events: ["PreInvocation", "PreToolUse", "Stop"],
    },
    cursor: {
      path: "/home/dev/.cursor/hooks.json",
      events: ["beforeSubmitPrompt", "preToolUse", "stop"],
    },
    copilot: {
      path: "/home/dev/.copilot/hooks/lynx.json",
      events: ["SessionStart", "UserPromptSubmit", "Stop"],
    },
    grok: {
      path: "/home/dev/.grok/hooks/lynx-status.json",
      events: ["SessionStart", "UserPromptSubmit", "Stop"],
    },
    droid: {
      path: "/home/dev/.factory/settings.json",
      events: ["SessionStart", "UserPromptSubmit", "Stop"],
    },
    kiro: {
      path: "/home/dev/.kiro/agents",
      events: ["agentSpawn", "userPromptSubmit", "stop"],
    },
    qwen: {
      path: "/home/dev/.qwen/settings.json",
      events: ["SessionStart", "UserPromptSubmit", "Stop"],
    },
    pi: {
      path: "/home/dev/.pi/agent/extensions/lynx-agent-status.ts",
      events: ["session_start", "agent_start", "agent_end"],
    },
    omp: {
      path: "/home/dev/.omp/agent/extensions/lynx-agent-status.ts",
      events: ["session_start", "agent_start", "agent_end"],
    },
  };
  // Every agent Lynx lists, and the ones whose CLI this mock machine has on its PATH.
  const catalogHarnesses = ["claude", "codex", "opencode", "omp"] as const;
  // What the last "Fix with AI" wrote and typed, for the e2e to read.
  let lastAgentPrompt = "";
  let lastTerminalCommand: string | null = null;
  const onThisMachine = new Set(["claude", "codex", "omp"]);
  function hookStatusOf(harness: string) {
    const file = hookFiles[harness] ?? hookFiles.claude;
    return {
      state: hookStates[harness] ?? "notInstalled",
      configPath: file.path,
      events: file.events,
      detail: null,
    };
  }

  // What the app asked the system to show, for the e2e to read.
  interface MockNotification {
    title: string;
    body?: string;
  }
  const sentNotifications: MockNotification[] = [];
  // The notification plugin replaces the browser's Notification the same way inside the app.
  class MockSystemNotification {
    static permission: NotificationPermission = "granted";
    static requestPermission = () => Promise.resolve("granted" as const);
    constructor(title: string, options?: { body?: string }) {
      sentNotifications.push({ title, body: options?.body });
    }
  }
  Object.assign(window, { Notification: MockSystemNotification });

  // Output reaches the page the way the backend sends it: raw bytes through the attached channel.
  const outputChannels = new Map<string, { channel: number; index: number }>();
  function sendOutput(id: string, seq: number, data: string) {
    const target = outputChannels.get(id);
    if (!target) return;
    const text = new TextEncoder().encode(data);
    const frame = new Uint8Array(4 + text.length);
    new DataView(frame.buffer).setUint32(0, seq, true);
    frame.set(text, 4);
    const internals = (
      window as unknown as {
        __TAURI_INTERNALS__: { runCallback(id: number, data: unknown): void };
      }
    ).__TAURI_INTERNALS__;
    internals.runCallback(target.channel, {
      message: frame.buffer,
      index: target.index++,
    });
  }

  function addSession(
    path: string,
    command: string | null,
    running: boolean,
    text: string,
    startedSecondsAgo = 0,
    agentStatus: AgentStatus = "idle",
  ) {
    const id = `t${++nextSessionId}`;
    sessions.push({
      id,
      worktreePath: path,
      command,
      shell: "zsh",
      running,
      hibernated: false,
      exitCode: running ? null : 0,
      startedAt: Math.floor(Date.now() / 1000) - startedSecondsAgo,
      harness: harnessOf(command),
      agentStatus: harnessOf(command) && running ? agentStatus : null,
    });
    transcripts.set(id, text);
    seq.set(id, 1);
    return id;
  }

  if (scenario === "crowded") {
    // Four agents in one worktree, to show the collapsed badges and the "+X" count.
    for (const [command, secondsAgo] of [
      ["claude", 60],
      ["claude", 2 * 3600],
      ["codex", 2 * 86400],
    ] as const) {
      addSession(
        "/home/dev/lynx-ide-worktrees/fix-pty",
        command,
        true,
        AGENT_TRANSCRIPT,
        secondsAgo,
      );
    }
  }

  if (scenario === "bench") {
    // Every session keeps a mounted terminal, so each one listens to the output events.
    for (let i = 0; i < 12; i++) {
      addSession("/home/dev/lynx-ide-worktrees/feat-auth", null, true, "");
    }
  }

  if (scenario !== "empty") {
    addSession(
      "/home/dev/lynx-ide-worktrees/feat-auth",
      "claude",
      true,
      AGENT_TRANSCRIPT,
      5 * 60,
    );
    addSession(
      "/home/dev/lynx-ide-worktrees/feat-auth",
      null,
      true,
      SHELL_TRANSCRIPT,
    );
    // Codex waits for an answer, so its folder's summary shows the question mark too.
    addSession(
      "/home/dev/lynx-ide-worktrees/fix-pty",
      "codex",
      true,
      AGENT_TRANSCRIPT,
      3 * 3600,
      "needsYou",
    );
    addSession(
      "/home/dev/api-server-worktrees/refactor-db",
      "claude",
      false,
      AGENT_TRANSCRIPT,
      2 * 86400,
    );
  }

  // The team in miniature. Its rules are the screen's own (team-hierarchy, team-pairs), so the mock refuses what
  // the backend refuses; the messages are the backend's for the common refusals.
  let mockMembers: MockTeamMember[] =
    scenario === "full"
      ? structuredClone([...TEAM_MEMBERS, ...TEAM_DOCS_MEMBERS])
      : [];
  let mockPairs: TeamPair[] =
    scenario === "full" ? TEAM_PAIRS.map(([from, to]) => ({ from, to })) : [];
  // The members whose branch is open: the seeded lead and docs bot, as the backend seeds a root orchestrator.
  const mockExpanded = new Set<string>(
    scenario === "full" ? [TEAM_ROOT_ID, "docs-lead"] : [],
  );
  const teamError = (
    kind: "teamNotFound" | "teamInvalid" | "teamRefused",
    message: string,
  ) => ({ kind, message });
  const ROLE_MESSAGE = "the role needs 1 to 40 characters";
  const TEXT_MESSAGE =
    "the task and the done-when line need up to 2000 characters";
  const cleanText = (
    value: unknown,
    min: number,
    max: number,
    message: string,
  ) => {
    const text = String(value).trim();
    if (text.length < min || text.length > max) {
      throw teamError("teamInvalid", message);
    }
    return text;
  };
  const memberNamed = (id: string) => {
    const member = mockMembers.find((candidate) => candidate.id === id);
    if (!member) throw teamError("teamNotFound", `no member with id ${id}`);
    return member;
  };
  // The worktree a member joins: an absolute path with no .. step, as the backend checks it.
  const worktreeOf = (value: unknown) => {
    const path = String(value);
    if (!path.startsWith("/") || path.split("/").includes("..")) {
      throw teamError(
        "teamInvalid",
        "the worktree needs an absolute path with no .. steps",
      );
    }
    return path;
  };
  // A member the mock makes starts with nothing live, as the screen's own defaults.
  const freshMember = (id: string, worktree: string, role: string) => ({
    id,
    worktree,
    role,
    status: "idle" as const,
    workerState: null,
    parentId: null as string | null,
    orchestrator: false,
    task: "",
    acceptance: "",
    taskState: null as TaskState | null,
    dependsOn: [] as string[],
    executionWorktree: null as string | null,
    sessionId: null as string | null,
    activity: { text: "", at: 0 },
    branch: null as string | null,
    added: 0,
    removed: 0,
    waitingOn: null as string | null,
    note: null as string | null,
    question: null as string | null,
    output: [] as string[],
  });
  const recordOf = (member: MockTeamMember): TeamMemberRecord => ({
    id: member.id,
    worktree: member.worktree,
    kind: member.kind,
    role: member.role,
    harness: member.kind === "agent" ? member.harness : null,
    botId: member.kind === "bot" ? member.botId : null,
    parentId: member.parentId,
    orchestrator: member.orchestrator,
    task: member.task,
    acceptance: member.acceptance,
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    dispatchState: null,
    sessionId: member.kind === "agent" ? member.sessionId : null,
    expanded: mockExpanded.has(member.id),
    // The mock has no orchestration, so no member is a worker.
    workerState: null,
    // The mock's simulated activity stands in for messages, so no member has one here.
    lastMessage: null,
  });
  const teamSnapshot = (): TeamSnapshot => ({
    members: mockMembers.map(recordOf),
    notes: [],
    pairs: mockPairs.map(({ from, to }) => ({
      a: from < to ? from : to,
      b: from < to ? to : from,
    })),
  });
  // Throws the refusal when `parentId` cannot orchestrate `child`, which must be in `members`.
  const checkParent = (
    members: MockTeamMember[],
    child: MockTeamMember,
    parentId: string,
  ) => {
    if (canOrchestrate(members, parentId, child.id)) return;
    if (child.kind === "bot") {
      throw teamError(
        "teamRefused",
        "a bot only orchestrates, so it is never under an orchestrator",
      );
    }
    const parent = members.find((member) => member.id === parentId);
    if (!parent)
      throw teamError("teamNotFound", `no member with id ${parentId}`);
    if (parent.id === child.id) {
      throw teamError(
        "teamRefused",
        "a member cannot be its own orchestrator or pair with itself",
      );
    }
    if (parent.worktree !== child.worktree) {
      throw teamError(
        "teamRefused",
        "members of different worktrees cannot be linked or answer to each other",
      );
    }
    if (!parent.orchestrator) {
      throw teamError(
        "teamRefused",
        "only an orchestrator can have members under it",
      );
    }
    throw teamError(
      "teamRefused",
      "an orchestrator cannot answer to one of its own members",
    );
  };
  // Throws the refusal when two members may not talk.
  const checkLink = (a: string, b: string) => {
    if (isValidConnection({ source: a, target: b }, mockMembers, mockPairs)) {
      return;
    }
    const first = memberNamed(a);
    const second = memberNamed(b);
    if (a === b) {
      throw teamError(
        "teamRefused",
        "a member cannot be its own orchestrator or pair with itself",
      );
    }
    if (first.worktree !== second.worktree) {
      throw teamError(
        "teamRefused",
        "members of different worktrees cannot be linked or answer to each other",
      );
    }
    if (
      mockPairs.some((pair) => pairKey(pair.from, pair.to) === pairKey(a, b))
    ) {
      throw teamError("teamRefused", "these two members already talk");
    }
    if (first.parentId === b || second.parentId === a) {
      throw teamError(
        "teamRefused",
        "an orchestrator and its member already talk through the tree",
      );
    }
    throw teamError("teamRefused", "two bots do not talk to each other");
  };
  const teamChanged = () => {
    void emit("team-changed", {});
    return null;
  };

  mockIPC(
    async (command, payload) => {
      const args = (payload ?? {}) as Args;
      const delay = (ms: number) =>
        new Promise((resolve) => setTimeout(resolve, ms));

      switch (command) {
        case "list_directory": {
          const folder = String(args.relativePath);
          const entries = MOCK_FILES[folder];
          if (!entries)
            throw { kind: "pathNotFound", message: `${folder} does not exist` };
          await delay(120);
          return {
            entries: entries.map((e) => ({
              name: e.name,
              kind: e.kind,
              symlink: e.symlink ?? false,
              ignored: e.ignored ?? false,
            })),
            truncated: false,
          };
        }
        case "get_plan_usage": {
          await delay(60);
          // Mirrors the backend: nothing is read while the limits are off, and only the chosen plans are.
          const usage = configSnapshot.config.usage;
          if (!usage.visible) return [];
          return structuredClone(
            mockPlanUsage.filter((plan) =>
              usage.providers.includes(plan.harness),
            ),
          );
        }
        case "watch_worktree":
          // The mock backend can always watch; the e2e sends the events itself.
          return args.worktreePath !== null;
        case "list_changes":
          await delay(60);
          return structuredClone(MOCK_CHANGES);
        case "stage_change":
          await delay(60);
          stageMockChange(String(args.relativePath));
          return null;
        case "discard_changes":
          await delay(60);
          // The unstaged change is gone; what was staged stays, and a file with nothing left leaves the list.
          MOCK_CHANGES = MOCK_CHANGES.flatMap((change) => {
            if (!(args.relativePaths as string[]).includes(change.path)) {
              return [change];
            }
            const rest = { ...change, unstaged: null };
            return rest.staged ? [rest] : [];
          });
          return null;
        case "stage_changes":
          await delay(60);
          for (const path of args.relativePaths as string[]) {
            stageMockChange(path);
          }
          return null;
        case "unstage_change": {
          const entry = MOCK_CHANGES.find(
            (change) => change.path === String(args.relativePath),
          );
          await delay(60);
          if (entry?.staged) {
            entry.unstaged =
              entry.staged === "added" ? "untracked" : entry.staged;
            entry.staged = null;
          }
          return null;
        }
        case "get_branch_sync": {
          await delay(40);
          const path = String(args.worktreePath);
          if (path.endsWith("fix-pty")) {
            return {
              branch: "fix/pty",
              upstream: null,
              ahead: 0,
              behind: 0,
              remote: remoteCreated.has(path) ? "origin" : null,
              rewritten: false,
            };
          }
          return {
            branch: "feat/auth",
            upstream: "origin/feat/auth",
            ahead: mockCommits.filter((commit) => !commit.pushed).length,
            behind: mockBehind,
            remote: "origin",
            rewritten: mockRewritten,
          };
        }
        case "fetch_remote":
          await delay(30);
          return null;
        case "pull_branch":
          await delay(150);
          mockBehind = 0;
          return null;
        case "force_push_branch":
          await delay(200);
          mockBehind = 0;
          mockRewritten = false;
          for (const commit of mockCommits) commit.pushed = true;
          return null;
        case "push_branch":
          await delay(200);
          mockBehind = 0;
          for (const commit of mockCommits) commit.pushed = true;
          return null;
        case "list_commit_files":
          await delay(40);
          return [
            {
              path: "src/App.tsx",
              originalPath: null,
              kind: "modified",
              lines: { added: 3, removed: 1 },
            },
            {
              path: "docs/notes.md",
              originalPath: null,
              kind: "added",
              lines: { added: 12, removed: 0 },
            },
          ];
        case "get_last_commit_message":
          await delay(30);
          return { summary: mockCommits[0].subject, description: "" };
        case "list_commits":
          await delay(40);
          return mockCommits.map((commit, index) => {
            const id = String(1000 + mockCommits.length - index);
            return {
              hash: `${id}`.padEnd(40, "a"),
              shortHash: id.slice(0, 7),
              subject: commit.subject,
              author: commit.author,
              timestamp: NOW_SECONDS - commit.ago,
              pushed: commit.pushed,
            };
          });
        case "get_pull_request_form":
          await delay(60);
          return {
            headBranch: "refactor/db-pool",
            headSha: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678",
            remote: "origin",
            bases: ["develop", "main"],
            defaultBase: "main",
            needsPush: true,
            suggestedTitle: "Refactor db pool",
          };
        case "draft_pull_request_text":
          await delay(200);
          return {
            title: "refactor(db): size the pool from the config",
            body: "Reads the pool size from the config instead of a constant.",
            agentLabel: "Claude Code",
          };
        case "create_pull_request": {
          await delay(300);
          const request = args.request as { title: string; draft: boolean };
          createdPullRequests.set(String(args.worktreePath), {
            title: request.title,
            draft: request.draft,
          });
          mockPrStates.set(77, {
            state: "open",
            isDraft: request.draft,
            autoMerge: false,
          });
          return { url: "https://github.com/acme/api/pull/77" };
        }
        case "get_check_log":
          await delay(80);
          return {
            text: "## Run tests\n$ cargo test\nerror: test failed, to rerun pass `--lib`",
            truncated: true,
          };
        case "get_github_account":
          await delay(60);
          return { login: "zdearo", organizations: ["acme"] };
        case "create_github_remote":
          await delay(300);
          remoteCreated.add(String(args.worktreePath));
          return null;
        case "get_pull_request": {
          await delay(120);
          const path = String(args.worktreePath);
          // The fix/pty worktree has no GitHub remote until the dialog creates one.
          if (path.endsWith("fix-pty") && !remoteCreated.has(path)) {
            throw { kind: "ghNoRemote", message: "no git remotes found" };
          }
          const created = createdPullRequests.get(path);
          if (created) {
            const held = mockPrStates.get(77) ?? {
              state: "open",
              isDraft: created.draft,
              autoMerge: false,
            };
            return {
              number: 77,
              title: created.title,
              url: "https://github.com/acme/api/pull/77",
              state: held.state,
              isDraft: held.isDraft,
              review: null,
              headBranch: "refactor/db-pool",
              headSha: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678",
              baseBranch: "main",
              mergeState: "clean",
              autoMerge: held.autoMerge,
              checks: [],
            };
          }
          // Only the feat/auth worktree has a pull request in the mock; the others have none.
          if (!path.endsWith("feat-auth")) return null;
          const held42 = mockPrStates.get(42);
          return {
            number: 42,
            title: "fix(auth): refresh the token before it expires",
            url: "https://github.com/acme/app/pull/42",
            state: held42?.state ?? "open",
            isDraft: held42?.isDraft ?? false,
            review: "reviewRequired",
            headBranch: "feat/auth",
            headSha: "5f2c1e0a9b8d7c6e5f4a3b2c1d0e9f8a7b6c5d4e",
            baseBranch: "main",
            mergeState: "unstable",
            autoMerge: held42?.autoMerge ?? false,
            checks: [
              {
                name: "CI / test",
                state: "failed",
                detailsUrl:
                  "https://github.com/acme/app/actions/runs/9/job/123",
              },
              { name: "CI / build", state: "pending", detailsUrl: null },
              { name: "CI / lint", state: "passed", detailsUrl: null },
              { name: "CI / docs", state: "skipped", detailsUrl: null },
            ],
          };
        }
        case "get_merge_methods":
          await delay(40);
          return ["squash", "merge"];
        case "run_pull_request_action": {
          await delay(150);
          const held = mockPrStates.get(Number(args.number));
          const action = args.action as {
            kind: string;
            auto?: boolean;
          };
          if (held) {
            if (action.kind === "merge") {
              if (action.auto) held.autoMerge = true;
              else held.state = "merged";
            } else if (action.kind === "close") held.state = "closed";
            else if (action.kind === "reopen") held.state = "open";
            else if (action.kind === "markReady") held.isDraft = false;
            else if (action.kind === "markDraft") held.isDraft = true;
          }
          return null;
        }
        case "get_pull_request_discussion":
          await delay(60);
          return structuredClone(mockDiscussion);
        case "comment_on_pull_request":
          await delay(100);
          mockDiscussion.comments.push({
            id: `IC_${mockDiscussion.comments.length + 1}`,
            author: "Dev User",
            body: String(args.body),
            createdAt: new Date().toISOString(),
            url: "https://github.com/acme/app/pull/42#issuecomment-2",
          });
          return null;
        case "reply_to_review_thread": {
          await delay(100);
          const thread = mockDiscussion.threads.find(
            (item) => item.id === args.threadId,
          );
          thread?.comments.push({
            id: `C_${(thread?.comments.length ?? 0) + 1}`,
            author: "Dev User",
            body: String(args.body),
            createdAt: new Date().toISOString(),
            url: "https://github.com/acme/app/pull/42#discussion_r2",
          });
          return null;
        }
        case "resolve_review_thread": {
          await delay(80);
          const thread = mockDiscussion.threads.find(
            (item) => item.id === args.threadId,
          );
          if (thread) thread.resolved = Boolean(args.resolved);
          return null;
        }
        case "open_link":
          await delay(40);
          return null;
        case "draft_commit_message":
          await delay(200);
          return {
            summary: "Update the Changes panel",
            description: "Describes the staged files.",
            agentLabel: "Claude Code",
          };
        case "commit_changes":
          await delay(120);
          // A summary that says so makes a pre-commit hook reject the commit.
          if (String(args.summary).includes("hook fails")) {
            throw {
              kind: "git",
              message: "pre-commit hook failed: eslint found 2 errors",
            };
          }
          if (args.amend) {
            const last = mockCommits[0];
            // The mock remote has the commit when it is pushed: only a yes lets it be rewritten.
            if (last.pushed && !args.allowPushed) {
              throw {
                kind: "changeRefused",
                message: "the last commit is already on the remote",
              };
            }
            // The remote still holds the old copy: the branch now reads as rewritten.
            if (last.pushed) {
              mockBehind = 1;
              mockRewritten = true;
            }
            last.subject = String(args.summary);
            last.pushed = false;
          } else {
            mockCommits.unshift({
              subject: String(args.summary),
              author: "Dev User",
              ago: 5,
              pushed: false,
            });
          }
          // What was staged is now in a commit: it leaves the list unless the working tree changed it again.
          MOCK_CHANGES = MOCK_CHANGES.flatMap((change) => {
            if (!change.staged) return [change];
            const rest = { ...change, staged: null };
            return rest.unstaged ? [rest] : [];
          });
          return null;
        case "read_file_diff": {
          const relative = String(args.relativePath);
          const change = MOCK_CHANGES.find((entry) => entry.path === relative);
          const key = `${args.worktreePath}::${relative}`;
          const original = MOCK_ORIGINAL[String(args.originalPath ?? relative)];
          await delay(60);
          const source = args.source as { kind: string; hash?: string };
          // An old commit: the same two small texts for every file, marked with the hash.
          if (source.kind === "commit") {
            return {
              original: `// before ${source.hash}\nconst value = 1;\n`,
              modified: `// after ${source.hash}\nconst value = 2;\n`,
            };
          }
          if (change?.unstaged === "deleted") {
            return { original: original ?? null, modified: null };
          }
          const stored = fileContents.get(key);
          const working =
            stored?.content ?? MOCK_CONTENT[relative] ?? `${relative}\n`;
          // The index sits between HEAD and the working tree: HEAD plus one staged line.
          const staged = `${original ?? ""}// staged line\n`;
          if (source.kind === "staged") {
            return { original: original ?? null, modified: staged };
          }
          if (source.kind === "unstaged") {
            return { original: staged, modified: working };
          }
          return { original: original ?? null, modified: working };
        }
        case "read_file": {
          const relative = String(args.relativePath);
          if (
            MOCK_CHANGES.some(
              (e) => e.path === relative && e.unstaged === "deleted",
            )
          ) {
            throw {
              kind: "pathNotFound",
              message: `${relative} does not exist`,
            };
          }
          if (MOCK_BINARY.has(relative)) {
            throw { kind: "binaryFile", message: `${relative} is not text` };
          }
          await delay(60);
          const stored = fileContents.get(`${args.worktreePath}::${relative}`);
          const content =
            stored?.content ?? MOCK_CONTENT[relative] ?? `${relative}\n`;
          return { content, hash: stored?.hash ?? `h-${content.length}` };
        }
        case "read_file_image":
          await delay(60);
          // A 1×1 transparent PNG.
          return {
            mime: "image/png",
            data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAfbLI3wAAAABJRU5ErkJggg==",
          };
        case "write_file": {
          const key = `${args.worktreePath}::${String(args.relativePath)}`;
          const stored = fileContents.get(key);
          if (
            args.expectedHash != null &&
            args.expectedHash !==
              (stored?.hash ??
                `h-${(MOCK_CONTENT[String(args.relativePath)] ?? `${String(args.relativePath)}\n`).length}`)
          ) {
            throw { kind: "fileChanged", message: "the file changed on disk" };
          }
          const content = String(args.content);
          const next = { content, hash: `w-${++writeCounter}` };
          fileContents.set(key, next);
          return next;
        }
        case "reveal_in_file_manager":
          probe.revealed = `${String(args.worktreePath)}::${String(args.relativePath)}`;
          return null;
        case "get_update_status":
          return structuredClone(updateStatus);
        case "check_for_updates":
          return await checkMockUpdate();
        case "install_update":
          return await installMockUpdate();
        case "restart_to_update":
          return null;
        case "list_shells":
          return ["/bin/bash", "/bin/zsh", "/usr/bin/fish"];
        case "get_config":
          return structuredClone(configSnapshot);
        case "team_snapshot":
          return teamSnapshot();
        case "team_add_member": {
          const input = args.member as NewTeamMember;
          const id = crypto.randomUUID();
          let added: MockTeamMember;
          if (input.kind === "agent") {
            const role = cleanText(input.role, 1, 40, ROLE_MESSAGE);
            const candidate: MockTeamMember = {
              ...freshMember(id, worktreeOf(input.worktree), role),
              kind: "agent",
              harness: input.harness,
              orchestrator: input.orchestrator,
              sessionId: input.sessionId,
            };
            // A member with no orchestrator keeps no task.
            let task = "";
            let acceptance = "";
            if (input.parentId !== null) {
              checkParent(
                [...mockMembers, candidate],
                candidate,
                input.parentId,
              );
              task = cleanText(input.task, 0, 2000, TEXT_MESSAGE);
              acceptance = cleanText(input.acceptance, 0, 2000, TEXT_MESSAGE);
            }
            added = {
              ...candidate,
              parentId: input.parentId,
              task,
              acceptance,
            };
          } else {
            const worktree = worktreeOf(input.worktree);
            if (!/^[a-z0-9-]+$/.test(input.botId)) {
              throw teamError("teamInvalid", "that is not a bot id");
            }
            // A bot joins a worktree's team once.
            if (
              mockMembers.some(
                (member) =>
                  member.worktree === worktree &&
                  member.kind === "bot" &&
                  member.botId === input.botId,
              )
            ) {
              throw teamError(
                "teamRefused",
                "that bot is already in this worktree's team",
              );
            }
            added = {
              ...freshMember(
                id,
                worktree,
                cleanText(input.role, 1, 40, ROLE_MESSAGE),
              ),
              kind: "bot",
              botId: input.botId,
              orchestrator: true,
            };
          }
          mockMembers = [...mockMembers, added];
          // A root orchestrator opens its branch; anything else starts closed.
          if (added.orchestrator && added.parentId === null) {
            mockExpanded.add(added.id);
          }
          teamChanged();
          return recordOf(added);
        }
        case "team_set_parent": {
          const member = memberNamed(String(args.memberId));
          const parentId =
            typeof args.parentId === "string" ? args.parentId : null;
          if (parentId === null) {
            member.parentId = null;
            member.task = "";
            member.acceptance = "";
          } else {
            checkParent(mockMembers, member, parentId);
            member.parentId = parentId;
            // Parent and child talk through the tree, so a line between them goes.
            mockPairs = mockPairs.filter(
              (pair) =>
                pairKey(pair.from, pair.to) !== pairKey(member.id, parentId),
            );
          }
          return teamChanged();
        }
        case "team_set_orchestrator": {
          const member = memberNamed(String(args.memberId));
          const on = Boolean(args.orchestrator);
          if (!on) {
            if (member.kind === "bot") {
              throw teamError(
                "teamRefused",
                "a bot only orchestrates, so it stays an orchestrator",
              );
            }
            if (mockMembers.some((other) => other.parentId === member.id)) {
              throw teamError(
                "teamRefused",
                "a member with members under it cannot stop being an orchestrator",
              );
            }
          }
          member.orchestrator = on;
          return teamChanged();
        }
        case "team_set_role": {
          const member = memberNamed(String(args.memberId));
          if (member.kind === "bot") {
            throw teamError(
              "teamRefused",
              "a bot keeps its name as its role, so only agents take a new role",
            );
          }
          member.role = cleanText(args.role, 1, 40, ROLE_MESSAGE);
          return teamChanged();
        }
        case "team_set_task": {
          const member = memberNamed(String(args.memberId));
          if (member.parentId === null) {
            throw teamError(
              "teamRefused",
              "a member with no orchestrator has no task",
            );
          }
          const task = cleanText(args.task, 0, 2000, TEXT_MESSAGE);
          const acceptance = cleanText(args.acceptance, 0, 2000, TEXT_MESSAGE);
          member.task = task;
          member.acceptance = acceptance;
          return teamChanged();
        }
        case "team_set_session": {
          const member = memberNamed(String(args.memberId));
          if (member.kind === "bot") {
            throw teamError("teamRefused", "a bot runs no terminal session");
          }
          const sessionId =
            typeof args.sessionId === "string" ? args.sessionId : null;
          // Clearing the session keeps the worktree, so a session started later goes back there.
          member.sessionId = sessionId;
          return teamChanged();
        }
        case "team_dismiss_member": {
          const id = String(args.memberId);
          const dismissed = memberNamed(id);
          // The members it orchestrated answer to no one, and lose their task.
          for (const other of mockMembers) {
            if (other.parentId !== id) continue;
            other.parentId = null;
            other.task = "";
            other.acceptance = "";
          }
          mockMembers = mockMembers.filter((member) => member.id !== id);
          mockPairs = mockPairs.filter(
            (pair) => pair.from !== id && pair.to !== id,
          );
          teamChanged();
          // The session the member ran in, so the screen can close it.
          return dismissed.kind === "agent" ? dismissed.sessionId : null;
        }
        case "team_set_expanded": {
          const id = String(args.memberId);
          memberNamed(id);
          if (args.expanded) mockExpanded.add(id);
          else mockExpanded.delete(id);
          return teamChanged();
        }
        case "team_set_linked": {
          const a = String(args.a);
          const b = String(args.b);
          if (args.linked) {
            checkLink(a, b);
            mockPairs = [...mockPairs, { from: a, to: b }];
          } else {
            mockPairs = mockPairs.filter(
              (pair) => pairKey(pair.from, pair.to) !== pairKey(a, b),
            );
          }
          return teamChanged();
        }
        case "bots_chatgpt_status":
          return structuredClone(chatGptStatus);
        case "bots_chatgpt_sign_in":
          chatGptStatus = { phase: "signingIn", email: null, detail: null };
          void finishMockSignIn();
          return structuredClone(chatGptStatus);
        case "bots_chatgpt_sign_out":
          chatGptStatus = { phase: "signedOut", email: null, detail: null };
          return structuredClone(chatGptStatus);
        case "bots_list":
          return structuredClone(mockBots);
        case "bots_create": {
          const bot = args.bot as NewBot;
          const name = bot.name.trim();
          const created: BotSummary = {
            id: name
              .toLowerCase()
              .replace(/[^a-z0-9]+/g, "-")
              .replace(/^-|-$/g, ""),
            name,
            color: bot.color,
            provider: bot.provider,
            model: bot.model,
            team: bot.team,
          };
          mockBots = [...mockBots, created].sort((a, b) =>
            a.name.localeCompare(b.name),
          );
          mockInstructions.set(created.id, bot.instructions);
          return structuredClone(created);
        }
        case "bots_get": {
          const id = String(args.id);
          const bot = mockBots.find((candidate) => candidate.id === id);
          if (!bot) {
            throw {
              kind: "invalidBot",
              message: "there is no bot with that id",
            };
          }
          const detail: BotDetail = {
            ...structuredClone(bot),
            instructions: mockInstructions.get(id) ?? "",
            folder: `/home/mock/.lynx/bots/${id}`,
          };
          return detail;
        }
        case "bots_update_instructions": {
          const id = String(args.id);
          const bot = mockBots.find((candidate) => candidate.id === id);
          if (!bot) {
            throw {
              kind: "invalidBot",
              message: "there is no bot with that id",
            };
          }
          mockInstructions.set(id, String(args.instructions).trim());
          const detail: BotDetail = {
            ...structuredClone(bot),
            instructions: mockInstructions.get(id) ?? "",
            folder: `/home/mock/.lynx/bots/${id}`,
          };
          return detail;
        }
        case "bots_update_setup": {
          const id = String(args.id);
          const setup = args.setup as BotSetup;
          const bot = mockBots.find((candidate) => candidate.id === id);
          if (!bot) {
            throw {
              kind: "invalidBot",
              message: "there is no bot with that id",
            };
          }
          bot.provider = setup.provider;
          bot.model = setup.model;
          bot.team = setup.team;
          const detail: BotDetail = {
            ...structuredClone(bot),
            instructions: mockInstructions.get(id) ?? "",
            folder: `/home/mock/.lynx/bots/${id}`,
          };
          return detail;
        }
        case "bots_delete": {
          const id = String(args.id);
          mockBots = mockBots.filter((candidate) => candidate.id !== id);
          mockInstructions.delete(id);
          for (const thread of mockThreads.get(id) ?? [])
            mockConversations.delete(thread.id);
          mockThreads.delete(id);
          return null;
        }
        // The mock keeps no team or runs under a bot, so deleting one takes nothing else with it.
        case "bots_delete_impact":
          return { members: [], runningWorkers: 0, runs: 0 };
        case "bots_workers":
          return [];
        case "bots_grants":
          return structuredClone(mockGrants.get(String(args.id)) ?? []);
        case "bots_approve_access": {
          const id = String(args.id);
          const turn = (mockConversations.get(String(args.thread)) ?? []).find(
            (message) => message.id === String(args.messageId),
          );
          if (!turn?.request || turn.request.state !== "pending") {
            throw {
              kind: "invalidBot",
              message:
                "there is no access request waiting for an answer with that id",
            };
          }
          turn.request = { ...turn.request, state: "approved" };
          const items = turn.request.items ?? [];
          if (turn.request.kind === "workers") {
            mockWorkerGrants.set(id, [
              ...(mockWorkerGrants.get(id) ?? []),
              { id: turn.request.repoId ?? "", name: items[0] ?? "" },
            ]);
          } else if (turn.request.kind === "commands") {
            mockCommands.set(id, [
              ...new Set([...(mockCommands.get(id) ?? []), ...items]),
            ]);
          } else {
            mockGrants.set(id, [
              ...(mockGrants.get(id) ?? []),
              { id: turn.request.repoId ?? "", name: items[0] ?? "" },
            ]);
          }
          // The bot goes on once it is approved, and its answer is kept in the conversation.
          const answer =
            turn.request.kind === "workers"
              ? `Certo! Vou lançar agentes em ${items[0] ?? ""}.`
              : `Obrigado! Vou usar ${items.join(", ")}.`;
          (mockConversations.get(String(args.thread)) ?? []).push({
            id: crypto.randomUUID(),
            author: "bot",
            text: answer,
            at: Math.floor(Date.now() / 1000),
            speaker: id,
          });
          return { ok: true, text: answer };
        }
        case "bots_deny_access": {
          const turn = (mockConversations.get(String(args.thread)) ?? []).find(
            (message) => message.id === String(args.messageId),
          );
          if (!turn?.request || turn.request.state !== "pending") {
            throw {
              kind: "invalidBot",
              message:
                "there is no access request waiting for an answer with that id",
            };
          }
          turn.request = { ...turn.request, state: "denied" };
          return null;
        }
        case "bots_revoke_access": {
          const id = String(args.id);
          const kept = (mockGrants.get(id) ?? []).filter(
            (grant) => grant.id !== String(args.repoId),
          );
          mockGrants.set(id, kept);
          return structuredClone(kept);
        }
        case "bots_jev_status":
          return { configured: mockJevKey };
        case "bots_jev_save_key":
          mockJevKey = true;
          return { configured: true };
        case "bots_jev_clear_key":
          mockJevKey = false;
          return { configured: false };
        case "bots_jev_test":
          return mockJevKey
            ? { ok: true, latencyMs: 340, reason: null }
            : { ok: false, latencyMs: 0, reason: "no key saved" };
        case "bots_worker_grants":
          return structuredClone(mockWorkerGrants.get(String(args.id)) ?? []);
        case "bots_revoke_workers": {
          const id = String(args.id);
          const kept = (mockWorkerGrants.get(id) ?? []).filter(
            (grant) => grant.id !== String(args.repoId),
          );
          mockWorkerGrants.set(id, kept);
          return structuredClone(kept);
        }
        case "bots_running_commands":
          return structuredClone(mockRunning.get(String(args.id)) ?? []);
        case "bots_stop_command": {
          const id = String(args.id);
          mockRunning.set(
            id,
            (mockRunning.get(id) ?? []).map((one) =>
              one.session === String(args.session)
                ? { ...one, running: false }
                : one,
            ),
          );
          return null;
        }
        case "bots_allowed_commands":
          return structuredClone(mockCommands.get(String(args.id)) ?? []);
        case "bots_allow_commands": {
          const id = String(args.id);
          const patterns = (args.patterns as string[]).map((p) => p.trim());
          const refused = patterns.find(
            (p) => !p || /^(\*|(ba|z|fi)?sh|sudo|env)(\s|$)/.test(p),
          );
          if (refused !== undefined || patterns.length === 0) {
            throw {
              kind: "invalidBot",
              message: `that command pattern is not allowed: "${refused ?? ""}" would allow everything`,
            };
          }
          const next = [
            ...new Set([...(mockCommands.get(id) ?? []), ...patterns]),
          ];
          mockCommands.set(id, next);
          return structuredClone(next);
        }
        case "bots_disallow_command": {
          const id = String(args.id);
          const kept = (mockCommands.get(id) ?? []).filter(
            (pattern) => pattern !== String(args.pattern),
          );
          mockCommands.set(id, kept);
          return structuredClone(kept);
        }
        case "bots_skills":
          return structuredClone(mockSkills.get(String(args.id)) ?? []);
        case "bots_skill_add": {
          const id = String(args.id);
          // The mock names the skill after the folder the picker returned.
          const name = String(args.path).split("/").pop() ?? "skill";
          const skills = mockSkills.get(id) ?? [];
          if (skills.some((skill) => skill.name === name)) {
            throw {
              kind: "invalidBot",
              message: "the bot already has a skill with that name",
            };
          }
          const next = [...skills, { name, description: `Skill ${name}` }];
          mockSkills.set(id, next);
          return structuredClone(next);
        }
        case "bots_skill_remove": {
          const id = String(args.id);
          const kept = (mockSkills.get(id) ?? []).filter(
            (skill) => skill.name !== String(args.name),
          );
          mockSkills.set(id, kept);
          return structuredClone(kept);
        }
        case "bots_mcp_servers":
          return structuredClone(mockMcp.get(String(args.id)) ?? []);
        case "bots_mcp_add": {
          const id = String(args.id);
          let parsed: {
            mcpServers?: Record<string, { command?: string; url?: string }>;
          };
          try {
            parsed = JSON.parse(String(args.config));
          } catch {
            throw {
              kind: "invalidBot",
              message: "the MCP servers are not valid: it is not valid JSON",
            };
          }
          const servers = new Map(
            (mockMcp.get(id) ?? []).map((server) => [server.name, server]),
          );
          for (const [name, entry] of Object.entries(parsed.mcpServers ?? {})) {
            servers.set(name, {
              name,
              kind: entry.url ? "url" : "command",
              target: entry.url ?? entry.command ?? "",
            });
          }
          const next = [...servers.values()].sort((a, b) =>
            a.name.localeCompare(b.name),
          );
          mockMcp.set(id, next);
          return structuredClone(next);
        }
        case "bots_mcp_remove": {
          const id = String(args.id);
          const kept = (mockMcp.get(id) ?? []).filter(
            (server) => server.name !== String(args.name),
          );
          mockMcp.set(id, kept);
          return structuredClone(kept);
        }
        case "channels_update_members": {
          const id = String(args.id);
          const channel = mockChannels.find((candidate) => candidate.id === id);
          if (!channel) {
            throw {
              kind: "invalidBot",
              message: "there is no channel with that id",
            };
          }
          channel.members = [...new Set(args.members as string[])];
          return structuredClone(channel);
        }
        case "channels_delete": {
          const id = String(args.id);
          if (!mockChannels.some((candidate) => candidate.id === id)) {
            throw {
              kind: "invalidBot",
              message: "there is no channel with that id",
            };
          }
          mockChannels = mockChannels.filter(
            (candidate) => candidate.id !== id,
          );
          mockChannelConversations.delete(id);
          return null;
        }
        case "channels_list":
          return structuredClone(mockChannels);
        case "channels_create": {
          const channel = args.channel as NewChannel;
          const name = channel.name.trim();
          const created: ChannelSummary = {
            id: name
              .toLowerCase()
              .replace(/[^a-z0-9]+/g, "-")
              .replace(/^-|-$/g, ""),
            name,
            members: channel.members,
          };
          mockChannels = [...mockChannels, created].sort((a, b) =>
            a.name.localeCompare(b.name),
          );
          return structuredClone(created);
        }
        case "channels_conversation":
          return structuredClone(
            mockChannelConversations.get(String(args.id)) ?? [],
          );
        case "channels_chat": {
          const id = String(args.id);
          const channel = mockChannels.find((candidate) => candidate.id === id);
          if (!channel) {
            throw {
              kind: "invalidBot",
              message: "there is no channel with that id",
            };
          }
          const thread = `channel:${id}`;
          const text = String(args.text).trim();
          const log = mockChannelConversations.get(id) ?? [];
          mockChannelConversations.set(id, log);
          // The backend's order: the person's message, the routing, then each member in turn.
          const person: ChatMessage = {
            id: crypto.randomUUID(),
            author: "user",
            text,
            at: Math.floor(Date.now() / 1000),
            speaker: null,
          };
          log.push(person);
          void emit("chat-message-added", { thread, message: person });
          void emit("channel-status", {
            thread,
            phase: "routing",
            author: null,
            waiting: [],
          });
          const members = channel.members.filter((memberId) =>
            mockBots.some((candidate) => candidate.id === memberId),
          );
          for (const [index, memberId] of members.entries()) {
            const member = mockBots.find(
              (candidate) => candidate.id === memberId,
            );
            if (!member) continue;
            const waiting = members.slice(index + 1);
            void emit("channel-status", {
              thread,
              phase: "answering",
              author: member.id,
              waiting,
            });
            const reply = `Oi, sou ${member.name}.`;
            for (const piece of reply.split(/(?<=\s)/)) {
              await delay(60);
              void emit("bot-reply-delta", {
                thread,
                author: member.id,
                text: piece,
              });
            }
            const saved: ChatMessage = {
              id: crypto.randomUUID(),
              author: "bot",
              text: reply,
              at: Math.floor(Date.now() / 1000),
              speaker: member.id,
            };
            log.push(saved);
            void emit("chat-message-added", { thread, message: saved });
          }
          void emit("channel-status", {
            thread,
            phase: "idle",
            author: null,
            waiting: [],
          });
          return null;
        }
        case "bots_conversation":
          return structuredClone(
            mockConversations.get(String(args.thread)) ?? [],
          );
        case "bots_threads":
          return structuredClone(
            [...threadsOfMockBot(String(args.id))].sort(
              (a, b) => b.updatedAt - a.updatedAt,
            ),
          );
        case "bots_thread_create": {
          const created = newMockThread();
          threadsOfMockBot(String(args.id)).unshift(created);
          return structuredClone(created);
        }
        case "bots_thread_rename": {
          const found = threadsOfMockBot(String(args.id)).find(
            (thread) => thread.id === String(args.thread),
          );
          if (!found) {
            throw {
              kind: "invalidBot",
              message: "there is no thread with that id",
            };
          }
          found.title = String(args.title).trim();
          return structuredClone(found);
        }
        case "bots_thread_delete": {
          const id = String(args.id);
          if (mockSlow.has(`${id}:${String(args.thread)}`)) {
            throw {
              kind: "invalidBot",
              message:
                "the bot is answering: change it once its reply is written",
            };
          }
          mockThreads.set(
            id,
            threadsOfMockBot(id).filter(
              (thread) => thread.id !== String(args.thread),
            ),
          );
          mockConversations.delete(String(args.thread));
          return null;
        }
        case "channels_stop":
          return false;
        case "bots_stop": {
          const slow = `${String(args.id)}:${String(args.thread)}`;
          if (!mockSlow.has(slow)) return false;
          mockStopRequested.add(slow);
          return true;
        }
        case "bots_chat": {
          const id = String(args.id);
          const thread = String(args.thread);
          const bot = mockBots.find((candidate) => candidate.id === id);
          if (!bot) {
            throw {
              kind: "invalidBot",
              message: "there is no bot with that id",
            };
          }
          const text = String(args.text).trim();
          const log = mockConversations.get(thread) ?? [];
          // The first message titles the thread, as the backend does.
          const titled = threadsOfMockBot(id).find((one) => one.id === thread);
          if (titled && log.length === 0 && !titled.title)
            titled.title = text.slice(0, 40);
          log.push({
            id: crypto.randomUUID(),
            author: "user",
            text,
            at: Math.floor(Date.now() / 1000),
          });
          mockConversations.set(thread, log);
          // Asking to read a registered repository leaves a request in the chat, as the backend does.
          if (text.includes("acesso ao lynx")) {
            // The backend keeps the answer under the question and the request after it.
            const answer = "Pedi à pessoa para ler o lynx.";
            log.push({
              id: crypto.randomUUID(),
              author: "bot",
              text: answer,
              at: Math.floor(Date.now() / 1000),
            });
            const request: ChatMessage = {
              id: crypto.randomUUID(),
              author: "bot",
              text: "Preciso ler o código do lynx para responder.",
              at: Math.floor(Date.now() / 1000),
              speaker: id,
              request: {
                kind: "project",
                items: ["lynx"],
                repoId: "lynx",
                state: "pending",
              },
            };
            log.push(request);
            void emit("chat-message-added", {
              thread: `bot:${id}:${thread}`,
              message: request,
            });
            return { ok: true, text: answer };
          }
          // A slow reply the person can stop: what was written stays, marked as stopped.
          if (text.includes("demora")) {
            void emit("bot-reply-delta", {
              thread: `bot:${id}:${thread}`,
              author: id,
              text: "Começando a escrever",
            });
            const slow = `${id}:${thread}`;
            mockSlow.add(slow);
            const started = Date.now();
            while (
              !mockStopRequested.has(slow) &&
              Date.now() - started < 8000
            ) {
              await new Promise((resolve) => setTimeout(resolve, 50));
            }
            mockSlow.delete(slow);
            mockStopRequested.delete(slow);
            log.push({
              id: crypto.randomUUID(),
              author: "bot",
              text: "Começando a escrever",
              at: Math.floor(Date.now() / 1000),
              stopped: true,
            });
            return { ok: true, text: "Começando a escrever" };
          }
          // A reply with a block of code, to copy from.
          if (text.includes("mostra um comando")) {
            const answer =
              "Rode isto:\n\n```bash\nminha-cli serve --port 8080\n```\n\nDepois abra o navegador.";
            log.push({
              id: crypto.randomUUID(),
              author: "bot",
              text: answer,
              at: Math.floor(Date.now() / 1000),
            });
            return { ok: true, text: answer };
          }
          // A command the bot leaves running shows in its Agents tab.
          if (text.includes("deixa rodando")) {
            mockRunning.set(id, [
              {
                session: "t1",
                command: "minha-cli serve",
                running: true,
                startedAt: Math.floor(Date.now() / 1000),
                output: "listening on 8080",
              },
            ]);
            const answer = "Iniciei minha-cli serve.";
            log.push({
              id: crypto.randomUUID(),
              author: "bot",
              text: answer,
              at: Math.floor(Date.now() / 1000),
            });
            return { ok: true, text: answer };
          }
          // Asking to start agents leaves the same card, for one project.
          if (text.includes("lançar um agente")) {
            const answer = "Pedi à pessoa: lançar agentes em lynx";
            log.push({
              id: crypto.randomUUID(),
              author: "bot",
              text: answer,
              at: Math.floor(Date.now() / 1000),
            });
            const request: ChatMessage = {
              id: crypto.randomUUID(),
              author: "bot",
              text: "Vou pedir a um agente para revisar o PR.",
              at: Math.floor(Date.now() / 1000),
              speaker: id,
              request: {
                kind: "workers",
                items: ["lynx"],
                repoId: "lynx",
                state: "pending",
              },
            };
            log.push(request);
            void emit("chat-message-added", {
              thread: `bot:${id}:${thread}`,
              message: request,
            });
            return { ok: true, text: answer };
          }
          // Asking to run commands leaves the same kind of card, listing the patterns.
          if (text.includes("rodar a cli")) {
            const answer = "Pedi à pessoa: rodar minha-cli *, git status";
            log.push({
              id: crypto.randomUUID(),
              author: "bot",
              text: answer,
              at: Math.floor(Date.now() / 1000),
            });
            const request: ChatMessage = {
              id: crypto.randomUUID(),
              author: "bot",
              text: "Preciso rodar a CLI para fazer a tarefa.",
              at: Math.floor(Date.now() / 1000),
              speaker: id,
              request: {
                kind: "commands",
                items: ["minha-cli *", "git status"],
                repoId: null,
                state: "pending",
              },
            };
            log.push(request);
            void emit("chat-message-added", {
              thread: `bot:${id}:${thread}`,
              message: request,
            });
            return { ok: true, text: answer };
          }
          // A reply that does not come is kept as an error line, as the backend keeps it.
          if (text.includes("falhar")) {
            const failure = "Claude did not answer.";
            log.push({
              id: crypto.randomUUID(),
              author: "error",
              text: failure,
              at: Math.floor(Date.now() / 1000),
            });
            return { ok: false, text: failure };
          }
          // The engine writes its reply in pieces, and so does the mock, with Markdown in it.
          const reply = `Oi! Sou ${bot.name}. Você disse: "${text}".\n\n**Hoje:**\n- Reunião às 10h\n- Revisar o PR\n\nVeja [a agenda](https://example.com/agenda).`;
          for (const piece of reply.split(/(?<=\s)/)) {
            await delay(60);
            void emit("bot-reply-delta", {
              thread: `bot:${id}:${thread}`,
              author: id,
              text: piece,
            });
          }
          log.push({
            id: crypto.randomUUID(),
            author: "bot",
            text: reply,
            at: Math.floor(Date.now() / 1000),
          });
          return { ok: true, text: reply };
        }
        case "update_config": {
          const next = args.config as AppConfig;
          if (configSnapshot.fileBroken) {
            throw {
              kind: "brokenConfigFile",
              message: "the config file is not valid JSON",
            };
          }
          const issue = configIssue(next);
          if (issue) throw { kind: "invalidConfig", message: issue };
          configSnapshot = {
            ...configSnapshot,
            config: structuredClone(next),
            fileExists: true,
          };
          return structuredClone(configSnapshot);
        }
        case "open_config_file":
          probe.configFileOpened = true;
          return null;
        case "list_folders":
          return structuredClone(folders);
        case "create_folder": {
          const name = String(args.name ?? "").trim();
          if (!name)
            throw {
              kind: "invalidName",
              message: "invalid name: the folder name is empty",
            };
          const parentId = (args.parentId as string | null) ?? null;
          if (parentId) findFolder(parentId);
          const folder: Folder = {
            id: `f${++nextFolderId}`,
            name,
            parentId,
            collapsed: false,
            sourcePath: null,
            linkedRootId: null,
            missing: false,
          };
          folders.push(folder);
          const sourcePath = (args.sourcePath as string | null) ?? null;
          if (sourcePath) {
            // What a scan of a code folder finds: a repo at the top and one inside a subfolder.
            folder.sourcePath = sourcePath;
            folder.linkedRootId = folder.id;
            const clients: Folder = {
              id: `f${++nextFolderId}`,
              name: "clients",
              parentId: folder.id,
              collapsed: false,
              sourcePath: `${sourcePath}/clients`,
              linkedRootId: folder.id,
              missing: false,
            };
            folders.push(clients);
            const top = addMockRepo(`${sourcePath}/lynx`);
            top.folderId = folder.id;
            top.linkedRootId = folder.id;
            const inner = addMockRepo(`${sourcePath}/clients/acme`);
            inner.folderId = clients.id;
            inner.linkedRootId = folder.id;
          }
          return structuredClone(folder);
        }
        case "preview_linked_folder": {
          const path = String(args.path);
          if (path.endsWith("/repo-itself"))
            throw {
              kind: "linkedFolder",
              message: `${path} is a git repository; add it as a project instead`,
            };
          return { folders: 1, projects: 2, truncated: false };
        }
        case "sync_linked_folders": {
          const roots = folders.filter(
            (f) =>
              f.linkedRootId === f.id &&
              (args.folderId == null || f.id === args.folderId),
          );
          if (args.folderId != null && roots.length === 0)
            findFolder(args.folderId);
          let changed = linkedChanged;
          linkedChanged = false;
          for (const name of clonedIntoLinked.splice(0)) {
            const root = roots[0];
            if (!root?.sourcePath) continue;
            const repo = addMockRepo(`${root.sourcePath}/${name}`);
            repo.folderId = root.id;
            repo.linkedRootId = root.id;
            changed = true;
          }
          return changed;
        }
        case "unlink_folder": {
          const root = findFolder(args.folderId);
          if (root.linkedRootId !== root.id)
            throw {
              kind: "linkedFolder",
              message: "this folder is not linked to a folder on disk",
            };
          for (const f of folders)
            if (f.linkedRootId === root.id) {
              f.linkedRootId = null;
              f.sourcePath = null;
              f.missing = false;
            }
          for (const r of repos)
            if (r.linkedRootId === root.id) {
              r.linkedRootId = null;
              r.missing = false;
            }
          return null;
        }
        case "rename_folder": {
          const folder = findFolder(args.folderId);
          folder.name = String(args.name ?? "").trim() || folder.name;
          return structuredClone(folder);
        }
        case "move_folder": {
          const folder = findFolder(args.folderId);
          const parentId = (args.parentId as string | null) ?? null;
          if (parentId) findFolder(parentId);
          if (parentId && descendantsOf(folder.id).has(parentId)) {
            throw {
              kind: "folderCycle",
              message:
                "a folder cannot be moved into itself or into one of its own folders",
            };
          }
          folder.parentId = parentId;
          return null;
        }
        case "delete_folder": {
          const folder = findFolder(args.folderId);
          if (folder.linkedRootId && folder.linkedRootId !== folder.id)
            throw {
              kind: "linkedFolder",
              message:
                "this folder mirrors a folder on disk; unlink the folder it belongs to first",
            };
          if (folder.linkedRootId === folder.id) {
            for (const f of folders)
              if (f.linkedRootId === folder.id) {
                f.linkedRootId = null;
                f.sourcePath = null;
                f.missing = false;
              }
            for (const r of repos)
              if (r.linkedRootId === folder.id) {
                r.linkedRootId = null;
                r.missing = false;
              }
          }
          for (const child of folders)
            if (child.parentId === folder.id) child.parentId = folder.parentId;
          for (const repo of repos)
            if (repo.folderId === folder.id) repo.folderId = folder.parentId;
          folders.splice(folders.indexOf(folder), 1);
          return null;
        }
        case "set_folder_collapsed":
          findFolder(args.folderId).collapsed = Boolean(args.collapsed);
          return null;
        case "move_repo": {
          const repo = repos.find((r) => r.id === args.repoId);
          if (!repo)
            throw {
              kind: "repoNotFound",
              message: `repository not found: ${String(args.repoId)}`,
            };
          const folderId = (args.folderId as string | null) ?? null;
          if (folderId) findFolder(folderId);
          repo.folderId = folderId;
          return null;
        }
        case "list_repos":
          // Copies, like real IPC: the app must never share memory with the backend.
          return structuredClone(repos);
        case "list_worktrees":
          return structuredClone(worktrees[String(args.repoId)] ?? []);
        case "repo_refs":
          return {
            defaultBase: "origin/main",
            refs: [
              "main",
              "origin/main",
              "feat/auth",
              "fix/pty",
              "origin/release",
            ],
          };
        case "plugin:dialog|open":
          return "/home/dev/web-app";
        case "add_repo":
          return addMockRepo(String(args.path));
        case "create_project": {
          const name = String(args.name).trim();
          if (!name || /[/\\:*?"<>|]/.test(name))
            throw {
              kind: "invalidName",
              message: `invalid name: ${name || "the project name is empty"}`,
            };
          return addMockRepo(`${String(args.parentPath)}/${name}`);
        }
        case "remove_repo":
          repos.splice(
            repos.findIndex((r) => r.id === args.repoId),
            1,
          );
          return null;
        case "create_worktree": {
          await delay(1200);
          const name = String(args.name).trim();
          if (/\s/.test(name)) {
            throw {
              kind: "invalidName",
              message: `invalid name: ${name} is not a valid branch name`,
            };
          }
          const repo = repos.find((r) => r.id === args.repoId);
          const created = worktree(
            repo?.path ?? "/home/dev/repo",
            name.replaceAll("/", "-"),
            name,
            String(args.baseRef ?? "origin/main"),
          );
          worktrees[String(args.repoId)] = [
            ...(worktrees[String(args.repoId)] ?? []),
            created,
          ];
          return created;
        }
        case "remove_worktree": {
          const list = worktrees[String(args.repoId)] ?? [];
          const removed = list.find((w) => w.path === args.worktreePath);
          worktrees[String(args.repoId)] = list.filter(
            (w) => w.path !== args.worktreePath,
          );
          return {
            branch: removed?.branch ?? null,
            branchDeleted: true,
            branchKeptReason: null,
          };
        }
        case "clean_up_merged_worktree": {
          await delay(120);
          // The backend asks GitHub itself: only a merged pull request gets this far.
          if (mockPrStates.get(42)?.state !== "merged") {
            throw {
              kind: "changeRefused",
              message: "the pull request is not merged",
            };
          }
          const list = worktrees[String(args.repoId)] ?? [];
          const removed = list.find((w) => w.path === args.worktreePath);
          worktrees[String(args.repoId)] = list.filter(
            (w) => w.path !== args.worktreePath,
          );
          return {
            branch: removed?.branch ?? null,
            branchDeleted: true,
            branchKeptReason: null,
          };
        }
        case "list_terminals":
          return structuredClone(sessions);
        case "pty_daemon_status":
          return { enabled: false, state: "idle", sessions: 0 };
        case "pty_daemon_memory":
          return null;
        case "hibernate_terminal":
          return null;
        case "forget_resumable_session": {
          // The backend drops every ended record of the conversation; each mock session is its own, so its id is enough.
          const id = String(args.id);
          for (const [path, rows] of Object.entries(resumable)) {
            resumable[path] = rows.filter((row) => row.id !== id);
          }
          return null;
        }
        case "list_resumable_sessions":
          return structuredClone(resumable[String(args.worktreePath)] ?? []);
        case "prepare_quit":
          return null;
        case "mark_agent_seen":
          return null;
        case "agent_catalog":
          // Agents whose CLI is missing show under "Other supported agents".
          return catalogHarnesses.map((harness) => ({
            harness,
            found: onThisMachine.has(harness),
            hooks: hookStatusOf(harness),
          }));
        case "hook_status":
          return hookStatusOf(String(args.harness));
        case "install_hooks":
          hookStates[String(args.harness)] = "installed";
          browserToolsOn.add(String(args.harness));
          return hookStatusOf(String(args.harness));
        case "remove_hooks":
          hookStates[String(args.harness)] = "notInstalled";
          browserToolsOn.delete(String(args.harness));
          return hookStatusOf(String(args.harness));
        case "browser_engine_status":
          return engineStatus();
        case "browser_engine_install":
          return await installMockEngine();
        case "browser_engine_remove":
          engineInstalled = false;
          return engineStatus();
        case "browser_open":
          if (!engineInstalled) {
            throw {
              kind: "browserEngineMissing",
              message: "the browser engine is not installed",
            };
          }
          browserState = { ...browserState, status: "running" };
          // An address given at start goes to the tab shown, if there is one.
          if (args.url) navigateShown(String(args.url));
          return browserState;
        case "browser_state":
          return browserState;
        case "browser_navigate":
          navigateShown(String(args.input));
          return null;
        case "browser_pages":
          return structuredClone(mockPages);
        case "browser_thumbnails":
          return Array.from(mockThumbnails, ([page, png]) => ({ page, png }));
        case "browser_page_new": {
          const url = (args.url as string | null) ?? null;
          const page: BrowserPageInfo = {
            id: nextPageId++,
            owner: {
              kind: "workspace",
              worktreePath: String(args.worktreePath),
            },
            url: "about:blank",
            title: "",
            loading: false,
            agent: null,
            detached: false,
            agentPage: false,
          };
          mockPages.push(page);
          setShown(page.id);
          if (url !== null) navigateShown(url);
          pagesChanged();
          return structuredClone(page);
        }
        case "browser_page_show": {
          const id = Number(args.page);
          if (!mockPages.some((page) => page.id === id)) {
            throw { kind: "browser", message: `page not found: ${id}` };
          }
          setShown(id);
          pagesChanged();
          return structuredClone(browserState);
        }
        case "browser_page_assign_agent": {
          const id = Number(args.page);
          const page = mockPages.find((item) => item.id === id);
          if (!page) {
            throw { kind: "browser", message: `page not found: ${id}` };
          }
          // One agent tab per worktree: the owner's other tabs stay open as ordinary ones.
          for (const other of mockPages) {
            if (JSON.stringify(other.owner) === JSON.stringify(page.owner)) {
              other.agentPage = other.id === id;
            }
          }
          pagesChanged();
          return null;
        }
        case "browser_page_close": {
          const id = Number(args.page);
          const index = mockPages.findIndex((page) => page.id === id);
          // Closing a tab that is gone is not an error.
          if (index === -1) return null;
          mockPages.splice(index, 1);
          mockThumbnails.delete(id);
          if (shownPage === id) setShown(null);
          pagesChanged();
          return null;
        }
        case "browser_place":
          // The mock draws no page, so where the engine's view sits does not matter.
          return null;
        case "browser_page_detach": {
          const id = Number(args.page);
          const page = mockPages.find((item) => item.id === id);
          if (!page) {
            throw { kind: "browser", message: `page not found: ${id}` };
          }
          page.detached = true;
          setShown(id);
          pagesChanged();
          return null;
        }
        case "browser_page_attach": {
          const id = Number(args.page);
          const page = mockPages.find((item) => item.id === id);
          if (!page) {
            throw { kind: "browser", message: `page not found: ${id}` };
          }
          page.detached = false;
          setShown(id);
          pagesChanged();
          return null;
        }
        case "browser_page_state": {
          const id = Number(args.page);
          const page = mockPages.find((item) => item.id === id);
          if (!page) {
            throw { kind: "browser", message: `page not found: ${id}` };
          }
          return structuredClone({
            ...browserState,
            page: id,
            detached: page.detached,
            url: page.url,
            title: page.title,
            loading: page.loading,
          });
        }
        case "write_agent_prompt":
          lastAgentPrompt = String(args.prompt);
          return "/tmp/lynx-prompts/prompt-mock.md";
        case "agent_launch_line": {
          // The same line the backend builds (src-tauri/src/agents/launch_line.rs).
          const launchers: Record<string, [string, string]> = {
            claude: ["claude", "--dangerously-skip-permissions"],
            codex: ["codex", "--dangerously-bypass-approvals-and-sandbox"],
            opencode: ["opencode", "--auto"],
            omp: ["omp", "--auto-approve"],
          };
          const harness = String(args.harness);
          const [command, yolo] = launchers[harness] ?? [harness, ""];
          const { launchMode, launchArguments } = configSnapshot.config.agents;
          const extra = (launchArguments as Record<string, string>)[
            harness
          ]?.trim();
          return [
            command,
            ...(launchMode === "yolo" ? [yolo] : []),
            ...(extra ? [extra] : []),
          ].join(" ");
        }
        case "create_terminal": {
          lastTerminalCommand = (args.command as string | null) ?? null;
          const id = addSession(
            String(args.worktreePath),
            (args.command as string | null) ?? null,
            true,
            `${green("dev@host")} $ ${args.command ?? ""}\r\n`,
          );
          return structuredClone(sessions.find((s) => s.id === id));
        }
        case "terminal_attach": {
          const id = String(args.id);
          // A real channel arrives as `__CHANNEL__:<callback id>`; its messages are numbered from 0.
          const channel = Number(
            /(\d+)/.exec(JSON.stringify(args.onOutput))?.[1],
          );
          outputChannels.set(id, { channel, index: 0 });
          const session = sessions.find((s) => s.id === id);
          if (!session)
            throw {
              kind: "terminalNotFound",
              message: `terminal not found: ${id}`,
            };
          return {
            data: transcripts.get(id) ?? "",
            lastSeq: seq.get(id) ?? 0,
            running: session.running,
            exitCode: session.exitCode,
            // The mock draws no screen: the size is the one a new terminal starts at.
            cols: 80,
            rows: 24,
          };
        }
        case "write_terminal": {
          const id = String(args.id);
          const next = (seq.get(id) ?? 0) + 1;
          seq.set(id, next);
          const echoed = String(args.data).replace(/\r/g, "\r\n");
          transcripts.set(id, (transcripts.get(id) ?? "") + echoed);
          sendOutput(id, next, echoed);
          return null;
        }
        case "resize_terminal":
          return null;
        case "plugin:notification|is_permission_granted":
          return true;
        case "close_terminal":
          sessions.splice(
            sessions.findIndex((s) => s.id === args.id),
            1,
          );
          return null;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );

  // Lets the e2e wait for commands still in flight instead of sleeping.
  let pendingCommands = 0;
  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => Promise<unknown> };
    }
  ).__TAURI_INTERNALS__;
  const mockedInvoke = internals.invoke;
  internals.invoke = async (...invokeArgs) => {
    pendingCommands += 1;
    try {
      return await mockedInvoke(...invokeArgs);
    } finally {
      pendingCommands -= 1;
    }
  };

  // Test hooks: change the config as if another program had edited the file.
  Object.assign(window, {
    __lynxMock: {
      probe,
      pendingCommands: () => pendingCommands,
      /** The prompt file the last agent was given, and the command typed to start it. */
      lastAgentRun: () => ({
        prompt: lastAgentPrompt,
        command: lastTerminalCommand,
      }),
      /** A repo is cloned into the linked folder: the next scan finds it. */
      cloneIntoLinkedFolder(name: string) {
        clonedIntoLinked.push(name);
      },
      /** The next scan stops finding this repo (or finds it again). */
      setRepoMissing(name: string, missing: boolean) {
        const repo = repos.find((r) => r.name === name);
        if (repo) repo.missing = missing;
        linkedChanged = true;
      },
      /** Replaces what the plan limits footer reports, for the e2e to show each state. */
      setPlanUsage(usages: PlanUsage[]) {
        mockPlanUsage = usages;
      },
      /** An agent made a worktree in the backend, and the screen is told so it reads the project again. */
      worktreeMadeByAgent(repoName: string, name: string) {
        const repo = repos.find((r) => r.name === repoName);
        if (!repo) return;
        worktrees[repo.id] = [
          ...(worktrees[repo.id] ?? []),
          worktree(repo.path, name, name, "origin/main"),
        ];
        void emit("worktree-created", { repoId: repo.id });
      },
      /** Pretends the remote has this many commits the branch lacks. */
      setBehind(count: number) {
        mockBehind = count;
      },
      /** An agent creates a file: the listing and the changed files have it, and the watcher reports it. */
      agentWritesFile(worktreePath: string, folder: string, name: string) {
        MOCK_FILES[folder]?.push({ name, kind: "file" });
        MOCK_CHANGES.push({
          path: folder ? `${folder}/${name}` : name,
          originalPath: null,
          staged: null,
          unstaged: "untracked",
          stagedLines: null,
          unstagedLines: { added: 3, removed: 0 },
        });
        void emit("worktree-changed", {
          worktreePath,
          directories: [folder],
          everything: false,
          git: false,
        });
      },
      /** Changes a file as if another program had written it. */
      touchFile(worktreePath: string, relative: string, content: string) {
        fileContents.set(`${worktreePath}::${relative}`, {
          content,
          hash: `ext-${++writeCounter}`,
        });
      },
      /** Ends every running program, as if each one had exited with code 0. */
      /** Changes the status of every agent session, as if the agents had reported it. */
      setAgentStatus(status: AgentStatus) {
        for (const session of sessions) {
          if (!session.harness || !session.running) continue;
          void emit("agent-status-changed", {
            id: session.id,
            harness: session.harness,
            status,
          });
        }
      },
      /** Output arriving from the backend: `chunks` events of `bytes` each, in bursts so the page can breathe. */
      async flood(id: string, chunks: number, bytes: number) {
        const line = `${"x".repeat(Math.max(1, bytes - 2))}\r\n`;
        for (let sent = 0; sent < chunks;) {
          for (let burst = 0; burst < 20 && sent < chunks; burst++, sent++) {
            const next = (seq.get(id) ?? 0) + 1;
            seq.set(id, next);
            sendOutput(id, next, line);
          }
          await new Promise((resolve) => setTimeout(resolve));
        }
      },
      sessionIds: () => sessions.map((session) => session.id),
      notifications: () => structuredClone(sentNotifications),
      /** Whether the agent's browser tools were added together with its hooks. */
      browserToolsOn: (harness: string) => browserToolsOn.has(harness),
      /** Says an agent used the browser, as the backend does on each tool call. */
      agentUsesBrowser(summary: string, agent = "Claude Code") {
        // An agent that uses the browser starts the engine, and the interface learns it first.
        browserState = { ...browserState, status: "running" };
        void emit("browser-state-changed", {
          state: structuredClone(browserState),
        });
        void emit("browser-agent-activity", {
          tool: "browser_navigate",
          summary,
          agent,
          // The mark shows only when this is the tab shown; 0 is no tab, so nothing shows.
          page: shownPage ?? 0,
        });
        // The backend lets go six seconds after the agent's last action.
        clearTimeout(agentReleaseTimer);
        agentReleaseTimer = setTimeout(
          () => void emit("browser-agent-released", {}),
          6000,
        );
      },
      endAllTerminals() {
        for (const session of sessions) {
          if (!session.running) continue;
          session.running = false;
          session.exitCode = 0;
          void emit("terminal-exit", { id: session.id, exitCode: 0 });
        }
      },
      /** Changes the plan limits settings, as if the file had been edited by hand. */
      setUsageConfig(patch: Partial<AppConfig["usage"]>) {
        configSnapshot = {
          ...configSnapshot,
          config: {
            ...configSnapshot.config,
            usage: { ...configSnapshot.config.usage, ...patch },
          },
        };
        void emit("config-changed", {
          snapshot: structuredClone(configSnapshot),
        });
      },
      editConfigFile(patch: Partial<AppConfig["terminal"]>) {
        configSnapshot = {
          ...configSnapshot,
          config: {
            ...configSnapshot.config,
            terminal: { ...configSnapshot.config.terminal, ...patch },
          },
        };
        void emit("config-changed", {
          snapshot: structuredClone(configSnapshot),
        });
      },
    },
  });
}
