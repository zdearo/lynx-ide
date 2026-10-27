import type { ReactNode } from "react";
import { CircleInfoGlyph18 } from "@/components/icons/CircleInfoGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";

/** One block of the bot details tab: a title, an optional hint on hover, the actions, and the content. */
export function DetailSection({
  title,
  hint,
  actions,
  children,
}: {
  title: string;
  hint?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section
      aria-label={title}
      className="grid gap-2 border-t border-border pt-4"
    >
      <div className="flex min-h-6 items-center gap-1">
        <h3 className="text-ui font-medium">{title}</h3>
        {hint && (
          <TooltipIconButton
            label={hint}
            tooltipSide="top"
            className="text-muted-foreground"
          >
            <CircleInfoGlyph18 />
          </TooltipIconButton>
        )}
        <div className="ml-auto flex items-center gap-0.5">{actions}</div>
      </div>
      {children}
    </section>
  );
}

/** A list row: the name and a line under it, with the row's action at the end. */
export function DetailRow({
  title,
  subtitle,
  mono = false,
  leading,
  action,
}: {
  title: string;
  subtitle?: string;
  mono?: boolean;
  leading?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <li className="flex min-w-0 items-center gap-2 rounded-md bg-muted/40 py-1.5 pr-1 pl-2.5">
      {leading}
      <div className="min-w-0 flex-1">
        <p
          className={`truncate text-ui ${mono && !subtitle ? "font-mono" : ""}`}
        >
          {title}
        </p>
        {subtitle && (
          <p
            className={`truncate text-meta text-muted-foreground ${mono ? "font-mono" : ""}`}
          >
            {subtitle}
          </p>
        )}
      </div>
      {action}
    </li>
  );
}
