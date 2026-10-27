import {
  BOT_PROVIDERS,
  REACHABLE_PROVIDERS,
  type BotProvider,
} from "@/bots/bot-colors";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

/** The engines a bot can run on, as a choice. Only the engines Lynx can reach can be picked. */
export function EnginePicker({
  value,
  onChange,
}: {
  value: BotProvider;
  onChange: (provider: BotProvider) => void;
}) {
  const text = copy.bots.wizard.engine;
  return (
    <div role="radiogroup" aria-label={text.label} className="grid gap-1.5">
      {BOT_PROVIDERS.map((option: BotProvider) => {
        const reachable = REACHABLE_PROVIDERS.includes(option);
        const checked = value === option;
        return (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={checked}
            disabled={!reachable}
            onClick={() => onChange(option)}
            className={cn(
              "flex items-center justify-between gap-2 rounded-md border px-3 py-2 text-left text-ui outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-60",
              checked ? "border-ring bg-accent" : "border-input",
            )}
          >
            <span>{copy.bots.providers[option]}</span>
            {!reachable && (
              <span className="text-meta text-muted-foreground">
                {text.comingSoon}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
