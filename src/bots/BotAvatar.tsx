import { cn } from "@/lib/utils";

const SIZES = {
  xs: "size-4 text-meta",
  sm: "size-6 text-meta",
  lg: "size-10 text-ui",
} as const;

/** A bot's face in the interface: its colour, with the first letter of its name on it. */
export function BotAvatar({
  name,
  color,
  size = "sm",
}: {
  name: string;
  color: string;
  size?: keyof typeof SIZES;
}) {
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-md font-medium text-[var(--bot-ink)]",
        SIZES[size],
      )}
      style={{ backgroundColor: `var(--bot-${color})` }}
    >
      {initial}
    </span>
  );
}
