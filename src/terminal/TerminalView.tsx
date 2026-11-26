import { memo, useCallback, useEffect, useLayoutEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { commands, type TerminalConfig } from "@/bindings";
import {
  currentTerminalConfig,
  useTerminalConfig,
} from "@/config/config-store";
import { cn } from "@/lib/utils";
import { unwrap } from "@/lib/ipc-result";
import {
  terminalFontFamily,
  terminalFontsReady,
} from "@/terminal/terminal-appearance";
import { resolveTerminalTheme } from "@/terminal/terminal-color-schemes";
import { createResizeSender } from "@/terminal/terminal-resize-sender";
import {
  webglBudget,
  type WebglRendererSlot,
} from "@/terminal/terminal-renderer";
import { useTerminalStore } from "@/terminal/terminal-store";
import { terminalExitHub } from "@/terminal/terminal-event-hub";
import { attachTerminalOutput } from "@/terminal/terminal-output-stream";
import { TerminalStreamGate } from "@/terminal/terminal-stream-gate";
import { TerminalWriteQueue } from "@/terminal/terminal-write-queue";
import { queueTerminalFrame } from "@/terminal/terminal-frame-queue";
import { TerminalHosts, type Hosted } from "@/terminal/terminal-registry";

interface TerminalViewProps {
  terminalId: string;
  /** Only the visible terminal is fitted and focused. */
  active: boolean;
  /** Whether this terminal takes the keyboard when it is shown: of several open at once, only the picked one does. */
  focused?: boolean;
  /** Whether the terminal is on the team's screen now. Coming back asks for its GPU renderer again, so it counts as seen. */
  inView?: boolean;
  /** The screen is fitted, but the process keeps its size until this is released (a card moving to its size). */
  resizeHeld?: boolean;
  /** A node is being dragged to a new size: the screen follows it, and the process gets the size a few times meanwhile. */
  resizeLive?: boolean;
  onExit?: (exitCode: number | null) => void;
  className?: string;
}

/** While a node is being resized, the process gets the size at most this often: each size makes the agent redraw. */
const LIVE_RESIZE_MS = 120;

interface Mounted {
  terminal: Terminal;
  /** Fits the screen to its box and tells the process the size, even when it did not change. */
  refit: () => void;
  /** Keeps the process's size while held; releasing sends the screen's latest size. */
  holdResize: (held: boolean) => void;
  /** The card this terminal sits in has finished its size transition: the process gets the size it landed on. */
  landed: () => void;
  /** While a node is dragged, the screen follows every frame and the process gets the size at most every LIVE_RESIZE_MS. */
  setLiveResize: (live: boolean) => void;
  /** WebGL is attached only while this terminal is the visible one. */
  webgl: WebglRendererSlot;
  /** Whether the terminal is shown now: a terminal that is not shown does not follow the window. */
  setShown: (shown: boolean) => void;
  /** Whether the snapshot is applied; the GPU renderer waits for it, so the replay is drawn by the DOM view. */
  replayed: boolean;
  dispose: () => void;
}

/** Opens xterm in `container` and wires it to one backend session. */
/** The room around the text: the same at the left, and at the right where the scrollbar sits. */
const TERMINAL_PADDING_PX = 8;

// The sessions' elements and terminals, outside React: a view that unmounts only lets go of its element.
const terminalHosts = new TerminalHosts<Mounted, HTMLDivElement>(() => {
  const host = document.createElement("div");
  host.className = "size-full";
  return host;
});

/** Applies the settings to a terminal that is already made: its font, scrollback and colours. */
function restyleTerminal(terminal: Terminal, settings: TerminalConfig) {
  terminal.options.fontFamily = terminalFontFamily(settings.fontFamily);
  terminal.options.fontSize = settings.fontSize;
  terminal.options.scrollback = settings.scrollback;
  terminal.options.macOptionIsMeta = settings.macOptionIsMeta;
  terminal.options.theme = resolveTerminalTheme(settings);
}

function mountTerminal(
  container: HTMLDivElement,
  terminalId: string,
  handlers: Hosted<Mounted, HTMLDivElement>["handlers"],
): Mounted {
  const getSettings = (): TerminalConfig => currentTerminalConfig();
  const settings = getSettings();
  const terminal = new Terminal({
    cursorBlink: true,
    fontFamily: terminalFontFamily(settings.fontFamily),
    fontSize: settings.fontSize,
    scrollback: settings.scrollback,
    macOptionIsMeta: settings.macOptionIsMeta,
    theme: resolveTerminalTheme(settings),
    // Programs pick their own colours; this keeps any text readable (WCAG AA).
    minimumContrastRatio: 4.5,
    // FitAddon keeps this much room at the right for the scrollbar: as wide as the padding on the left.
    overviewRuler: { width: TERMINAL_PADDING_PX },
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  // Whether the process has accepted a size yet: the attach's own size is only forced when it has not.
  let sentToProcess = false;
  const sendSize = createResizeSender((size) =>
    commands.resizeTerminal(terminalId, size.cols, size.rows).then((result) => {
      sentToProcess ||= result.status === "ok";
      return result.status === "ok";
    }),
  );
  terminal.open(container);
  // Shown on the screen now: only a shown terminal follows the window, and the others fit when they are shown again.
  let shown = false;
  // The screen's cell is measured differently by each renderer: a renderer change refits, so the columns match it.
  const webgl = webglBudget.slotFor(terminal, terminalId, () => {
    if (!disposed && shown) fitIfVisible();
  });

  // A hidden container measures 0x0 and would collapse the terminal.
  // The size is always sent: the screen's own resize event only fires when its size changes, and a
  // process that kept an older size (a session that outlived the window) would stay at it.
  // The process holds its size while a node is held (dragged, or moved to its size): the screen still follows.
  let holding = false;
  // A card's size transition runs in the DOM: the screen follows it, and the process gets its size once it has landed.
  let landing = false;
  const cardMoving = () =>
    (container.closest("[data-owner]")?.getAnimations() ?? []).some(
      (animation) =>
        "transitionProperty" in animation &&
        (animation.transitionProperty === "width" ||
          animation.transitionProperty === "height"),
    );
  const syncHold = () => sendSize.hold(holding || landing);
  // Nothing is fitted or sent before the session's snapshot is applied: mounting sends no size.
  let sized = false;
  // The snapshot's own size is applied without telling the process: it already has that size.
  let applying = false;
  // While a node is dragged the process gets the size at most every LIVE_RESIZE_MS, and the last one when the drag ends.
  let live = false;
  let liveTimer: ReturnType<typeof setTimeout> | null = null;
  const sendScreenSize = () => {
    if (cardMoving()) landing = true;
    syncHold();
    if (!live) {
      sendSize({ cols: terminal.cols, rows: terminal.rows });
      return;
    }
    liveTimer ??= setTimeout(() => {
      liveTimer = null;
      sendSize({ cols: terminal.cols, rows: terminal.rows });
    }, LIVE_RESIZE_MS);
  };

  // `attaching` is the first fit of a session: the session gets the screen's size even when it is the size it has.
  const fitIfVisible = (attaching = false) => {
    if (!sized) return;
    if (container.clientWidth > 0 && container.clientHeight > 0) {
      if (attaching && !sentToProcess) sendSize.forget();
      fit.fit();
      sendScreenSize();
    }
  };

  let disposed = false;

  // Everything written to the terminal goes through this queue, in order: the replay, the live output, the exit line.
  const output = new TerminalWriteQueue((data, done) =>
    terminal.write(data, done),
  );
  const gate = new TerminalStreamGate(output, (exitCode) => {
    const status = exitCode === null ? "" : ` with code ${exitCode}`;
    output.push(`\r\n\x1b[2m[process exited${status}]\x1b[0m\r\n`);
    handlers.onExit(exitCode);
  });

  // Exit is an event, so it is subscribed before attaching: attaching is what returns the snapshot.
  const exit = terminalExitHub.subscribe(terminalId, (payload) =>
    gate.pushExit(payload.exitCode),
  );

  // The snapshot is asked for one frame at a time: sessions asking together would decode their replays in one frame.
  let cancelAttach = () => {};
  exit.ready
    .then(
      () =>
        new Promise<void>((resolve) => {
          cancelAttach = queueTerminalFrame(resolve);
        }),
    )
    .then(() => {
      return unwrap(
        attachTerminalOutput(terminalId, (chunk) => {
          // Output that comes after the view is gone has no screen to draw on.
          if (!disposed) gate.pushOutput(chunk);
        }),
      );
    })
    .then((snapshot) => {
      if (disposed) return;
      // The output was drawn at the session's size: the screen takes that size before replaying it, or the
      // lines wrap at the wrong column, even while the container is hidden.
      applying = true;
      terminal.resize(snapshot.cols, snapshot.rows);
      applying = false;
      // The screen's own size may differ from the session's; the session takes the screen's size once the replay is parsed.
      gate.attach(snapshot, () => {
        // The last piece of a replay can finish after the terminal was closed: a closed terminal sends no size.
        if (disposed) return;
        sized = true;
        fitIfVisible(true);
        mounted.replayed = true;
        handlers.onReplayed(mounted);
      });
    })
    .catch((error: unknown) => {
      if (disposed) return;
      const reason = error instanceof Error ? error.message : String(error);
      output.push(`\x1b[31m${reason}\x1b[0m\r\n`);
    });

  const input = terminal.onData((data) => {
    // A dead session rejects input; there is nothing useful to do about it.
    void commands.writeTerminal(terminalId, data);
    useTerminalStore.getState().woken(terminalId);
  });
  const resize = terminal.onResize(() => {
    if (!applying) sendScreenSize();
  });
  const observer = new ResizeObserver(() => {
    if (shown) fitIfVisible();
  });
  observer.observe(container);

  // The bell is only shown: the terminal engine reports it, it plays no sound.
  const bell = terminal.onBell(() => {
    if (getSettings().bell === "visual") {
      container.classList.add("terminal-bell");
    }
  });
  const endFlash = (event: AnimationEvent) => {
    if (event.animationName === "terminal-bell") {
      container.classList.remove("terminal-bell");
    }
  };
  container.addEventListener("animationend", endFlash);

  const copyOnRelease = () => {
    if (!getSettings().copyOnSelect || !terminal.hasSelection()) return;
    // The clipboard can refuse; the selection stays, so it can still be copied by hand.
    void navigator.clipboard
      .writeText(terminal.getSelection())
      .catch(() => undefined);
  };
  container.addEventListener("mouseup", copyOnRelease);

  const mounted: Mounted = {
    terminal,
    refit: fitIfVisible,
    holdResize: (next) => {
      // Only a change of the hold matters: a view placed with no hold on it does not refit here.
      if (next === holding) return;
      holding = next;
      syncHold();
      if (!next) fitIfVisible();
    },
    landed: () => {
      landing = false;
      syncHold();
      fitIfVisible();
    },
    setLiveResize: (next) => {
      if (next === live) return;
      live = next;
      if (next) return;
      if (liveTimer !== null) clearTimeout(liveTimer);
      liveTimer = null;
      fitIfVisible();
    },
    webgl,
    setShown: (next) => {
      shown = next;
    },
    replayed: false,
    dispose() {
      disposed = true;
      cancelAttach();
      output.clear();
      if (liveTimer !== null) clearTimeout(liveTimer);
      observer.disconnect();
      input.dispose();
      resize.dispose();
      bell.dispose();
      container.removeEventListener("animationend", endFlash);
      container.removeEventListener("mouseup", copyOnRelease);
      exit.unsubscribe();
      webgl.dispose();
      terminal.dispose();
    },
  };
  return mounted;
}

/** Shows one backend terminal session. The session outlives the view. */
// Its parents redraw on every worktree or team change, so it re-renders only for its own props.
export const TerminalView = memo(TerminalViewBase);

function TerminalViewBase({
  terminalId,
  active,
  focused = true,
  inView = true,
  resizeHeld = false,
  resizeLive = false,
  onExit,
  className,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mountedRef = useRef<Mounted | null>(null);
  const onExitRef = useRef(onExit);
  const activeRef = useRef(active);
  const focusedRef = useRef(focused);
  const settings = useTerminalConfig();
  const focusRequest = useTerminalStore((state) => state.focusRequest);
  const settingsRef = useRef(settings);
  const heldRef = useRef(resizeHeld);
  const liveRef = useRef(resizeLive);
  // The view as the holder of its session's element: a view that unmounts lets go of it under this name.
  const holderRef = useRef<object>({});
  // The GPU request still waiting for its frame, if any.
  const gpuPending = useRef<() => void>(() => {});
  const visibleRef = useRef(false);
  const shownRef = useRef(false);

  // The GPU renderer starts once the replay is parsed, in a frame of its own, and only while the terminal is shown.
  const requestGpu = useCallback((mounted: Mounted) => {
    // A context the terminal already holds comes back at once: only a new one waits its turn in the frame queue.
    if (mounted.webgl.attached) {
      if (mounted.replayed && activeRef.current) void mounted.webgl.show();
      return;
    }
    // Several requests in one commit are one: the latest replaces the one still waiting.
    gpuPending.current();
    gpuPending.current = queueTerminalFrame(() => {
      if (mounted.replayed && activeRef.current) void mounted.webgl.show();
    });
  }, []);

  useEffect(() => {
    onExitRef.current = onExit;
    activeRef.current = active;
    focusedRef.current = focused;
    settingsRef.current = settings;
  }, [onExit, active, focused, settings]);

  // A session shown again is already made: its element goes back before the paint, in the same frame as the change.
  // Only a new session waits for the fonts and its set-up, one per frame.
  useLayoutEffect(() => {
    const slot = containerRef.current;
    if (!slot) return;
    const holder = holderRef.current;
    let cancelled = false;
    // The work of this terminal, one piece per frame; the last one queued is the one cancelled with it.
    let cancelWork = () => {};
    const attach = () => {
      if (cancelled) return;
      const entry = terminalHosts.place(terminalId, holder, slot);
      entry.handlers.onExit = (code) => onExitRef.current?.(code);
      entry.handlers.onReplayed = (value) => requestGpu(value);
      if (entry.value === null) {
        entry.value = mountTerminal(entry.host, terminalId, entry.handlers);
      } else {
        // Shown again: the terminal kept its screen, and only its settings may have changed meanwhile.
        restyleTerminal(entry.value.terminal, currentTerminalConfig());
        if (activeRef.current) requestGpu(entry.value);
      }
      const mounted = entry.value;
      mountedRef.current = mounted;
      mounted.webgl.setVisible(visibleRef.current);
      mounted.setShown(shownRef.current);
      mounted.holdResize(heldRef.current);
      mounted.setLiveResize(liveRef.current);
      if (activeRef.current && focusedRef.current) mounted.terminal.focus();
    };
    if (terminalHosts.has(terminalId)) {
      attach();
    } else {
      void terminalFontsReady().then(() => {
        if (cancelled) return;
        cancelWork = queueTerminalFrame(attach);
      });
    }

    return () => {
      cancelled = true;
      cancelWork();
      gpuPending.current();
      // Only the element is let go: the terminal stays with its session, and keeps its GPU context until the budget needs it.
      if (terminalHosts.release(terminalId, holder)) {
        mountedRef.current?.webgl.setVisible(false);
        // The replay and the exit may still land after this view: they reach nothing.
        const handlers = terminalHosts.entryOf(terminalId).handlers;
        handlers.onReplayed = () => {};
        handlers.onExit = () => {};
      }
      mountedRef.current = null;
    };
  }, [terminalId, requestGpu]);

  // On a screen now: a visible terminal keeps its GPU context, and a hidden one gives it to the budget's next claim.
  useLayoutEffect(() => {
    visibleRef.current = active && inView;
    mountedRef.current?.webgl.setVisible(visibleRef.current);
    shownRef.current = active;
    mountedRef.current?.setShown(active);
  }, [active, inView]);

  // A terminal that comes back on the screen asks for its GPU renderer again: the budget then sees it as recent.
  useEffect(() => {
    if (!inView) return;
    const mounted = mountedRef.current;
    if (mounted) requestGpu(mounted);
  }, [inView, requestGpu]);

  // A card moving to its size: the PTY keeps its size until it lands, then the screen's size is sent once.
  useEffect(() => {
    heldRef.current = resizeHeld;
    const mounted = mountedRef.current;
    if (!mounted) return;
    mounted.holdResize(resizeHeld);
  }, [resizeHeld]);

  // A card's size transition ends (or is cut short): the terminal in it takes the size it has now.
  useEffect(() => {
    const owner = containerRef.current?.closest("[data-owner]");
    if (!owner) return;
    const landed = (event: Event) => {
      if (event.target === owner) mountedRef.current?.landed();
    };
    owner.addEventListener("transitionend", landed);
    owner.addEventListener("transitioncancel", landed);
    return () => {
      owner.removeEventListener("transitionend", landed);
      owner.removeEventListener("transitioncancel", landed);
    };
  }, []);

  useEffect(() => {
    liveRef.current = resizeLive;
    mountedRef.current?.setLiveResize(resizeLive);
  }, [resizeLive]);

  // Settings changed while terminals are open: restyle them in place.
  useEffect(() => {
    const mounted = mountedRef.current;
    if (!mounted) return;
    const { terminal, refit } = mounted;
    restyleTerminal(terminal, settings);
    refit();
    // Re-applies the whole config: every field here is cheap to set again.
  }, [settings, terminalId]);

  // Something outside (an agent row in the sidebar) wants this terminal focused.
  useEffect(() => {
    if (!active || focusRequest?.id !== terminalId) return;
    const frame = requestAnimationFrame(() =>
      mountedRef.current?.terminal.focus(),
    );
    return () => cancelAnimationFrame(frame);
  }, [focusRequest, active, terminalId]);

  // Before the paint: the tab is shown in the same commit, so the first frame already has the final size.
  useLayoutEffect(() => {
    if (!active) return;
    mountedRef.current?.refit();
    if (focusedRef.current) mountedRef.current?.terminal.focus();
    // The DOM view is usable at once; the GPU renderer follows in a frame of its own, with the other terminals' work.
    const mounted = mountedRef.current;
    if (mounted) requestGpu(mounted);
    return undefined;
  }, [active, requestGpu, terminalId]);

  return (
    // The padding lives out here: xterm sizes itself from its parent's box and
    // would count padding on that parent as room it does not have. Its colour is
    // the terminal's, so a light scheme does not sit inside a dark frame.
    <div
      className={cn("size-full py-2 pl-2", !active && "invisible", className)}
      style={{ backgroundColor: resolveTerminalTheme(settings).background }}
    >
      {/* WebGL draws the text on a canvas, so the applied font is mirrored here for the page to read. */}
      <div
        ref={containerRef}
        className="size-full"
        data-terminal-font-family={terminalFontFamily(settings.fontFamily)}
        data-terminal-font-size={settings.fontSize}
      />
    </div>
  );
}

// A session that is gone from the list is gone for good: its terminal is disposed here, not by the view that showed it.
useTerminalStore.subscribe((state) => {
  terminalHosts.prune(new Set(state.sessions.map((session) => session.id)));
});
