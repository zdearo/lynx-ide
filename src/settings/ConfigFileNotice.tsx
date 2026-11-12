import { WarningIcon } from "@phosphor-icons/react";

import { useConfigStore } from "@/config/config-store";
import { copy } from "@/ui-copy";

/** Says what is wrong with the config file, if anything, without ever hiding the rest of the screen. */
export function ConfigFileNotice() {
  const snapshot = useConfigStore((state) => state.snapshot);
  if (!snapshot || (!snapshot.fileBroken && snapshot.issues.length === 0)) {
    return null;
  }

  return (
    <div
      role="alert"
      className="flex gap-2 rounded-lg bg-destructive/10 p-3 text-xs text-destructive"
    >
      <WarningIcon className="mt-0.5 size-3.5 shrink-0" aria-hidden />
      <div className="grid gap-1">
        <p className="font-medium">
          {snapshot.fileBroken
            ? copy.settings.file.brokenTitle
            : copy.settings.file.issuesTitle}
        </p>
        {snapshot.fileBroken && <p>{copy.settings.file.brokenBody}</p>}
        <ul className="grid gap-0.5" data-selectable>
          {snapshot.issues.map((issue) => (
            <li key={`${issue.key}-${issue.message}`}>
              {issue.key ? <code>{issue.key}</code> : null}
              {issue.key ? ": " : ""}
              {issue.message}
              {!snapshot.fileBroken && ` (${copy.settings.file.usingDefault})`}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
