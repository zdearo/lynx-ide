import { useEffect, useRef, useState } from "react";

import { commands } from "@/bindings";
import { ChevronRightGlyph18 } from "@/components/icons/ChevronRightGlyph18";
import { MinimizeWindowGlyph18 } from "@/components/icons/MinimizeWindowGlyph18";
import { OpenInNewWindowGlyph18 } from "@/components/icons/OpenInNewWindowGlyph18";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { EngineProgressBar } from "@/browser/EngineProgressBar";
import { useBrowserStore } from "@/browser/browser-store";
import { isBrowserWindow } from "@/browser/browser-window";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/use-selected-worktree";

// Menus, dialogs and lists are drawn by the web view, and the page is a native view above it:
// while one is open the page steps aside, or it would cover it.
const OVERLAYS =
  '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]';

/** The browser: a toolbar, and the area the engine's page is shown in by a native view. */
export function BrowserView() {
  const state = useBrowserStore((s) => s.state);
  const starting = useBrowserStore((s) => s.starting);
  const failure = useBrowserStore((s) => s.failure);
  const slot = useRef<HTMLDivElement>(null);
  const running = state.status === "running";
  // A tab in its own window is drawn there, never here: this view says so instead.
  const detachedHere = !isBrowserWindow() && state.detached;
  // The engine runs but no tab is shown: the page area offers to open one instead.
  const showing = running && state.page !== null && !detachedHere;
  const page = state.page;

  // Where the page goes follows this box. Polling every frame is deliberate: a resized window or
  // a moved panel shifts the box without resizing it, and an observer would miss that.
  useEffect(() => {
    if (!showing || page === null) return;
    let frame = 0;
    let last = "";
    const tick = () => {
      const box = slot.current?.getBoundingClientRect();
      const covered = document.querySelector(OVERLAYS) !== null;
      const key =
        box && !covered
          ? `${box.left}|${box.top}|${box.width}|${box.height}`
          : "hidden";
      if (key !== last) {
        last = key;
        if (box && !covered) {
          void commands.browserPlace(
            page,
            box.left,
            box.top,
            box.width,
            box.height,
          );
        } else {
          void commands.browserHide();
        }
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      void commands.browserHide();
    };
  }, [showing, page]);

  return (
    <div className="absolute inset-0 flex flex-col bg-background">
      <Toolbar />
      <div ref={slot} className="relative min-h-0 flex-1">
        {!running && (
          <Notice starting={starting || state.status === "starting"} />
        )}
        {running && detachedHere && state.page !== null && <DetachedNotice />}
        {running && state.page === null && <NoTab />}
        {running && failure && (
          <p
            role="alert"
            className="absolute inset-x-0 top-0 bg-destructive/10 px-3 py-1.5 text-ui text-destructive"
          >
            {failure.message}
          </p>
        )}
      </div>
    </div>
  );
}

function Toolbar() {
  const state = useBrowserStore((s) => s.state);
  const navigate = useBrowserStore((s) => s.navigate);
  const running = state.status === "running";
  const inOwnWindow = isBrowserWindow();
  const detachedHere = !inOwnWindow && state.detached;
  // Back, forward, reload and the address act on the shown page; the engine alone is not enough,
  // and a tab that is in its own window is driven from there.
  const hasPage = running && state.page !== null && !detachedHere;
  const [typed, setTyped] = useState<string | null>(null);

  const detachPage = useBrowserStore((s) => s.detachPage);
  const attachPage = useBrowserStore((s) => s.attachPage);
  const assignAgentPage = useBrowserStore((s) => s.assignAgentPage);
  const isAgentPage = useBrowserStore(
    (s) =>
      s.pages.find((entry) => entry.id === s.state.page)?.agentPage === true,
  );
  const agentAction = useBrowserStore((s) => s.agentAction);
  const reload = () => void commands.browserReload(false);
  const stop = () => void commands.browserStop();

  return (
    <div
      role="toolbar"
      aria-label={copy.browser.label}
      className="flex h-9 shrink-0 items-center gap-1 border-b border-border px-1.5"
    >
      <TooltipIconButton
        label={copy.browser.back}
        disabled={!hasPage || !state.canGoBack}
        onClick={() => void commands.browserBack()}
      >
        <ChevronRightGlyph18 className="rotate-180" />
      </TooltipIconButton>
      <TooltipIconButton
        label={copy.browser.forward}
        disabled={!hasPage || !state.canGoForward}
        onClick={() => void commands.browserForward()}
      >
        <ChevronRightGlyph18 />
      </TooltipIconButton>
      <TooltipIconButton
        label={state.loading ? copy.browser.stop : copy.browser.reload}
        disabled={!hasPage}
        onClick={state.loading ? stop : reload}
      >
        {state.loading ? <XmarkGlyph18 /> : <RefreshGlyph18 />}
      </TooltipIconButton>
      <form
        className="min-w-0 flex-1"
        onSubmit={(event) => {
          event.preventDefault();
          if (typed !== null && typed.trim() !== "") void navigate(typed);
          setTyped(null);
        }}
      >
        <Input
          aria-label={copy.browser.address}
          placeholder={copy.browser.addressPlaceholder}
          disabled={!hasPage}
          className="h-7 text-ui"
          // What was typed wins while the field is being edited; otherwise it follows the page.
          value={typed ?? state.url}
          onChange={(event) => setTyped(event.target.value)}
          onBlur={() => setTyped(null)}
          onFocus={(event) => event.target.select()}
          spellCheck={false}
        />
      </form>
      <TooltipIconButton
        label={
          isAgentPage ? copy.browser.agentAssigned : copy.browser.assignAgent
        }
        aria-pressed={isAgentPage}
        disabled={!hasPage}
        className={cn(isAgentPage && "text-agent")}
        onClick={() => {
          if (state.page !== null) void assignAgentPage(state.page);
        }}
      >
        <SparkleGlyph18 />
      </TooltipIconButton>
      {agentAction && !detachedHere && (
        <span
          role="status"
          className="flex max-w-72 shrink-0 items-center gap-1.5 rounded-md bg-agent/15 px-2 py-0.5 text-meta text-foreground"
          title={copy.browser.inControl(agentAction.agent)}
        >
          <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-live" />
          <span className="truncate">
            {copy.browser.inControl(agentAction.agent)} · {agentAction.summary}
          </span>
        </span>
      )}
      {inOwnWindow ? (
        <TooltipIconButton
          label={copy.browser.attach}
          disabled={!hasPage}
          onClick={() => {
            if (state.page !== null) void attachPage(state.page);
          }}
        >
          <MinimizeWindowGlyph18 />
        </TooltipIconButton>
      ) : (
        <TooltipIconButton
          label={copy.browser.detach}
          disabled={!hasPage}
          onClick={() => {
            if (state.page !== null) void detachPage(state.page);
          }}
        >
          <OpenInNewWindowGlyph18 />
        </TooltipIconButton>
      )}
    </div>
  );
}

function Notice({ starting }: { starting: boolean }) {
  const failure = useBrowserStore((s) => s.failure);
  const stateError = useBrowserStore((s) => s.state.error);
  const start = useBrowserStore((s) => s.start);

  if (starting) {
    return (
      <Centered>
        <p className="text-ui text-muted-foreground">{copy.browser.starting}</p>
      </Centered>
    );
  }
  if (
    failure?.kind === "browserEngineMissing" ||
    failure?.kind === "browserEngineOutdated"
  ) {
    return <EngineNotice outdated={failure.kind === "browserEngineOutdated"} />;
  }
  return (
    <Centered>
      <h2 className="text-sm font-medium">{copy.browser.failedTitle}</h2>
      <p className="mt-1 text-ui text-muted-foreground">
        {failure?.message ?? stateError ?? copy.browser.stopped}
      </p>
      <div className="mt-3">
        <Button variant="outline" size="sm" onClick={() => void start()}>
          {copy.browser.retry}
        </Button>
      </div>
    </Centered>
  );
}

/** The selected tab is in a window of its own. It comes back here when that window closes. */
function DetachedNotice() {
  return (
    <Centered>
      <h2 className="text-sm font-medium">{copy.browser.detachedTitle}</h2>
      <p className="mt-1 text-ui text-muted-foreground">
        {copy.browser.detachedBody}
      </p>
    </Centered>
  );
}

/** The engine runs and no tab is shown: a new tab opens here, in the worktree the person selected. */
function NoTab() {
  const selected = useSelectedWorktree();
  const newPage = useBrowserStore((s) => s.newPage);
  const [address, setAddress] = useState("");
  const worktreePath = selected?.worktree.path ?? null;

  return (
    <Centered>
      <h2 className="text-sm font-medium">{copy.browser.noTab.title}</h2>
      <p className="mt-1 text-ui text-muted-foreground">
        {copy.browser.noTab.body}
      </p>
      <form
        className="mt-3 flex items-center gap-2 text-left"
        onSubmit={(event) => {
          event.preventDefault();
          if (worktreePath === null) return;
          const url = address.trim();
          void newPage(worktreePath, url === "" ? null : url);
        }}
      >
        <Input
          aria-label={copy.browser.noTab.address}
          placeholder={copy.browser.noTab.address}
          className="h-7 flex-1 text-ui"
          value={address}
          onChange={(event) => setAddress(event.target.value)}
          spellCheck={false}
        />
        <Button type="submit" size="sm" disabled={worktreePath === null}>
          {copy.browser.panel.newTab}
        </Button>
      </form>
    </Centered>
  );
}

/** The engine is not there, or is the wrong one: offer to download it, and show how it goes. */
function EngineNotice({ outdated }: { outdated: boolean }) {
  const busy = useBrowserStore((s) => s.engineBusy);
  const error = useBrowserStore((s) => s.engineError);
  const install = useBrowserStore((s) => s.installEngine);
  const text = copy.browser.engine;

  return (
    <Centered>
      <h2 className="text-sm font-medium">
        {outdated ? text.outdatedTitle : text.missingTitle}
      </h2>
      <p className="mt-1 text-ui text-muted-foreground">
        {outdated ? text.outdatedBody : text.missingBody}
      </p>
      {busy ? (
        <div className="mt-3 text-left">
          <EngineProgressBar />
        </div>
      ) : (
        <div className="mt-3">
          <Button size="sm" onClick={() => void install(outdated)}>
            {error ? text.retry : outdated ? text.update : text.install}
          </Button>
        </div>
      )}
      {error && (
        <p role="alert" className="mt-2 text-ui text-destructive">
          {error}
        </p>
      )}
      <p className="mt-4 text-meta text-muted-foreground">
        {text.developing}{" "}
        <code className="rounded-md bg-muted px-1.5 py-0.5">
          {text.developingCommand}
        </code>
      </p>
    </Centered>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center px-6 text-center">
      <div className="max-w-sm">{children}</div>
    </div>
  );
}
