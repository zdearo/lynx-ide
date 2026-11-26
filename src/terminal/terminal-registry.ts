/**
 * The terminals of the backend sessions, kept outside React. A view that unmounts only lets go of its element: the
 * terminal keeps its screen and its scrollback until its session is gone, and the element goes back into the next view
 * that shows the session. A session has one element, so it is in one view at a time.
 */

/** The part of an element the registry moves about. */
export interface HostNode {
  readonly parentNode: unknown;
  remove(): void;
}

/** Where an element can go: the container of a view. */
export interface HostSlot<H> {
  appendChild(node: H): unknown;
}

/** One session's element, and the terminal built on it once a view has made one. */
export interface Hosted<T, H> {
  readonly host: H;
  /** The terminal, once made. It is disposed only when its session is gone. */
  value: T | null;
  /** The view that holds the element now. */
  holder: object | null;
  /** What the terminal reports to the view that holds it now. */
  handlers: {
    onExit: (exitCode: number | null) => void;
    onReplayed: (value: T) => void;
  };
}

export class TerminalHosts<T extends { dispose(): void }, H extends HostNode> {
  private readonly hosted = new Map<string, Hosted<T, H>>();

  constructor(private readonly createHost: () => H) {}

  /** The session's entry, made the first time it is asked for. */
  entryOf(id: string): Hosted<T, H> {
    let entry = this.hosted.get(id);
    if (!entry) {
      entry = {
        host: this.createHost(),
        value: null,
        holder: null,
        handlers: { onExit: () => {}, onReplayed: () => {} },
      };
      this.hosted.set(id, entry);
    }
    return entry;
  }

  /** Whether the session already has its entry: a session shown again needs no set-up. */
  has(id: string): boolean {
    return this.hosted.has(id);
  }

  /** Puts the session's element into `slot` for `holder`. Taking it from another view takes it away from that one. */
  place(id: string, holder: object, slot: HostSlot<H>): Hosted<T, H> {
    const entry = this.entryOf(id);
    entry.holder = holder;
    if (entry.host.parentNode !== slot) slot.appendChild(entry.host);
    return entry;
  }

  /** Lets go of the element, when `holder` still holds it. The terminal stays alive. Returns whether it held it. */
  release(id: string, holder: object): boolean {
    const entry = this.hosted.get(id);
    if (!entry || entry.holder !== holder) return false;
    entry.holder = null;
    entry.host.remove();
    return true;
  }

  /** Disposes the terminal of every session that is not in `liveIds` any more. */
  prune(liveIds: ReadonlySet<string>): void {
    for (const [id, entry] of this.hosted) {
      if (liveIds.has(id)) continue;
      entry.value?.dispose();
      entry.host.remove();
      this.hosted.delete(id);
    }
  }
}
