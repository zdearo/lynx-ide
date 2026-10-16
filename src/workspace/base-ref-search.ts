import type { RepoRefs } from "@/bindings";

/** Most rows the base picker renders at once; a narrower search reaches the rest. */
export const BASE_REF_LIMIT = 100;

/** The default ref first, then every other one in the order the backend sent. */
export function baseRefOptions(refs: RepoRefs | null): string[] {
  if (!refs) return [];
  return [
    refs.defaultBase,
    ...refs.refs.filter((ref) => ref !== refs.defaultBase),
  ];
}

/**
 * Case-insensitive substring match. Returns at most `limit` rows, plus the
 * full match count so the picker can say how many were left out.
 */
export function searchBaseRefs(
  options: string[],
  query: string,
  limit = BASE_REF_LIMIT,
): { shown: string[]; total: number } {
  const needle = query.trim().toLowerCase();
  const matches = needle
    ? options.filter((ref) => ref.toLowerCase().includes(needle))
    : options;
  return { shown: matches.slice(0, limit), total: matches.length };
}
