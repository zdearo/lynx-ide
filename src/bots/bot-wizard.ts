import type { NewBot } from "@/bindings";
import {
  REACHABLE_PROVIDERS,
  type BotColor,
  type BotProvider,
} from "@/bots/bot-colors";

/** The steps of the wizard, in the order it shows them. The last one only reviews. */
export const WIZARD_STEPS = [
  "name",
  "engine",
  "instructions",
  "review",
] as const;

export type WizardStep = (typeof WIZARD_STEPS)[number];

/** What the wizard has collected so far. */
export interface WizardDraft {
  name: string;
  color: BotColor;
  provider: BotProvider;
  model: string;
  instructions: string;
  /** Optional: the label the bot wears in the sidebar. */
  team: string;
}

export const EMPTY_DRAFT: WizardDraft = {
  name: "",
  color: "rose",
  provider: "claude",
  model: "",
  instructions: "",
  team: "",
};

// The same limits the backend enforces, so a bot the wizard accepts is one Lynx can make.
const MAX_NAME = 40;
const MAX_MODEL = 80;
const MAX_INSTRUCTIONS = 8000;
const MAX_TEAM = 24;

function nameIsValid(name: string): boolean {
  const trimmed = name.trim();
  return (
    trimmed.length > 0 &&
    [...trimmed].length <= MAX_NAME &&
    /[\p{L}\p{N}]/u.test(trimmed)
  );
}

/** Whether what the wizard holds for `step` is enough to go on. */
export function canAdvance(step: WizardStep, draft: WizardDraft): boolean {
  switch (step) {
    case "name":
      return nameIsValid(draft.name) && draft.team.trim().length <= MAX_TEAM;
    case "engine":
      return (
        REACHABLE_PROVIDERS.includes(draft.provider) &&
        draft.model.trim().length <= MAX_MODEL
      );
    case "instructions": {
      const length = draft.instructions.trim().length;
      return length > 0 && length <= MAX_INSTRUCTIONS;
    }
    case "review":
      return (
        canAdvance("name", draft) &&
        canAdvance("engine", draft) &&
        canAdvance("instructions", draft)
      );
  }
}

/** The request the backend takes to make the bot the draft describes. */
export function toNewBot(draft: WizardDraft): NewBot {
  return {
    name: draft.name.trim(),
    color: draft.color,
    provider: draft.provider,
    model: draft.model.trim() || null,
    instructions: draft.instructions.trim(),
    team: draft.team.trim() || null,
  };
}
