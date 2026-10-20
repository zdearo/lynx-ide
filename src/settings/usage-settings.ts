import type { HarnessId } from "@/bindings";
import { USAGE_PROVIDERS } from "@/config/config-store";

/** The plans after one is switched on or off, in the order the footer shows them. */
export function providersWith(
  current: readonly HarnessId[],
  harness: HarnessId,
  on: boolean,
): HarnessId[] {
  const others = current.filter((item) => item !== harness);
  const next = on ? [...others, harness] : others;
  return USAGE_PROVIDERS.filter((item) => next.includes(item));
}

/** The warning level can go up to one below the "almost used up" level, and no lower than 1. */
export function warnAtBounds(criticalAt: number): { min: number; max: number } {
  return { min: 1, max: criticalAt - 1 };
}

/** The "almost used up" level must sit above the warning, and cannot pass 100. */
export function criticalAtBounds(warnAt: number): { min: number; max: number } {
  return { min: warnAt + 1, max: 100 };
}
