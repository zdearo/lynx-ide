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
    { id: "r1", name: "agent-ade", path: "/home/dev/agent-ade" },
    { id: "r2", name: "api-server", path: "/home/dev/api-server" },
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
    file("pnpm-lock.yaml"),
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
  terminal: { fontFamily: null, fontSize: 13, shell: null, scrollback: 5000 },
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
  const worktrees: Record<string, WorktreeInfo[]> = { ...seeded.worktrees };

  let nextSessionId = 0;
  const sessions: TerminalInfo[] = [];
  const transcripts = new Map<string, string>();
  const seq = new Map<string, number>();

  function addSession(
    path: string,
    command: string | null,
    running: boolean,
    text: string,
  ) {
    const id = `t${++nextSessionId}`;
    sessions.push({
      id,
      worktreePath: path,
      command,
      shell: "zsh",
      running,
      exitCode: running ? null : 0,
    });
    transcripts.set(id, text);
    seq.set(id, 1);
    return id;
  }

  if (scenario !== "empty") {
    addSession(
      "/home/dev/agent-ade-worktrees/feat-auth",
      "claude",
      true,
      AGENT_TRANSCRIPT,
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
    );
    addSession(
      "/home/dev/api-server-worktrees/refactor-db",
      "claude",
      false,
      AGENT_TRANSCRIPT,
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
        case "add_repo": {
          const path = String(args.path);
          const repo = {
            id: `r${repos.length + 1}`,
            name: path.split("/").at(-1) ?? path,
            path,
          };
          repos.push(repo);
          worktrees[repo.id] = [worktree(path, repo.name, "main", null, true)];
          return repo;
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
