import { useEffect, useState } from "react";
import { toast } from "sonner";

import { useAgentCatalogStore } from "@/agents/agent-catalog-store";
import { commands, type AgentEntry, type HarnessId } from "@/bindings";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Switch } from "@/components/ui/switch";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** A switch the user flipped, waiting for a yes before the agent's config is touched. */
interface Pending {
  entry: AgentEntry;
  turnOn: boolean;
}

/** Reloads the catalog; the settings and the new-terminal menu both read it from the store. */
function refreshCatalog(): Promise<void> {
  return useAgentCatalogStore
    .getState()
    .load()
    .catch((error: unknown) => {
      toast.error(copy.settings.agents.failed, {
        description: errorMessage(error),
      });
    });
}

/** The agents found on this computer get a switch; the rest are listed for information only. */
export function AgentsSettings() {
  const entries = useAgentCatalogStore((state) => state.entries);
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  const text = copy.settings.agents;

  useEffect(() => {
    void refreshCatalog();
  }, []);

  const onThisComputer = (entries ?? []).filter(
    (entry) => entry.found || entry.hooks.state !== "notInstalled",
  );
  const elsewhere = (entries ?? []).filter(
    (entry) => !(entry.found || entry.hooks.state !== "notInstalled"),
  );

  async function confirm() {
    if (!pending) return;
    setBusy(true);
    try {
      const { entry, turnOn } = pending;
      if (turnOn) {
        await unwrap(commands.installHooks(entry.harness));
      } else {
        await unwrap(commands.removeHooks(entry.harness));
      }
      setPending(null);
      await refreshCatalog();
    } catch (error) {
      toast.error(text.failed, { description: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-start justify-between gap-3">
        <p className="text-sm text-muted-foreground">{text.intro}</p>
        <TooltipIconButton
          label={text.refresh}
          className="size-7 shrink-0"
          onClick={() => void refreshCatalog()}
        >
          <RefreshGlyph18 />
        </TooltipIconButton>
      </div>

      <Section id="agents-detected" title={text.detected}>
        {onThisComputer.map((entry) => (
          <li key={entry.harness} className="flex items-center gap-3 py-2.5">
            <AgentIcon harness={entry.harness} />
            <span className="min-w-0 flex-1 truncate text-sm">
              {text.harnesses[entry.harness].name}
            </span>
            {entry.hooks.state === "partial" && (
              <span className="shrink-0 text-meta text-muted-foreground">
                {text.hooks.states.partial}
              </span>
            )}
            {entry.hooks.state === "error" && (
              <span className="shrink-0 text-meta text-destructive">
                {text.hooks.states.error}
              </span>
            )}
            <Switch
              aria-label={text.use(text.harnesses[entry.harness].name)}
              checked={entry.hooks.state === "installed"}
              disabled={busy || entry.hooks.state === "error"}
              onCheckedChange={(turnOn) => setPending({ entry, turnOn })}
            />
          </li>
        ))}
      </Section>

      {elsewhere.length > 0 && (
        <Section id="agents-elsewhere" title={text.undetected}>
          {elsewhere.map((entry) => (
            <li
              key={entry.harness}
              className="flex items-center gap-3 py-2.5 text-muted-foreground"
            >
              <AgentIcon harness={entry.harness} muted />
              <span className="min-w-0 flex-1 truncate text-sm">
                {text.harnesses[entry.harness].name}
              </span>
              <span className="shrink-0 text-meta">{text.notFound}</span>
            </li>
          ))}
        </Section>
      )}

      <ConfirmChange
        pending={pending}
        busy={busy}
        onCancel={() => setPending(null)}
        onConfirm={() => void confirm()}
      />
    </div>
  );
}

function Section({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-1">
      <h3 id={id} className="text-sm font-medium">
        {title}
      </h3>
      <ul className="flex flex-col divide-y divide-border/50">{children}</ul>
    </section>
  );
}

function AgentIcon({
  harness,
  muted = false,
}: {
  harness: HarnessId;
  muted?: boolean;
}) {
  return (
    <span
      className={
        muted
          ? "flex size-7 shrink-0 items-center justify-center rounded-md border border-border/50 bg-background/50 opacity-60"
          : "flex size-7 shrink-0 items-center justify-center rounded-md border border-border/50 bg-background/50"
      }
    >
      <AgentBrandIcon harness={harness} className="size-4 text-agent" />
    </span>
  );
}

/** The wording depends on the direction: turning on, updating a partial install, or turning off. */
function ConfirmChange({
  pending,
  busy,
  onCancel,
  onConfirm,
}: {
  pending: Pending | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const text = copy.settings.agents.hooks;
  const name = pending
    ? copy.settings.agents.harnesses[pending.entry.harness].name
    : "";
  const path = pending?.entry.hooks.configPath ?? "";
  const partial = pending?.entry.hooks.state === "partial";

  let title = text.connectTitle(name);
  let body = text.connectBody(path);
  let action: string = text.confirmConnect;
  if (pending && !pending.turnOn) {
    title = text.disconnectTitle(name);
    body = text.disconnectBody(path);
    action = text.confirmDisconnect;
  } else if (partial) {
    title = text.updateTitle(name);
    body = text.updateBody(path);
    action = text.confirmUpdate;
  }

  return (
    <AlertDialog
      open={pending !== null}
      onOpenChange={(open) => {
        if (!open && !busy) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            {body}
            {pending?.turnOn && (
              <>
                {" "}
                {text.eventsLabel}: {pending.entry.hooks.events.join(", ")}.
              </>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>{text.cancel}</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy}
            onClick={(event) => {
              // Stay open until the change lands, so a failure can be read here.
              event.preventDefault();
              onConfirm();
            }}
          >
            {action}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
