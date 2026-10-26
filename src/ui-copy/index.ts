import { shellCopy } from "@/ui-copy/shell";
import { settingsCopy } from "@/ui-copy/settings";
import { browserCopy } from "@/ui-copy/browser";
import { filesCopy } from "@/ui-copy/files";
import { changesCopy } from "@/ui-copy/changes";
import { workspaceCopy } from "@/ui-copy/workspace";
import { terminalCopy } from "@/ui-copy/terminal";
import { usageCopy } from "@/ui-copy/usage";

/** Every user-facing string lives under src/ui-copy, one file per area, so the interface can be translated later. */
export const copy = {
  appName: "Lynx",
  ...shellCopy,
  ...settingsCopy,
  ...browserCopy,
  ...filesCopy,
  ...changesCopy,
  ...workspaceCopy,
  ...terminalCopy,
  ...usageCopy,
} as const;
