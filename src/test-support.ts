import type { IpcError, TerminalInfo, WorktreeInfo } from "@/bindings";

export const ok = <T>(data: T) => ({ status: "ok" as const, data });

export const failure = (kind: IpcError["kind"], message: string) => ({
  status: "error" as const,
  error: { kind, message },
});

export function worktree(
  path: string,
  overrides: Partial<WorktreeInfo> = {},
): WorktreeInfo {
  const name = path.split("/").at(-1) ?? path;
  return {
    path,
    name,
    branch: name,
    head: "abc1234def",
    baseRef: null,
    isMain: false,
    locked: false,
    prunable: false,
    ...overrides,
  };
}

export function session(
  id: string,
  worktreePath: string,
  overrides: Partial<TerminalInfo> = {},
): TerminalInfo {
  return {
    id,
    worktreePath,
    command: null,
    shell: "zsh",
    running: true,
    exitCode: null,
    startedAt: 1_800_000_000,
    harness: null,
    agentStatus: null,
    ...overrides,
  };
}
