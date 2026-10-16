import { useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { commands, events } from "@/bindings";
import { cn } from "@/lib/utils";
import { unwrap } from "@/lib/ipc-result";
import { TerminalStreamGate } from "@/terminal/terminal-stream-gate";

// xterm paints on a canvas and cannot read the oklch CSS tokens, so these mirror
// the dark theme's --background and --foreground.
const TERMINAL_THEME = {
  background: "#0a0a0a",
  foreground: "#fafafa",
  scrollbarSliderBackground: "rgba(255, 255, 255, 0.10)",
  scrollbarSliderHoverBackground: "rgba(255, 255, 255, 0.18)",
  scrollbarSliderActiveBackground: "rgba(255, 255, 255, 0.26)",
};

interface TerminalViewProps {
  terminalId: string;
  /** Only the visible terminal is fitted and focused. */
  active: boolean;
  onExit?: (exitCode: number | null) => void;
  className?: string;
}

/** Shows one backend terminal session. The session outlives the view. */
export function TerminalView({
  terminalId,
  active,
  onExit,
  className,
}: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const onExitRef = useRef(onExit);

  useEffect(() => {
    onExitRef.current = onExit;
  }, [onExit]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: 'ui-monospace, "JetBrains Mono", Menlo, Consolas, monospace',
      fontSize: 13,
      scrollback: 5000,
      theme: TERMINAL_THEME,
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(container);
    terminalRef.current = terminal;
    fitRef.current = fit;

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
        onExitRef.current?.(exitCode);
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

    return () => {
      disposed = true;
      observer.disconnect();
      input.dispose();
      resize.dispose();
      unlisteners.forEach((unlisten) => unlisten());
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
    };
  }, [terminalId]);

  useEffect(() => {
    if (!active) return;
    // Wait a frame so the container is laid out after becoming visible.
    const frame = requestAnimationFrame(() => {
      fitRef.current?.fit();
      terminalRef.current?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [active]);

  return (
    <div
      ref={containerRef}
      className={cn(
        "size-full bg-background p-2",
        !active && "hidden",
        className,
      )}
    />
  );
}
