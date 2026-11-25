import { memo, useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { commands, type TerminalConfig } from "@/bindings";
import { useTerminalConfig } from "@/config/config-store";
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
import { queueTerminalFrame } from "@/terminal/terminal-frame-queue";

interface TerminalViewProps {
  terminalId: string;
  /** Only the visible terminal is fitted and focused. */
  active: boolean;
  /** Whether this terminal takes the keyboard when it is shown: of several open at once, only the picked one does. */
  focused?: boolean;
  /** The screen is fitted, but the process keeps its size until this is released (a card moving to its size). */
  resizeHeld?: boolean;
  onExit?: (exitCode: number | null) => void;
  className?: string;
}

interface Mounted {
  terminal: Terminal;
  /** Fits the screen to its box and tells the process the size, even when it did not change. */
  refit: () => void;
  /** Keeps the process's size while held; releasing sends the screen's latest size. */
  holdResize: (held: boolean) => void;
  /** WebGL is attached only while this terminal is the visible one. */
  webgl: WebglRendererSlot;
  /** Whether the snapshot is applied; the GPU renderer waits for it, so the replay is drawn by the DOM view. */
  replayed: boolean;
  dispose: () => void;
}

/** Opens xterm in `container` and wires it to one backend session. */
/** The room around the text: the same at the left, and at the right where the scrollbar sits. */
const TERMINAL_PADDING_PX = 8;

function mountTerminal(
  container: HTMLElement,
  terminalId: string,
  onExit: (exitCode: number | null) => void,
  getSettings: () => TerminalConfig,
  onReplayed: (mounted: Mounted) => void,
): Mounted {
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
  const sendSize = createResizeSender((size) =>
    commands.resizeTerminal(terminalId, size.cols, size.rows),
  );
  terminal.open(container);
  const webgl = webglBudget.slotFor(terminal);

  // A hidden container measures 0x0 and would collapse the terminal.
  // The size is always sent: the screen's own resize event only fires when its size changes, and a
  // process that kept an older size (a session that outlived the window) would stay at it.
  // While the process holds its size (a card moving), the terminal is not refitted on each frame: it is fitted once, on release.
  let holding = false;
  const fitIfVisible = () => {
    if (holding) return;
    if (container.clientWidth > 0 && container.clientHeight > 0) {
      fit.fit();
      sendSize({ cols: terminal.cols, rows: terminal.rows });
    }
  };

  let disposed = false;

  const gate = new TerminalStreamGate(
    (data, done) => terminal.write(data, done),
    (exitCode) => {
      const status = exitCode === null ? "" : ` with code ${exitCode}`;
      terminal.write(`\r\n\x1b[2m[process exited${status}]\x1b[0m\r\n`);
      onExit(exitCode);
    },
    queueTerminalFrame,
  );

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
    .then(() =>
      unwrap(
        attachTerminalOutput(terminalId, (chunk) => gate.pushOutput(chunk)),
      ),
    )
    .then((snapshot) => {
      if (disposed) return;
      // The output was drawn at the session's size: the screen takes that size before replaying it, or the
      // lines wrap at the wrong column, even while the container is hidden.
      terminal.resize(snapshot.cols, snapshot.rows);
      // The screen's own size may differ from the session's; the session takes the screen's size once the replay is parsed.
      gate.attach(snapshot, () => {
        fitIfVisible();
        mounted.replayed = true;
        onReplayed(mounted);
      });
    })
    .catch((error: unknown) => {
      if (disposed) return;
      const reason = error instanceof Error ? error.message : String(error);
      terminal.write(`\x1b[31m${reason}\x1b[0m\r\n`);
    });

  const input = terminal.onData((data) => {
    // A dead session rejects input; there is nothing useful to do about it.
    void commands.writeTerminal(terminalId, data);
    useTerminalStore.getState().woken(terminalId);
  });
  const resize = terminal.onResize(({ cols, rows }) => {
    sendSize({ cols, rows });
  });
  const observer = new ResizeObserver(fitIfVisible);
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
      holding = next;
      sendSize.hold(next);
      if (!next) fitIfVisible();
    },
    webgl,
    replayed: false,
    dispose() {
      disposed = true;
      cancelAttach();
      gate.dispose();
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
  resizeHeld = false,
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

  // The GPU renderer starts once the replay is parsed, in a frame of its own, and only while the terminal is shown.
  const requestGpu = (mounted: Mounted) =>
    queueTerminalFrame(() => {
      if (mounted.replayed && activeRef.current) void mounted.webgl.show();
    });

  useEffect(() => {
    onExitRef.current = onExit;
    activeRef.current = active;
    focusedRef.current = focused;
    settingsRef.current = settings;
  }, [onExit, active, focused, settings]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let cancelled = false;
    // The work of this terminal, one piece per frame; the last one queued is the one cancelled with it.
    let cancelWork = () => {};
    void terminalFontsReady().then(() => {
      if (cancelled) return;
      cancelWork = queueTerminalFrame(() => {
        if (cancelled) return;
        const mounted = mountTerminal(
          container,
          terminalId,
          (code) => onExitRef.current?.(code),
          () => settingsRef.current,
          requestGpu,
        );
        mountedRef.current = mounted;
        mounted.holdResize(heldRef.current);
        if (activeRef.current && focusedRef.current) mounted.terminal.focus();
      });
    });

    return () => {
      cancelled = true;
      cancelWork();
      mountedRef.current?.dispose();
      mountedRef.current = null;
    };
  }, [terminalId]);

  // A card moving to its size: the PTY keeps its size until it lands, then the screen's size is sent once.
  useEffect(() => {
    heldRef.current = resizeHeld;
    const mounted = mountedRef.current;
    if (!mounted) return;
    mounted.holdResize(resizeHeld);
    if (!resizeHeld) mounted.refit();
  }, [resizeHeld]);

  // Settings changed while terminals are open: restyle them in place.
  useEffect(() => {
    const mounted = mountedRef.current;
    if (!mounted) return;
    const { terminal, refit } = mounted;
    terminal.options.fontFamily = terminalFontFamily(settings.fontFamily);
    terminal.options.fontSize = settings.fontSize;
    terminal.options.scrollback = settings.scrollback;
    terminal.options.macOptionIsMeta = settings.macOptionIsMeta;
    terminal.options.theme = resolveTerminalTheme(settings);
    refit();
    // Re-applies the whole config: every field here is cheap to set again.
  }, [settings]);

  // Something outside (an agent row in the sidebar) wants this terminal focused.
  useEffect(() => {
    if (!active || focusRequest?.id !== terminalId) return;
    const frame = requestAnimationFrame(() =>
      mountedRef.current?.terminal.focus(),
    );
    return () => cancelAnimationFrame(frame);
  }, [focusRequest, active, terminalId]);

  useEffect(() => {
    if (!active) return;
    let cancelShow = () => {};
    // Wait a frame so the container is laid out after becoming visible.
    const frame = requestAnimationFrame(() => {
      mountedRef.current?.refit();
      if (focusedRef.current) mountedRef.current?.terminal.focus();
      // The DOM view is usable at once; the GPU renderer follows in a frame of its own, with the other terminals' work.
      const mounted = mountedRef.current;
      if (mounted) cancelShow = requestGpu(mounted);
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelShow();
    };
  }, [active]);

  return (
    // The padding lives out here: xterm sizes itself from its parent's box and
    // would count padding on that parent as room it does not have. Its colour is
    // the terminal's, so a light scheme does not sit inside a dark frame.
    <div
      className={cn("size-full py-2 pl-2", !active && "hidden", className)}
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
