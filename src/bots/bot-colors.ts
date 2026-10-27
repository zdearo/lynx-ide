/** The colours a bot can wear, in the order the form offers them. The names match the backend's. */
export const BOT_COLORS = [
  "rose",
  "sky",
  "indigo",
  "violet",
  "amber",
  "green",
  "teal",
  "graphite",
] as const;

export type BotColor = (typeof BOT_COLORS)[number];

/** The providers a bot can name. Only Claude answers so far. */
export const BOT_PROVIDERS = ["claude", "chatgpt", "grok"] as const;

export type BotProvider = (typeof BOT_PROVIDERS)[number];

/** The providers Lynx can reach today. The others can be named, but a bot on them does not answer yet. */
export const REACHABLE_PROVIDERS: readonly BotProvider[] = ["claude"];
