import type { AgentStatus, HarnessId } from "@/bindings";

export interface TeamActivity {
  text: string;
  /** Epoch seconds. */
  at: number;
}

/** What every member has, whether it is an agent or a bot. */
export interface TeamMemberBase {
  id: string;
  /** The run it belongs to: each run has its own goal and its own team. */
  runId: string;
  /** What the member does in the run. "Reassign role" changes it. */
  role: string;
  status: AgentStatus;
  /** The orchestrator this member answers to; null when it belongs to none. */
  parentId: string | null;
  /** Whether it is the run's one orchestrator. Only it can have members under it. */
  orchestrator: boolean;
  task: string;
  /** When the task counts as done. */
  acceptance: string;
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

/** A CLI agent in the run: a session in a worktree, run by its harness. */
export interface TeamAgent extends TeamMemberBase {
  kind: "agent";
  harness: HarnessId;
}

/**
 * A Lynx bot in the run. It only orchestrates: always an orchestrator, never under anyone, with agents under it.
 * Its name is the member's role; its avatar, engine and model come from the Bots list, by `botId`.
 */
export interface TeamBot extends TeamMemberBase {
  kind: "bot";
  botId: string;
}

/** One member of a run. Mock data: nothing here starts or reads a real session. */
export type TeamMember = TeamAgent | TeamBot;

export const TEAM_RUN_ID = "auth";
export interface TeamRun {
  id: string;
  /** The run's goal. It is not a task of any member. */
  title: string;
}
export const TEAM_RUNS: readonly TeamRun[] = [
  { id: TEAM_RUN_ID, title: "Auth refactor" },
  { id: "docs", title: "Docs rewrite" },
];
export const TEAM_ROOT_ID = "lead";
/** The member that needs the user, so the team view opens on something to answer. */
export const TEAM_NEEDS_YOU_ID = "researcher";
/** The pairs the run starts with, by member id: a permission to talk. */
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
    runId: TEAM_RUN_ID,
    role: "Lead",
    harness: "claude",
    status: "working",
    parentId: null,
    orchestrator: true,
    // The run's goal is not a task: only a member with an orchestrator above it has one.
    task: "",
    acceptance: "",
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
    runId: TEAM_RUN_ID,
    role: "Builder",
    harness: "codex",
    status: "working",
    parentId: "lead",
    orchestrator: true,
    task: "login form",
    acceptance:
      "The login form submits through the new adapter and its tests pass.",
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
    runId: TEAM_RUN_ID,
    role: "Tester",
    harness: "claude",
    status: "idle",
    parentId: "builder",
    orchestrator: false,
    task: "login tests",
    acceptance: "The login tests pass against the new form.",
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
    runId: TEAM_RUN_ID,
    role: "Reviewer",
    harness: "claude",
    status: "idle",
    // No orchestrator: no task, only conversation with Builder.
    parentId: null,
    orchestrator: false,
    task: "",
    acceptance: "",
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
    runId: TEAM_RUN_ID,
    role: "Researcher",
    harness: "opencode",
    status: "needsYou",
    parentId: "lead",
    orchestrator: false,
    task: "OAuth options",
    acceptance: "One OAuth library is chosen, with the reason written down.",
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
    runId: TEAM_RUN_ID,
    role: "Tester",
    harness: "omp",
    status: "done",
    parentId: "lead",
    orchestrator: false,
    task: "auth suite",
    acceptance: "The auth suite runs green.",
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

/** A second, independent run, so the run selector has something to switch to. */
export const TEAM_DOCS_MEMBERS: readonly TeamMember[] = [
  {
    id: "docs-lead",
    kind: "bot",
    // The Lynx bot it is: the one the Bots list has as "Lead" in the full mock.
    botId: "lead",
    runId: "docs",
    role: "Lead",
    status: "working",
    parentId: null,
    orchestrator: true,
    task: "",
    acceptance: "",
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
    runId: "docs",
    role: "Writer",
    harness: "claude",
    status: "working",
    parentId: "docs-lead",
    orchestrator: false,
    task: "setup chapter",
    acceptance: "The setup chapter is written and reviewed.",
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
