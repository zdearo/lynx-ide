import { useEffect, useState } from "react";
import { toast } from "sonner";

import { useAgentCatalogStore } from "@/agents/agent-catalog-store";
import {
  commands,
  type AgentEntry,
  type AppConfig,
  type HarnessId,
} from "@/bindings";
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
import { GaugeGlyph18 } from "@/components/icons/GaugeGlyph18";
import { SparkleGlyph18 } from "@/components/icons/SparkleGlyph18";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { UsageSettings } from "@/settings/UsageSettings";
import { Switch } from "@/components/ui/switch";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { AgentBrandIcon } from "@/components/icons/brands/AgentBrandIcon";
import { RefreshGlyph18 } from "@/components/icons/RefreshGlyph18";
import {
  ChoiceSetting,
  SwitchSetting,
  TextSetting,
} from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import {
  withLaunchArgument,
  withLaunchMode,
  yoloFlag,
} from "@/terminal/agent-presets";
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
  const { config, disabled, save } = useSettings();
  const [pending, setPending] = useState<Pending | null>(null);
  const [yoloPending, setYoloPending] = useState(false);
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

  async function confirmYolo() {
    if (!config) return;
    setBusy(true);
    try {
      await save(withLaunchMode(config, "yolo"));
      setYoloPending(false);
    } catch (error) {
      toast.error(text.failed, { description: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Tabs defaultValue="setup" className="gap-6">
      <TabsList variant="line" aria-label={text.tabs.label}>
        <TabsTrigger value="setup">
          <SparkleGlyph18 aria-hidden className="size-3.5" />
          {text.tabs.setup}
        </TabsTrigger>
        <TabsTrigger value="limits">
          <GaugeGlyph18 aria-hidden className="size-3.5" />
          {copy.settings.usage.title}
        </TabsTrigger>
      </TabsList>

      <TabsContent value="setup" className="flex flex-col gap-6">
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

        {config && (
          <ChoiceSetting
            id="agents-launch-mode"
            label={text.launchMode.label}
            hint={text.launchMode.hint}
            value={config.agents.launchMode}
            defaultValue="manual"
            options={[
              { value: "manual", label: text.launchMode.manual },
              { value: "yolo", label: text.launchMode.yolo },
            ]}
            disabled={disabled}
            onCommit={async (mode) => {
              // YOLO skips every permission prompt, so it waits for a yes; the select keeps its value.
              if (mode === "yolo") setYoloPending(true);
              else await save(withLaunchMode(config, "manual"));
            }}
          />
        )}

        <Section id="agents-detected" title={text.detected}>
          {onThisComputer.map((entry) => (
            <li key={entry.harness} className="grid gap-3 py-2.5">
              <div className="flex items-center gap-3">
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
              </div>
              {config && entry.hooks.state === "installed" && (
                <ArgumentsField
                  harness={entry.harness}
                  config={config}
                  disabled={disabled}
                  save={save}
                />
              )}
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
      </TabsContent>

      <TabsContent value="limits" className="grid gap-5">
        <UsageSettings />
      </TabsContent>

      <ConfirmChange
        pending={pending}
        busy={busy}
        onCancel={() => setPending(null)}
        onConfirm={() => void confirm()}
      />

      <AlertDialog
        open={yoloPending}
        onOpenChange={(open) => {
          if (!open && !busy) setYoloPending(false);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{text.launchMode.confirmTitle}</AlertDialogTitle>
            <AlertDialogDescription>
              {text.launchMode.confirmBody}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>
              {copy.settings.agents.hooks.cancel}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={(event) => {
                // Stay open until the change lands, so a failure can be read here.
                event.preventDefault();
                void confirmYolo();
              }}
            >
              {text.launchMode.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Tabs>
  );
}

/**
 * The launch arguments of an agent that is on: a switch, and the text box while the switch is on.
 * Turning the switch off drops the text. Under YOLO the hint names the flag the agent starts with.
 */
function ArgumentsField({
  harness,
  config,
  disabled,
  save,
}: {
  harness: HarnessId;
  config: AppConfig;
  disabled: boolean;
  save: (next: AppConfig) => Promise<void>;
}) {
  const text = copy.settings.agents;
  const name = text.harnesses[harness].name;
  const current = config.agents.launchArguments[harness];
  const enabled = current !== undefined;
  const flag = yoloFlag(harness);
  const yolo = config.agents.launchMode === "yolo";
  const hint = [
    text.launchArguments.hint,
    yolo
      ? flag
        ? text.launchArguments.yoloFlag(flag)
        : text.launchArguments.noYoloFlag
      : null,
  ]
    .filter((part): part is string => part !== null)
    .join(" ");

  return (
    <div className="grid gap-3 pl-10">
      <SwitchSetting
        id={`agent-arguments-on-${harness}`}
        label={text.launchArguments.toggle(name)}
        hint={hint}
        checked={enabled}
        disabled={disabled}
        onCommit={(on) =>
          save(withLaunchArgument(config, harness, on ? "" : null))
        }
      />
      {enabled && (
        <TextSetting
          id={`agent-arguments-${harness}`}
          label={text.launchArguments.text(name)}
          hint={text.launchArguments.hint}
          placeholder={text.launchArguments.placeholder}
          value={current || null}
          disabled={disabled}
          onCommit={(next) =>
            save(withLaunchArgument(config, harness, next ?? ""))
          }
        />
      )}
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
