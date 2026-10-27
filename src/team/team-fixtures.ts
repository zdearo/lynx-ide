import type { AgentStatus, HarnessId } from "@/bindings";

export interface TeamActivity {
  text: string;
  /** Epoch seconds. */
  at: number;
}

/** One agent in the run. Mock data: nothing here starts or reads a real session. */
export interface TeamMember {
  id: string;
  /** What the member does in the run. "Reassign role" changes it. */
  role: string;
  harness: HarnessId;
  status: AgentStatus;
  /** The member that recruited this one; null for the coordinator. */
  parentId: string | null;
  task: string;
  /** When the task counts as done. */
  acceptance: string;
  /** What the member did last, and when. */
  activity: TeamActivity;
  branch: string | null;
  added: number;
  removed: number;
  /** A member it reviews or works alongside, by id. */
  pairedWith: string | null;
  /** A member whose result it waits for, by id. */
  waitingOn: string | null;
  /** A file it has read, shown as a chip. */
  note: string | null;
  /** What it asks the user while it needs them. */
  question: string | null;
  /** Simulated terminal lines, newest last. */
  output: string[];
}

export const TEAM_RUN_TITLE = "Auth refactor";
export const TEAM_ROOT_ID = "lead";
/** The member that needs the user, so the team view opens on something to answer. */
export const TEAM_NEEDS_YOU_ID = "researcher";

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
    role: "Lead",
    harness: "claude",
    status: "working",
    parentId: null,
    task: "Coordinate the auth refactor",
    acceptance: "The auth refactor is merged and every member is done.",
    activity: { text: "splitting the work", at: NOW_SECONDS - 240 },
    branch: null,
    added: 0,
    removed: 0,
    pairedWith: null,
    waitingOn: null,
    note: null,
    question: null,
    output: [
      "Split the work: login form to Builder, review to Reviewer.",
      "Waiting on the OAuth library choice from Researcher.",
    ],
  },
  {
    id: "builder",
    role: "Builder",
    harness: "codex",
    status: "working",
    parentId: "lead",
    task: "login form",
    acceptance:
      "The login form submits through the new adapter and its tests pass.",
    activity: { text: "editing LoginForm.tsx", at: NOW_SECONDS - 120 },
    branch: "feat/login",
    added: 120,
    removed: 14,
    pairedWith: "reviewer",
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
    id: "reviewer",
    role: "Reviewer",
    harness: "claude",
    status: "idle",
    parentId: "lead",
    task: "review login",
    acceptance: "Every finding on the login form is answered or fixed.",
    activity: { text: "read spec.md", at: NOW_SECONDS - 600 },
    branch: null,
    added: 0,
    removed: 0,
    pairedWith: "builder",
    waitingOn: "builder",
    note: "spec.md (read)",
    question: null,
    output: ["Read spec.md", "Waiting for Builder to finish the login form."],
  },
  {
    id: "researcher",
    role: "Researcher",
    harness: "opencode",
    status: "needsYou",
    parentId: "lead",
    task: "OAuth options",
    acceptance: "One OAuth library is chosen, with the reason written down.",
    activity: { text: "compared three libraries", at: NOW_SECONDS - 900 },
    branch: null,
    added: 0,
    removed: 0,
    pairedWith: null,
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
    role: "Tester",
    harness: "omp",
    status: "done",
    parentId: "lead",
    task: "auth suite",
    acceptance: "The auth suite runs green.",
    activity: { text: "42 tests passed", at: NOW_SECONDS - 1800 },
    branch: null,
    added: 0,
    removed: 0,
    pairedWith: null,
    waitingOn: null,
    note: null,
    question: null,
    output: ["auth suite: 42 passed", "Done."],
  },
];
