/** One entry of a choice field. `null` is the "not set" entry, which means the default. */
export interface ChoiceOption {
  value: string | null;
  label: string;
}

/** Select items need text keys, so "not set" and every real value get distinct ones. */
export const CUSTOM_KEY = "custom";

export function choiceKey(value: string | null): string {
  return value === null ? "default" : `value:${value}`;
}

/** A value no listed entry offers: typed by hand in the file, or typed into the custom field. */
export function isCustomValue(
  value: string | null,
  options: readonly ChoiceOption[],
): boolean {
  return value !== null && !options.some((option) => option.value === value);
}

/**
 * The entries to show. A value outside the list gets an entry of its own, so the
 * select never looks empty; a field with a custom entry shows the text box instead.
 */
export function choiceItems(
  value: string | null,
  options: readonly ChoiceOption[],
  hasCustom: boolean,
): ChoiceOption[] {
  if (hasCustom || value === null || !isCustomValue(value, options)) {
    return [...options];
  }
  return [...options, { value, label: value }];
}
