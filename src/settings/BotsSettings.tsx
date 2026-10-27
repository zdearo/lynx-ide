import { toast } from "sonner";

import type { ChatGptStatus } from "@/bindings";
import { useChatGptAuthStore } from "@/bots/chatgpt-auth-store";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/error-message";
import { JevSettings } from "@/settings/JevSettings";
import { SwitchSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

/** Where the ChatGPT sign-in stands, in one line. */
function phaseText(status: ChatGptStatus): string {
  const text = copy.settings.bots.chatgpt;
  switch (status.phase) {
    case "signedOut":
      return text.signedOut;
    case "signingIn":
      return text.signingIn;
    case "signedIn":
      return text.signedIn(status.email);
    case "failed":
      return text.failed;
  }
}

async function run(action: () => Promise<void>, failure: string) {
  try {
    await action();
  } catch (error) {
    toast.error(failure, { description: errorMessage(error) });
  }
}

export function BotsSettings() {
  const { config, disabled, save } = useSettings();
  const status = useChatGptAuthStore((state) => state.status);
  const signIn = useChatGptAuthStore((state) => state.signIn);
  const signOut = useChatGptAuthStore((state) => state.signOut);
  if (!config || !status) return null;

  const text = copy.settings.bots;
  const busy = status.phase === "signingIn";

  return (
    <>
      <p className="text-nested text-muted-foreground">{text.intro}</p>
      <SwitchSetting
        id="bots-chatgpt-enabled"
        label={text.enabled.label}
        hint={text.enabled.hint}
        checked={config.bots.chatgpt.enabled}
        disabled={disabled}
        onCommit={(enabled) =>
          save({ ...config, bots: { ...config.bots, chatgpt: { enabled } } })
        }
      />

      <div className="grid gap-1.5">
        <p className="text-ui">{text.chatgpt.title}</p>
        <p className="text-nested text-muted-foreground">
          {text.chatgpt.intro}
        </p>
        <p role="status" className="text-nested">
          {phaseText(status)}
        </p>
        {status.detail && (
          <p className="text-meta text-muted-foreground">{status.detail}</p>
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        {status.phase === "signedIn" ? (
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => void run(signOut, text.chatgpt.signOutFailed)}
          >
            {text.chatgpt.signOut}
          </Button>
        ) : (
          <Button
            disabled={busy}
            onClick={() => void run(signIn, text.chatgpt.signInFailed)}
          >
            {text.chatgpt.signIn}
          </Button>
        )}
      </div>

      <SwitchSetting
        id="bots-memory-capture"
        label={text.memory.label}
        hint={text.memory.hint}
        checked={config.bots.memory.capture}
        disabled={disabled}
        onCommit={(capture) =>
          save({
            ...config,
            bots: { ...config.bots, memory: { capture } },
          })
        }
      />

      <JevSettings />
    </>
  );
}
