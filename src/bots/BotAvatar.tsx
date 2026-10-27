import { cn } from "@/lib/utils";

/** A bot's face in the interface: its colour, with the first letter of its name on it. */
export function BotAvatar({
  name,
  color,
  size = "sm",
}: {
  name: string;
  color: string;
  size?: "sm" | "lg";
}) {
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-md font-medium text-[var(--bot-ink)]",
        size === "sm" ? "size-6 text-meta" : "size-10 text-ui",
      )}
      style={{ backgroundColor: `var(--bot-${color})` }}
    >
      {initial}
    </span>
  );
}
