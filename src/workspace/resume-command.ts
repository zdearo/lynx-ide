/**
 * The line that resumes an agent's conversation: the line it is launched with, then the arguments
 * that name the conversation. Null for a plain shell, which has no conversation.
 */
export function resumeCommand(
  launch: string | null,
  resumeArguments: readonly string[],
): string | null {
  return launch === null ? null : [launch, ...resumeArguments].join(" ");
}
