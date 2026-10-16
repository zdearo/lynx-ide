import { useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { friendlyConfigError } from "@/config/config-store";
import { errorMessage } from "@/lib/error-message";
import { copy } from "@/ui-copy";

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
      {error ? (
        <p id={`${id}-note`} role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : (
        <p id={`${id}-note`} className="text-xs text-muted-foreground">
          {hint}
        </p>
      )}
    </div>
  );
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
}: TextSettingProps) {
  const [error, setError] = useState<string | null>(null);

  async function commit(next: string | null) {
    const normalized = next?.trim() || null;
    if (normalized === value) {
      setError(null);
      return;
    }
    try {
      await onCommit(normalized);
      setError(null);
    } catch (failure) {
      setError(friendlyConfigError(errorMessage(failure)));
    }
  }

  return (
    <SettingFrame
      id={id}
      label={label}
      hint={hint}
      error={error}
      canReset={value !== null}
      onReset={() => void commit(null)}
      disabled={disabled}
    >
      {/* Re-keyed by the saved value, so an edit made in the file shows up here. */}
      <Input
        key={value ?? ""}
        id={id}
        defaultValue={value ?? ""}
        placeholder={placeholder}
        disabled={disabled}
        autoComplete="off"
        spellCheck={false}
        aria-invalid={error !== null}
        aria-describedby={`${id}-note`}
        onBlur={(event) => void commit(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
    </SettingFrame>
  );
}

interface NumberSettingProps {
  id: string;
  label: string;
  hint: string;
  value: number;
  defaultValue: number;
  disabled: boolean;
  onCommit: (value: number) => Promise<void>;
}

export function NumberSetting({
  id,
  label,
  hint,
  value,
  defaultValue,
  disabled,
  onCommit,
}: NumberSettingProps) {
  const [error, setError] = useState<string | null>(null);

  async function commit(raw: string) {
    const text = raw.trim();
    if (!/^\d+$/.test(text)) {
      setError(copy.settings.invalidNumber);
      return;
    }
    const next = Number(text);
    if (next === value) {
      setError(null);
      return;
    }
    try {
      await onCommit(next);
      setError(null);
    } catch (failure) {
      setError(friendlyConfigError(errorMessage(failure)));
    }
  }

  return (
    <SettingFrame
      id={id}
      label={label}
      hint={hint}
      error={error}
      canReset={value !== defaultValue}
      onReset={() => void commit(String(defaultValue))}
      disabled={disabled}
    >
      <Input
        key={value}
        id={id}
        inputMode="numeric"
        defaultValue={String(value)}
        disabled={disabled}
        autoComplete="off"
        className="w-32 tabular-nums"
        aria-invalid={error !== null}
        aria-describedby={`${id}-note`}
        onBlur={(event) => void commit(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
    </SettingFrame>
  );
}
