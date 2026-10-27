import type { BotSummary } from "@/bindings";
import { cn } from "@/lib/utils";

/** The bots of a channel, as toggle chips: the ones picked are its members. */
export function MemberPicker({
  bots,
  value,
  onChange,
}: {
  bots: BotSummary[];
  value: string[];
  onChange: (members: string[]) => void;
}) {
  function toggle(id: string) {
    onChange(
      value.includes(id)
        ? value.filter((member) => member !== id)
        : [...value, id],
    );
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      {bots.map((bot) => {
        const picked = value.includes(bot.id);
        return (
          <button
            key={bot.id}
            type="button"
            aria-pressed={picked}
            onClick={() => toggle(bot.id)}
            className={cn(
              "rounded-md border px-2.5 py-1 text-meta outline-none focus-visible:ring-2 focus-visible:ring-ring",
              picked
                ? "border-ring bg-accent text-foreground"
                : "border-input text-muted-foreground",
            )}
          >
            {bot.name}
          </button>
        );
      })}
    </div>
  );
}
