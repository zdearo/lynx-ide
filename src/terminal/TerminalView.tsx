import { useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { commands, events, type TerminalConfig } from "@/bindings";
import { useTerminalConfig } from "@/config/config-store";
import { cn } from "@/lib/utils";
import { unwrap } from "@/lib/ipc-result";
import {
  terminalFontFamily,
  terminalFontsReady,
} from "@/terminal/terminal-appearance";
import { resolveTerminalTheme } from "@/terminal/terminal-color-schemes";
import { loadWebglRenderer } from "@/terminal/terminal-renderer";
import { useTerminalStore } from "@/terminal/terminal-store";
import { TerminalStreamGate } from "@/terminal/terminal-stream-gate";

interface TerminalViewProps {
  terminalId: string;
  /** Only the visible terminal is fitted and focused. */
  active: boolean;
  onExit?: (exitCode: number | null) => void;
  className?: string;
}

interface Mounted {
  terminal: Terminal;
  fit: FitAddon;
  dispose: () => void;
}

/** Opens xterm in `container` and wires it to one backend session. */
function mountTerminal(
  container: HTMLElement,
  terminalId: string,
  onExit: (exitCode: number | null) => void,
  getSettings: () => TerminalConfig,
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
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(container);
  loadWebglRenderer(terminal);

  // A hidden container measures 0x0 and would collapse the terminal.
  const fitIfVisible = () => {
    if (container.clientWidth > 0 && container.clientHeight > 0) fit.fit();
  };

  let disposed = false;
  const unlisteners: UnlistenFn[] = [];
  const keepListener = (pending: Promise<UnlistenFn>) => {
    void pending.then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });
  };

  const gate = new TerminalStreamGate(
    (data) => terminal.write(data),
    (exitCode) => {
      const status = exitCode === null ? "" : ` with code ${exitCode}`;
      terminal.write(`\r\n\x1b[2m[process exited${status}]\x1b[0m\r\n`);
      onExit(exitCode);
    },
  );

  // Listen before asking for the snapshot so no output falls in the gap.
  keepListener(
    events.terminalOutput.listen(({ payload }) => {
      if (payload.id === terminalId) gate.pushOutput(payload);
    }),
  );
  keepListener(
    events.terminalExit.listen(({ payload }) => {
      if (payload.id === terminalId) gate.pushExit(payload.exitCode);
    }),
  );

  unwrap(commands.terminalSnapshot(terminalId))
    .then((snapshot) => {
      if (disposed) return;
      gate.attach(snapshot);
      fitIfVisible();
      // The session was created at a default size; tell it the real one.
      void commands.resizeTerminal(terminalId, terminal.cols, terminal.rows);
    })
    .catch((error: unknown) => {
      if (disposed) return;
      const reason = error instanceof Error ? error.message : String(error);
      terminal.write(`\x1b[31m${reason}\x1b[0m\r\n`);
    });

  const input = terminal.onData((data) => {
    // A dead session rejects input; there is nothing useful to do about it.
    void commands.writeTerminal(terminalId, data);
  });
  const resize = terminal.onResize(({ cols, rows }) => {
    void commands.resizeTerminal(terminalId, cols, rows);
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

  return {
    terminal,
    fit,
    dispose() {
      disposed = true;
      observer.disconnect();
      input.dispose();
      resize.dispose();
      bell.dispose();
      container.removeEventListener("animationend", endFlash);
      container.removeEventListener("mouseup", copyOnRelease);
      unlisteners.forEach((unlisten) => unlisten());
      terminal.dispose();
    },
  };
}

/** Shows one backend terminal session. The session outlives the view. */
export function TerminalView({
  terminalId,
  active,
  onExit,
  className,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mountedRef = useRef<Mounted | null>(null);
  const onExitRef = useRef(onExit);
  const activeRef = useRef(active);
  const settings = useTerminalConfig();
  const focusRequest = useTerminalStore((state) => state.focusRequest);
  const settingsRef = useRef(settings);

  useEffect(() => {
    onExitRef.current = onExit;
    activeRef.current = active;
    settingsRef.current = settings;
  }, [onExit, active, settings]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let cancelled = false;
    void terminalFontsReady().then(() => {
      if (cancelled) return;
      const mounted = mountTerminal(
        container,
        terminalId,
        (code) => onExitRef.current?.(code),
        () => settingsRef.current,
      );
      mountedRef.current = mounted;
      // The tab may have become active while the fonts were loading.
      if (activeRef.current) mounted.terminal.focus();
    });

    return () => {
      cancelled = true;
      mountedRef.current?.dispose();
      mountedRef.current = null;
    };
  }, [terminalId]);

  // Settings changed while terminals are open: restyle them in place.
  useEffect(() => {
    const mounted = mountedRef.current;
    if (!mounted) return;
    const { terminal, fit } = mounted;
    terminal.options.fontFamily = terminalFontFamily(settings.fontFamily);
    terminal.options.fontSize = settings.fontSize;
    terminal.options.scrollback = settings.scrollback;
    terminal.options.macOptionIsMeta = settings.macOptionIsMeta;
    terminal.options.theme = resolveTerminalTheme(settings);
    if (containerRef.current && containerRef.current.clientWidth > 0) fit.fit();
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
    // Wait a frame so the container is laid out after becoming visible.
    const frame = requestAnimationFrame(() => {
      mountedRef.current?.fit.fit();
      mountedRef.current?.terminal.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [active]);

  return (
    // The padding lives out here: xterm sizes itself from its parent's box and
    // would count padding on that parent as room it does not have. Its colour is
    // the terminal's, so a light scheme does not sit inside a dark frame.
    <div
      className={cn("size-full p-2", !active && "hidden", className)}
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
