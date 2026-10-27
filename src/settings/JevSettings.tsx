import { useEffect, useState } from "react";
import { toast } from "sonner";

import { commands, type JevTest } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { SwitchSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

/** Above this, a channel goes on without Jev, so the test says so. */
const SLOW_MS = 1500;

/** What the test found, in one line. */
function testText(result: JevTest): string {
  const text = copy.settings.bots.jev.test;
  if (!result.ok) return text.failed(result.reason ?? "");
  return result.latencyMs > SLOW_MS
    ? text.slow(result.latencyMs)
    : text.ok(result.latencyMs);
}

/** Jev, the decision model that picks who answers in a channel: its switch and its key. */
export function JevSettings() {
  const { config, disabled, save } = useSettings();
  // Whether a key is saved. The key itself never comes back to the screen.
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [draft, setDraft] = useState("");
  const [result, setResult] = useState<JevTest | null>(null);
  const [working, setWorking] = useState(false);
  const text = copy.settings.bots.jev;

  useEffect(() => {
    let current = true;
    unwrap(commands.botsJevStatus())
      .then((status) => {
        if (current) setConfigured(status.configured);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, []);

  if (!config) return null;

  async function run(action: () => Promise<void>, failure: string) {
    setWorking(true);
    try {
      await action();
    } catch (error) {
      toast.error(failure, { description: errorMessage(error) });
    } finally {
      setWorking(false);
    }
  }

  return (
    <>
      <div className="grid gap-1.5">
        <p className="text-ui">{text.title}</p>
        <p className="text-nested text-muted-foreground">{text.intro}</p>
      </div>
      <SwitchSetting
        id="bots-decider-enabled"
        label={text.enabled.label}
        hint={text.enabled.hint}
        checked={config.bots.decider.enabled}
        disabled={disabled}
        onCommit={(enabled) =>
          save({
            ...config,
            bots: { ...config.bots, decider: { enabled } },
          })
        }
      />
      <div className="grid gap-1.5">
        <label htmlFor="jev-key" className="text-ui">
          {text.key.label}
        </label>
        <p role="status" className="text-nested text-muted-foreground">
          {configured ? text.key.saved : text.key.none}
        </p>
        <div className="flex gap-2">
          <Input
            id="jev-key"
            type="password"
            autoComplete="off"
            value={draft}
            placeholder={text.key.placeholder}
            onChange={(event) => setDraft(event.target.value)}
          />
          <Button
            disabled={working || !draft.trim()}
            onClick={() =>
              void run(async () => {
                const status = await unwrap(commands.botsJevSaveKey(draft));
                setConfigured(status.configured);
                setDraft("");
                setResult(null);
              }, text.key.saveFailed)
            }
          >
            {text.key.save}
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          disabled={working || !configured}
          onClick={() =>
            void run(async () => {
              setResult(await unwrap(commands.botsJevTest()));
            }, text.test.runFailed)
          }
        >
          {text.test.action}
        </Button>
        <Button
          variant="outline"
          disabled={working || !configured}
          onClick={() =>
            void run(async () => {
              const status = await unwrap(commands.botsJevClearKey());
              setConfigured(status.configured);
              setResult(null);
            }, text.key.removeFailed)
          }
        >
          {text.key.remove}
        </Button>
        {result && (
          <p role="status" className="text-nested">
            {testText(result)}
          </p>
        )}
      </div>
    </>
  );
}
