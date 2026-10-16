/** How many agent badges a collapsed group shows before it says "+X". */
export const MAX_AGENT_BADGES = 3;

/** The first agents, up to the limit, and how many were left out. */
export function splitAgentBadges<T>(agents: readonly T[]): {
  shown: T[];
  hidden: number;
} {
  const shown = agents.slice(0, MAX_AGENT_BADGES);
  return { shown, hidden: agents.length - shown.length };
}
