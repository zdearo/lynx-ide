import type { ComponentProps } from "react";

import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface TooltipIconButtonProps extends ComponentProps<typeof Button> {
  label: string;
  shortcut?: string;
  /** Where the tooltip opens; beside the button for a vertical bar. */
  tooltipSide?: "top" | "right" | "bottom" | "left";
}

/** An icon-only button that names itself to screen readers and on hover. */
export function TooltipIconButton({
  label,
  shortcut,
  tooltipSide = "bottom",
  children,
  ...props
}: TooltipIconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={label}
          {...props}
        >
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent side={tooltipSide}>
        {label}
        {shortcut && (
          <span className="ml-2 text-background/70 tabular-nums">
            {shortcut}
          </span>
        )}
      </TooltipContent>
    </Tooltip>
  );
}
