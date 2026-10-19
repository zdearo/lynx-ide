import { useState, type KeyboardEvent, type ReactNode } from "react";
import { MinusGlyph18 } from "@/components/icons/MinusGlyph18";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";

import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { friendlyConfigError } from "@/config/config-store";
import { errorMessage } from "@/lib/error-message";
import {
  CUSTOM_KEY,
  choiceItems,
  choiceKey,
  isCustomValue,
  type ChoiceOption,
} from "@/settings/choice-field";
import { copy } from "@/ui-copy";

/** Holds the reason the last save was refused, so the field can say it under itself. */
function useSaveError() {
  const [error, setError] = useState<string | null>(null);

  async function attempt(save: () => Promise<void>) {
    try {
      await save();
      setError(null);
    } catch (failure) {
      setError(friendlyConfigError(errorMessage(failure)));
    }
  }

  return { error, clearError: () => setError(null), attempt };
}

function Note({
  id,
  hint,
  error,
}: {
  id: string;
  hint: string;
  error: string | null;
}) {
  return error ? (
    <p id={id} role="alert" className="text-nested text-destructive">
      {error}
    </p>
  ) : (
    <p id={id} className="text-nested text-muted-foreground">
      {hint}
    </p>
  );
}

interface SettingFrameProps {
  id: string;
  label: string;
  hint: string;
  error: string | null;
  canReset: boolean;
  onReset: () => void;
  disabled: boolean;
  children: ReactNode;
}

function SettingFrame({
  id,
  label,
  hint,
  error,
  canReset,
  onReset,
  disabled,
  children,
}: SettingFrameProps) {
  return (
    <div className="grid gap-1.5">
      <div className="flex h-5 items-center justify-between">
        <Label htmlFor={id}>{label}</Label>
        {canReset && !disabled && (
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="text-muted-foreground"
            onClick={onReset}
          >
            {copy.settings.reset}
          </Button>
        )}
      </div>
      {children}
      <Note id={`${id}-note`} hint={hint} error={error} />
    </div>
  );
}

function blurOnEnter(event: KeyboardEvent<HTMLInputElement>) {
  if (event.key === "Enter") event.currentTarget.blur();
}

interface TextSettingProps {
  id: string;
  label: string;
  hint: string;
  placeholder: string;
  /** `null` means "not set", which is the default. */
  value: string | null;
  disabled: boolean;
  onCommit: (value: string | null) => Promise<void>;
  /** A button beside the field that picks a value, such as a folder, and saves it. */
  browse?: { label: string; onBrowse: () => Promise<void> };
}

/** Saves when the field loses focus or Enter is pressed, and shows the backend's reason if it refuses. */
export function TextSetting({
  id,
  label,
  hint,
  placeholder,
  value,
  disabled,
  onCommit,
  browse,
}: TextSettingProps) {
  const { error, clearError, attempt } = useSaveError();

  function commit(next: string | null) {
    if (next === value) clearError();
    else void attempt(() => onCommit(next));
  }

  return (
    <SettingFrame
      id={id}
      label={label}
      hint={hint}
      error={error}
      canReset={value !== null}
      onReset={() => commit(null)}
      disabled={disabled}
    >
      <div className="flex gap-2">
        {/* Re-keyed by the saved value, so an edit made in the file shows up here. */}
        <Input
          key={value ?? ""}
          id={id}
          className="min-w-0 flex-1"
          defaultValue={value ?? ""}
          placeholder={placeholder}
          disabled={disabled}
          autoComplete="off"
          spellCheck={false}
          aria-invalid={error !== null}
          aria-describedby={`${id}-note`}
          onBlur={(event) => commit(event.currentTarget.value.trim() || null)}
          onKeyDown={blurOnEnter}
        />
        {browse && (
          <Button
            type="button"
            variant="outline"
            disabled={disabled}
            onClick={() => void attempt(browse.onBrowse)}
          >
            {browse.label}
          </Button>
        )}
      </div>
    </SettingFrame>
  );
}

interface TextAreaSettingProps {
  id: string;
  label: string;
  hint: string;
  placeholder: string;
  /** `null` means "not set", which is the default. */
  value: string | null;
  disabled: boolean;
  onCommit: (value: string | null) => Promise<void>;
}

/** A multi-line text field. Saves when it loses focus; blank text means "not set". */
export function TextAreaSetting({
  id,
  label,
  hint,
  placeholder,
  value,
  disabled,
  onCommit,
}: TextAreaSettingProps) {
  const { error, clearError, attempt } = useSaveError();

  function commit(next: string | null) {
    if (next === value) clearError();
    else void attempt(() => onCommit(next));
  }

  return (
    <SettingFrame
      id={id}
      label={label}
      hint={hint}
      error={error}
      canReset={value !== null}
      onReset={() => commit(null)}
      disabled={disabled}
    >
      {/* Re-keyed by the saved value, so an edit made in the file shows up here. */}
      <Textarea
        key={value ?? ""}
        id={id}
        className="min-h-24"
        defaultValue={value ?? ""}
        placeholder={placeholder}
        disabled={disabled}
        spellCheck={false}
        aria-invalid={error !== null}
        aria-describedby={`${id}-note`}
        onBlur={(event) => commit(event.currentTarget.value.trim() || null)}
      />
    </SettingFrame>
  );
}

interface ChoiceSettingProps {
  id: string;
  label: string;
  hint: string;
  /** `null` means "not set", which is the default. */
  value: string | null;
  defaultValue: string | null;
  options: readonly ChoiceOption[];
  /** Adds a last entry that opens a text box for a value the list does not have. */
  custom?: { label: string; placeholder: string };
  disabled: boolean;
  onCommit: (value: string | null) => Promise<void>;
}

/** A select that saves on each pick. With `custom`, picking the last entry opens a text box. */
export function ChoiceSetting({
  id,
  label,
  hint,
  value,
  defaultValue,
  options,
  custom,
  disabled,
  onCommit,
}: ChoiceSettingProps) {
  const { error, clearError, attempt } = useSaveError();
  const [customOpen, setCustomOpen] = useState(false);
  const typedValue = isCustomValue(value, options);
  const showCustom = custom !== undefined && (customOpen || typedValue);
  const items = choiceItems(value, options, custom !== undefined);

  function commit(next: string | null) {
    if (next === value) clearError();
    else void attempt(() => onCommit(next));
  }

  function choose(key: string) {
    if (key === CUSTOM_KEY) {
      setCustomOpen(true);
      return;
    }
    setCustomOpen(false);
    const item = items.find((candidate) => choiceKey(candidate.value) === key);
    if (item) commit(item.value);
  }

  function commitCustom(text: string) {
    const next = text.trim() || null;
    // Only an empty box that was never a custom value is "still choosing": nothing to save yet.
    if (next === null && !typedValue) return;
    commit(next);
  }

  return (
    <SettingFrame
      id={id}
      label={label}
      hint={hint}
      error={error}
      canReset={value !== defaultValue}
      onReset={() => {
        setCustomOpen(false);
        commit(defaultValue);
      }}
      disabled={disabled}
    >
      <Select
        value={showCustom ? CUSTOM_KEY : choiceKey(value)}
        disabled={disabled}
        onValueChange={choose}
      >
        <SelectTrigger
          id={id}
          className="w-full"
          aria-invalid={error !== null}
          aria-describedby={`${id}-note`}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {items.map((item) => (
            <SelectItem
              key={choiceKey(item.value)}
              value={choiceKey(item.value)}
            >
              {item.label}
            </SelectItem>
          ))}
          {custom && <SelectItem value={CUSTOM_KEY}>{custom.label}</SelectItem>}
        </SelectContent>
      </Select>
      {custom && showCustom && (
        <Input
          // Re-keyed when the custom value changes, so the box shows what was saved.
          key={typedValue ? (value ?? "") : "open"}
          aria-label={label}
          className="w-full"
          defaultValue={typedValue ? (value ?? "") : ""}
          placeholder={custom.placeholder}
          disabled={disabled}
          autoComplete="off"
          spellCheck={false}
          aria-invalid={error !== null}
          aria-describedby={`${id}-note`}
          onBlur={(event) => commitCustom(event.currentTarget.value)}
          onKeyDown={blurOnEnter}
        />
      )}
    </SettingFrame>
  );
}

interface StepperSettingProps {
  id: string;
  label: string;
  hint: string;
  value: number;
  defaultValue: number;
  min: number;
  max: number;
  disabled: boolean;
  onCommit: (value: number) => Promise<void>;
}

/** A number changed one step at a time, so it never holds a value that was not offered. */
export function StepperSetting({
  id,
  label,
  hint,
  value,
  defaultValue,
  min,
  max,
  disabled,
  onCommit,
}: StepperSettingProps) {
  const { error, attempt } = useSaveError();
  const [busy, setBusy] = useState(false);
  const locked = disabled || busy;

  // Waits for each save, so two quick clicks cannot both step from the same value.
  async function save(next: number) {
    setBusy(true);
    await attempt(() => onCommit(next));
    setBusy(false);
  }

  return (
    <SettingFrame
      id={id}
      label={label}
      hint={hint}
      error={error}
      canReset={value !== defaultValue}
      onReset={() => void save(defaultValue)}
      disabled={disabled}
    >
      <div className="flex items-center gap-2">
        <TooltipIconButton
          label={copy.settings.decrease}
          disabled={locked || value <= min}
          onClick={() => void save(value - 1)}
        >
          <MinusGlyph18 />
        </TooltipIconButton>
        <output
          id={id}
          aria-live="polite"
          className="w-12 text-center text-ui tabular-nums"
        >
          {value}
        </output>
        <TooltipIconButton
          label={copy.settings.increase}
          disabled={locked || value >= max}
          onClick={() => void save(value + 1)}
        >
          <PlusGlyph18 />
        </TooltipIconButton>
      </div>
    </SettingFrame>
  );
}

interface SwitchSettingProps {
  id: string;
  label: string;
  hint: string;
  checked: boolean;
  disabled: boolean;
  onCommit: (checked: boolean) => Promise<void>;
}

/** An on/off setting: the label and hint on the left, the switch on the right. */
export function SwitchSetting({
  id,
  label,
  hint,
  checked,
  disabled,
  onCommit,
}: SwitchSettingProps) {
  const { error, attempt } = useSaveError();

  return (
    <div className="grid gap-1.5">
      <div className="flex items-start justify-between gap-4">
        <div className="grid gap-0.5">
          <Label htmlFor={id}>{label}</Label>
          <p id={`${id}-note`} className="text-nested text-muted-foreground">
            {hint}
          </p>
        </div>
        <Switch
          id={id}
          checked={checked}
          disabled={disabled}
          aria-describedby={`${id}-note`}
          onCheckedChange={(next) => void attempt(() => onCommit(next))}
        />
      </div>
      {error && (
        <p role="alert" className="text-nested text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
