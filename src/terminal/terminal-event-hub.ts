import type { UnlistenFn } from "@tauri-apps/api/event";

import { events, type TerminalExit } from "@/bindings";

/**
 * One listener for every terminal, then a lookup by session. A listener per terminal would run
 * its callback for every event of every other terminal, and all of them stay mounted.
 */
export function createEventHub<T extends { id: string }>(
  listen: (handler: (event: { payload: T }) => void) => Promise<UnlistenFn>,
) {
  const subscribers = new Map<string, Set<(payload: T) => void>>();
  let listening: Promise<UnlistenFn> | null = null;

  const start = () => {
    listening ??= listen(({ payload }) => {
      subscribers.get(payload.id)?.forEach((subscriber) => subscriber(payload));
    });
    return listening.then(() => undefined);
  };

  return {
    /** Resolves once events are flowing, so a caller can ask for a snapshot without a gap. */
    subscribe(id: string, subscriber: (payload: T) => void) {
      let ofSession = subscribers.get(id);
      if (!ofSession) subscribers.set(id, (ofSession = new Set()));
      ofSession.add(subscriber);
      return {
        ready: start(),
        unsubscribe() {
          ofSession.delete(subscriber);
          if (ofSession.size === 0) subscribers.delete(id);
        },
      };
    },
  };
}

export const terminalExitHub = createEventHub<TerminalExit>((handler) =>
  events.terminalExit.listen(handler),
);
