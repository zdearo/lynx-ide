/**
 * Runs `task` for every item with at most `limit` running at once. Each item still runs when
 * another fails; the first failure is thrown at the end.
 */
export async function forEachBounded<T>(
  items: readonly T[],
  limit: number,
  task: (item: T) => Promise<void>,
): Promise<void> {
  let next = 0;
  let firstFailure: { error: unknown } | null = null;

  const worker = async () => {
    while (next < items.length) {
      const item = items[next++];
      try {
        await task(item);
      } catch (error) {
        firstFailure ??= { error };
      }
    }
  };

  const workers = Array.from(
    { length: Math.max(1, Math.min(limit, items.length)) },
    worker,
  );
  await Promise.all(workers);
  if (firstFailure) throw (firstFailure as { error: unknown }).error;
}
