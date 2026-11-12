import type { AgentStatus, HarnessId, TaskState } from "@/bindings";

/** A member's card status: the agent's own status while its session runs, "closed" once the session is gone or its program ended. A worker's card shows its worker state instead. */
export type TeamStatus = AgentStatus | "closed" | "failed";

/** A worker's state as the orchestration gives it. */
export type WorkerState = "running" | "done" | "failed" | "stopped";

/** What a member's last message says: a report, a failure, a question to the user, or any kind Lynx does not know yet. */
export type TeamActionKind =
  "done" | "failed" | "noReport" | "needsYou" | "question" | "answer" | "other";

export interface TeamActivity {
  text: string;
  /** Epoch seconds. */
  at: number;
  /** The kind of the message the text comes from. Absent for a text that is not a message. */
  kind?: TeamActionKind;
}

/** What every member has, whether it is an agent or a bot. */
export interface TeamMemberBase {
  id: string;
  /** The absolute path of the worktree whose team the member is in. */
  worktree: string;
  /** What the member does in its team. */
  role: string;
  status: TeamStatus;
  /** The state of the worker this member is, when a coordinator started it; null for every other member. */
  workerState: WorkerState | null;
  /** The orchestrator this member answers to; null when it belongs to none. */
  parentId: string | null;
  /** Whether it is an orchestrator. Only one can have members under it. */
  orchestrator: boolean;
  task: string;
  /** When the task counts as done. */
  acceptance: string;
  /** Where the task stands; null for a member with no task. */
  taskState: TaskState | null;
  /** The members whose tasks must be done before this one starts. */
  dependsOn: string[];
  /** The worktree the worker runs in, when it is not the team's own. */
  executionWorktree: string | null;
  /** What the member did last, and when. */
  activity: TeamActivity;
  branch: string | null;
  added: number;
  removed: number;
  /** A member whose result it waits for, by id. */
  waitingOn: string | null;
  /** A file it has read, shown as a chip. */
  note: string | null;
  /** What it asks the user while it needs them. */
  question: string | null;
  /** Simulated terminal lines, newest last. */
  output: string[];
}

/** A CLI agent in a worktree's team: a session in that worktree, run by its harness. */
export interface TeamAgent extends TeamMemberBase {
  kind: "agent";
  /** Null until the first worker starts for a member the orchestrator added. */
  harness: HarnessId | null;
  /**
   * The terminal session the agent runs in, cleared when it closes. Null for most seeded fixtures; the login tests one
   * names a session that ended, which the mock lists as resumable.
   */
  sessionId: string | null;
}

/**
 * A Lynx bot in a worktree's team. It only orchestrates: always an orchestrator, never under anyone, with agents under it.
 * Its name is the member's role; its avatar, engine and model come from the Bots list, by `botId`.
 */
export interface TeamBot extends TeamMemberBase {
  kind: "bot";
  botId: string;
}

/** One member of a worktree's team. Mock data: nothing here starts or reads a real session. */
export type TeamMember = TeamAgent | TeamBot;

/** The mock worktrees the seeded teams are in: the "feat/auth" one of lynx-ide and the "docs/readme" one of api-server. */
export const TEAM_AUTH_WORKTREE = "/home/dev/lynx-ide-worktrees/feat-auth";
export const TEAM_DOCS_WORKTREE = "/home/dev/api-server-worktrees/docs-readme";
export const TEAM_ROOT_ID = "lead";
/** The member that needs the user, so the team view opens on something to answer. */
export const TEAM_NEEDS_YOU_ID = "researcher";
/** The pairs the teams start with, by member id: a permission to talk. */
export const TEAM_PAIRS: readonly [string, string][] = [
  ["builder", "reviewer"],
];

// The activity times are relative to when the mock loaded, so they read as minutes ago.
const NOW_SECONDS = Math.floor(Date.now() / 1000);

/** The roles a member can be given. The coordinator keeps its own. */
export const TEAM_ROLES = [
  "Builder",
  "Reviewer",
  "Researcher",
  "Tester",
] as const;

export const TEAM_MEMBERS: readonly TeamMember[] = [
  {
    id: "lead",
    kind: "agent",
    workerState: null,
    worktree: TEAM_AUTH_WORKTREE,
    role: "Lead",
    harness: "claude",
    sessionId: null,
    status: "working",
    parentId: null,
    orchestrator: true,
    // Only a member with an orchestrator above it has a task.
    task: "",
    acceptance: "",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "splitting the work", at: NOW_SECONDS - 240 },
    branch: null,
    added: 0,
    removed: 0,
    waitingOn: null,
    note: null,
    question: null,
    output: [
      "Split the work: login form to Builder, auth suite to Tester.",
      "Waiting on the OAuth library choice from Researcher.",
    ],
  },
  {
    id: "builder",
    kind: "agent",
    workerState: null,
    worktree: TEAM_AUTH_WORKTREE,
    role: "Builder",
    harness: "codex",
    sessionId: null,
    status: "working",
    parentId: "lead",
    orchestrator: true,
    task: "login form",
    acceptance:
      "The login form submits through the new adapter and its tests pass.",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "editing LoginForm.tsx", at: NOW_SECONDS - 120 },
    branch: "feat/login",
    added: 120,
    removed: 14,
    waitingOn: null,
    note: null,
    question: null,
    output: [
      "Editing src/auth/login_form.tsx",
      "Adding the submit handler",
      "Running the login tests",
    ],
  },
  {
    id: "login-tests",
    kind: "agent",
    workerState: null,
    worktree: TEAM_AUTH_WORKTREE,
    role: "Tester",
    harness: "claude",
    // Its session ended: the mock lists it as resumable, so its terminal shows Resume.
    sessionId: "ended-login-tests",
    status: "idle",
    parentId: "builder",
    orchestrator: false,
    task: "login tests",
    acceptance: "The login tests pass against the new form.",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "waiting for the form", at: NOW_SECONDS - 300 },
    branch: null,
    added: 0,
    removed: 0,
    waitingOn: "builder",
    note: null,
    question: null,
    output: ["Waiting for Builder to finish the login form."],
  },
  {
    id: "reviewer",
    kind: "agent",
    workerState: null,
    worktree: TEAM_AUTH_WORKTREE,
    role: "Reviewer",
    harness: "claude",
    sessionId: null,
    status: "idle",
    // No orchestrator: no task, only conversation with Builder.
    parentId: null,
    orchestrator: false,
    task: "",
    acceptance: "",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "read spec.md", at: NOW_SECONDS - 600 },
    branch: null,
    added: 0,
    removed: 0,
    waitingOn: null,
    note: "spec.md (read)",
    question: null,
    output: ["Read spec.md", "Waiting for Builder to finish the login form."],
  },
  {
    id: "researcher",
    kind: "agent",
    workerState: null,
    worktree: TEAM_AUTH_WORKTREE,
    role: "Researcher",
    harness: "opencode",
    sessionId: null,
    status: "needsYou",
    parentId: "lead",
    orchestrator: false,
    task: "OAuth options",
    acceptance: "One OAuth library is chosen, with the reason written down.",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "compared three libraries", at: NOW_SECONDS - 900 },
    branch: null,
    added: 0,
    removed: 0,
    waitingOn: null,
    note: null,
    question: "which OAuth lib?",
    output: [
      "Compared three OAuth libraries.",
      "Need a choice before writing the adapter.",
    ],
  },
  {
    id: "tester",
    kind: "agent",
    workerState: null,
    worktree: TEAM_AUTH_WORKTREE,
    role: "Tester",
    harness: "omp",
    sessionId: null,
    status: "done",
    parentId: "lead",
    orchestrator: false,
    task: "auth suite",
    acceptance: "The auth suite runs green.",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "42 tests passed", at: NOW_SECONDS - 1800 },
    branch: null,
    added: 0,
    removed: 0,
    waitingOn: null,
    note: null,
    question: null,
    output: ["auth suite: 42 passed", "Done."],
  },
];

/** A second, independent team in another worktree, so the Team view has another one to show. */
export const TEAM_DOCS_MEMBERS: readonly TeamMember[] = [
  {
    id: "docs-lead",
    kind: "bot",
    workerState: null,
    // The Lynx bot it is: the one the Bots list has as "Lead" in the full mock.
    botId: "lead",
    worktree: TEAM_DOCS_WORKTREE,
    role: "Lead",
    status: "working",
    parentId: null,
    orchestrator: true,
    task: "",
    acceptance: "",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "outlining the guide", at: NOW_SECONDS - 200 },
    branch: null,
    added: 0,
    removed: 0,
    waitingOn: null,
    note: null,
    question: null,
    output: ["Outlined the guide: setup, usage, reference."],
  },
  {
    id: "writer",
    kind: "agent",
    workerState: null,
    worktree: TEAM_DOCS_WORKTREE,
    role: "Writer",
    harness: "claude",
    sessionId: null,
    status: "working",
    parentId: "docs-lead",
    orchestrator: false,
    task: "setup chapter",
    acceptance: "The setup chapter is written and reviewed.",
    taskState: null,
    dependsOn: [],
    executionWorktree: null,
    activity: { text: "drafting setup.md", at: NOW_SECONDS - 90 },
    branch: "docs/setup",
    added: 64,
    removed: 2,
    waitingOn: null,
    note: null,
    question: null,
    output: ["Drafting docs/setup.md"],
  },
];
