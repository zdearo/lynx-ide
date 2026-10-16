/**
 * In-memory stand-in for the Rust backend, so the interface can be opened in a
 * plain browser (`?mock=full` or `?mock=empty`). Development only: main.tsx
 * imports it behind `import.meta.env.DEV`, so it never reaches a release build.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import type {
  AppConfig,
  ConfigSnapshot,
  Folder,
  HarnessId,
  Repo,
  TerminalInfo,
  WorktreeInfo,
} from "@/bindings";

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
  `${ESC}44;30m \uf07c ~/agent-ade ${ESC}0m${ESC}34m\ue0b0${ESC}0m ${green("\ue0a0 feat/auth")} ${orange("\uf113")}`,
  `${green("dev@host")} ${dim("~/agent-ade")} $ git status -sb`,
  "## feat/auth",
  " M src/workspace/Sidebar.tsx",
  "?? src/dev/mock-tauri.ts",
  `${green("dev@host")} ${dim("~/agent-ade")} $ `,
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
      name: "agent-ade",
      path: "/home/dev/agent-ade",
      folderId: null,
    },
    {
      id: "r2",
      name: "api-server",
      path: "/home/dev/api-server",
      folderId: "f-client",
    },
  ];
  return {
    repos,
    worktrees: {
      r1: [
        worktree(repos[0].path, "agent-ade", "main", null, true),
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

const DEFAULT_CONFIG: AppConfig = {
  terminal: {
    fontFamily: null,
    fontSize: 13,
    shell: null,
    scrollback: 5000,
    copyOnSelect: false,
    bell: "visual",
    macOptionIsMeta: false,
    closeOnExit: false,
    confirmClose: false,
    colorScheme: "default",
    textColorScheme: null,
  },
  worktrees: { directory: null },
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

export function installMockTauri(scenario: string): void {
  const seeded =
    scenario === "empty" ? { repos: [], worktrees: {} } : seedRepos();
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
          { id: "f-work", name: "Work", parentId: null, collapsed: false },
          {
            id: "f-client",
            name: "Client X",
            parentId: "f-work",
            collapsed: false,
          },
        ];
  let nextFolderId = 0;
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
    };
    repos.push(repo);
    worktrees[repo.id] = [worktree(path, repo.name, "main", null, true)];
    return repo;
  };

  let nextSessionId = 0;
  const sessions: TerminalInfo[] = [];
  const transcripts = new Map<string, string>();
  const seq = new Map<string, number>();

  // The program each agent's CLI runs as, which is how a typed command names its agent.
  const programs: Record<string, HarnessId> = {
    claude: "claude",
    codex: "codex",
    opencode: "opencode",
    agy: "antigravity",
    "cursor-agent": "cursor",
    copilot: "copilot",
    grok: "grok",
    droid: "droid",
    "kiro-cli": "kiro",
    qwen: "qwen",
    pi: "pi",
    omp: "omp",
  };
  // Which agent a typed command starts, as the backend decides it.
  function harnessOf(command: string | null) {
    const program = command?.trim().split(/\s+/)[0];
    return program ? (programs[program] ?? null) : null;
  }

  // Mirrors the backend's hook state per harness so the Agents settings can be exercised in dev.
  const hookStates: Record<string, "notInstalled" | "installed"> = {};
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
  const catalogHarnesses = [
    "claude",
    "codex",
    "opencode",
    "antigravity",
    "cursor",
    "copilot",
    "grok",
    "droid",
    "kiro",
    "qwen",
    "pi",
    "omp",
  ] as const;
  const onThisMachine = new Set([
    "claude",
    "codex",
    "antigravity",
    "grok",
    "omp",
  ]);
  function hookStatusOf(harness: string) {
    const file = hookFiles[harness] ?? hookFiles.claude;
    return {
      state: hookStates[harness] ?? "notInstalled",
      configPath: file.path,
      events: file.events,
      detail: null,
    };
  }

  function addSession(
    path: string,
    command: string | null,
    running: boolean,
    text: string,
    startedSecondsAgo = 0,
  ) {
    const id = `t${++nextSessionId}`;
    sessions.push({
      id,
      worktreePath: path,
      command,
      shell: "zsh",
      running,
      exitCode: running ? null : 0,
      startedAt: Math.floor(Date.now() / 1000) - startedSecondsAgo,
      harness: harnessOf(command),
      agentStatus: harnessOf(command) && running ? "idle" : null,
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
        "/home/dev/agent-ade-worktrees/fix-pty",
        command,
        true,
        AGENT_TRANSCRIPT,
        secondsAgo,
      );
    }
  }

  if (scenario !== "empty") {
    addSession(
      "/home/dev/agent-ade-worktrees/feat-auth",
      "claude",
      true,
      AGENT_TRANSCRIPT,
      5 * 60,
    );
    addSession(
      "/home/dev/agent-ade-worktrees/feat-auth",
      null,
      true,
      SHELL_TRANSCRIPT,
    );
    addSession(
      "/home/dev/agent-ade-worktrees/fix-pty",
      "codex",
      true,
      AGENT_TRANSCRIPT,
      3 * 3600,
    );
    addSession(
      "/home/dev/api-server-worktrees/refactor-db",
      "claude",
      false,
      AGENT_TRANSCRIPT,
      2 * 86400,
    );
  }

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
        case "reveal_in_file_manager":
          probe.revealed = `${String(args.worktreePath)}::${String(args.relativePath)}`;
          return null;
        case "list_shells":
          return ["/bin/bash", "/bin/zsh", "/usr/bin/fish"];
        case "get_config":
          return structuredClone(configSnapshot);
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
          };
          folders.push(folder);
          return structuredClone(folder);
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
        case "list_terminals":
          return structuredClone(sessions);
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
          return hookStatusOf(String(args.harness));
        case "remove_hooks":
          hookStates[String(args.harness)] = "notInstalled";
          return hookStatusOf(String(args.harness));
        case "create_terminal": {
          const id = addSession(
            String(args.worktreePath),
            (args.command as string | null) ?? null,
            true,
            `${green("dev@host")} $ ${args.command ?? ""}\r\n`,
          );
          return structuredClone(sessions.find((s) => s.id === id));
        }
        case "terminal_snapshot": {
          const id = String(args.id);
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
          };
        }
        case "write_terminal": {
          const id = String(args.id);
          const next = (seq.get(id) ?? 0) + 1;
          seq.set(id, next);
          const echoed = String(args.data).replace(/\r/g, "\r\n");
          transcripts.set(id, (transcripts.get(id) ?? "") + echoed);
          void emit("terminal-output", { id, seq: next, data: echoed });
          return null;
        }
        case "resize_terminal":
          return null;
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

  // Test hooks: change the config as if another program had edited the file.
  Object.assign(window, {
    __lynxMock: {
      probe,
      /** Ends every running program, as if each one had exited with code 0. */
      endAllTerminals() {
        for (const session of sessions) {
          if (!session.running) continue;
          session.running = false;
          session.exitCode = 0;
          void emit("terminal-exit", { id: session.id, exitCode: 0 });
        }
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
