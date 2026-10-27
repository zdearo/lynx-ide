import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
}

/**
 * A row of toggle buttons with one pressed. Buttons rather than tabs: a tab list would also count as
 * the terminals' tabs.
 */
export function SegmentedControl<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: readonly SegmentOption<T>[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div
      role="group"
      aria-label={label}
      className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
    >
      {options.map((option) => (
        <Button
          key={option.value}
          type="button"
          variant="ghost"
          size="sm"
          aria-pressed={value === option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            "h-6 px-2 text-ui",
            value === option.value
              ? "bg-background text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </Button>
      ))}
    </div>
  );
}
