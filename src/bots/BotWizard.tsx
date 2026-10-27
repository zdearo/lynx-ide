import { useState, type FormEvent } from "react";

import { useDialogStore } from "@/dialog-store";
import { BotAvatar } from "@/bots/BotAvatar";
import { BOT_COLORS, type BotColor } from "@/bots/bot-colors";
import { EnginePicker } from "@/bots/EnginePicker";
import {
  EMPTY_DRAFT,
  WIZARD_STEPS,
  canAdvance,
  toNewBot,
  type WizardDraft,
  type WizardStep,
} from "@/bots/bot-wizard";
import { useBotsStore } from "@/bots/bots-store";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

/** Makes a bot in a few steps, like an onboarding. The new bot is picked as soon as it exists. */
export function BotWizard() {
  const askBotWizard = useDialogStore((state) => state.askBotWizard);
  const create = useBotsStore((state) => state.create);
  const [step, setStep] = useState<WizardStep>("name");
  const [draft, setDraft] = useState<WizardDraft>(EMPTY_DRAFT);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const text = copy.bots.wizard;
  const index = WIZARD_STEPS.indexOf(step);
  const last = index === WIZARD_STEPS.length - 1;
  const ready = canAdvance(step, draft);

  function change(next: Partial<WizardDraft>) {
    setDraft((current) => ({ ...current, ...next }));
  }

  function back() {
    const previous = WIZARD_STEPS[index - 1];
    if (previous) setStep(previous);
  }

  async function next(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!ready || saving) return;
    const following = WIZARD_STEPS[index + 1];
    if (following) {
      setStep(following);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await create(toNewBot(draft));
      askBotWizard(false);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && askBotWizard(false)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>
            {text.stepOf(index + 1, WIZARD_STEPS.length)}
          </DialogDescription>
        </DialogHeader>

        <ol className="flex gap-1" aria-label={text.title}>
          {WIZARD_STEPS.map((candidate, position) => (
            <li
              key={candidate}
              aria-current={candidate === step ? "step" : undefined}
              className={cn(
                "h-1 flex-1 rounded-full",
                position <= index ? "bg-foreground" : "bg-muted",
              )}
            >
              <span className="sr-only">{text.steps[candidate]}</span>
            </li>
          ))}
        </ol>

        <form onSubmit={(event) => void next(event)} className="grid gap-4">
          {step === "name" && <NameStep draft={draft} onChange={change} />}
          {step === "engine" && <EngineStep draft={draft} onChange={change} />}
          {step === "instructions" && (
            <InstructionsStep draft={draft} onChange={change} />
          )}
          {step === "review" && <ReviewStep draft={draft} />}

          {error && (
            <p role="alert" className="text-meta text-destructive">
              {text.createFailed}: {error}
            </p>
          )}

          <DialogFooter>
            {index > 0 && (
              <Button type="button" variant="outline" onClick={back}>
                {text.back}
              </Button>
            )}
            <Button type="submit" disabled={!ready || saving}>
              {last ? text.add : text.next}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

interface StepProps {
  draft: WizardDraft;
  onChange: (change: Partial<WizardDraft>) => void;
}

function NameStep({ draft, onChange }: StepProps) {
  const text = copy.bots.wizard;
  return (
    <div className="grid gap-4">
      <StepTitle title={text.name.title} body={text.name.body} />
      <div className="flex items-center gap-3">
        <BotAvatar name={draft.name} color={draft.color} size="lg" />
        <p className="min-w-0 truncate text-ui">
          {draft.name.trim() || text.name.placeholder}
        </p>
      </div>
      <label className="grid gap-1.5 text-meta text-muted-foreground">
        {text.name.label}
        <Input
          value={draft.name}
          maxLength={40}
          placeholder={text.name.placeholder}
          autoFocus
          onChange={(event) => onChange({ name: event.target.value })}
        />
      </label>
      <label className="grid gap-1.5 text-meta text-muted-foreground">
        {text.name.team}
        <Input
          value={draft.team}
          maxLength={24}
          placeholder={text.name.teamPlaceholder}
          onChange={(event) => onChange({ team: event.target.value })}
        />
      </label>
      <fieldset className="grid gap-1.5">
        <legend className="text-meta text-muted-foreground">
          {text.name.colour}
        </legend>
        <div className="flex flex-wrap gap-1.5">
          {BOT_COLORS.map((option: BotColor) => (
            <button
              key={option}
              type="button"
              aria-pressed={draft.color === option}
              aria-label={text.name.colours[option]}
              title={text.name.colours[option]}
              onClick={() => onChange({ color: option })}
              className={cn(
                "size-6 rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring",
                draft.color === option &&
                  "ring-2 ring-ring ring-offset-2 ring-offset-popover",
              )}
              style={{ backgroundColor: `var(--bot-${option})` }}
            />
          ))}
        </div>
      </fieldset>
    </div>
  );
}

function EngineStep({ draft, onChange }: StepProps) {
  const text = copy.bots.wizard;
  return (
    <div className="grid gap-4">
      <StepTitle title={text.engine.title} body={text.engine.body} />
      <EnginePicker
        value={draft.provider}
        onChange={(provider) => onChange({ provider })}
      />
      <label className="grid gap-1.5 text-meta text-muted-foreground">
        {text.engine.model}
        <Input
          value={draft.model}
          maxLength={80}
          placeholder={text.engine.modelPlaceholder}
          onChange={(event) => onChange({ model: event.target.value })}
        />
        <span>{text.engine.modelHint}</span>
      </label>
    </div>
  );
}

function InstructionsStep({ draft, onChange }: StepProps) {
  const text = copy.bots.wizard;
  return (
    <div className="grid gap-4">
      <StepTitle
        title={text.instructions.title}
        body={text.instructions.body}
      />
      <label className="grid gap-1.5 text-meta text-muted-foreground">
        {text.instructions.label}
        <Textarea
          value={draft.instructions}
          rows={8}
          maxLength={8000}
          placeholder={text.instructions.placeholder}
          autoFocus
          onChange={(event) => onChange({ instructions: event.target.value })}
        />
        <span>{text.instructions.hint}</span>
      </label>
    </div>
  );
}

function ReviewStep({ draft }: { draft: WizardDraft }) {
  const text = copy.bots.wizard;
  const rows: [string, string][] = [
    [text.review.name, draft.name.trim()],
    [text.review.colour, text.name.colours[draft.color]],
    [text.review.engine, copy.bots.providers[draft.provider]],
    [text.review.model, draft.model.trim() || text.engine.modelPlaceholder],
  ];
  return (
    <div className="grid gap-4">
      <StepTitle title={text.review.title} body={text.review.body} />
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-ui">
        {rows.map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-meta text-muted-foreground">{label}</dt>
            <dd className="min-w-0 truncate">{value}</dd>
          </div>
        ))}
        <dt className="text-meta text-muted-foreground">
          {text.review.instructions}
        </dt>
        <dd className="max-h-40 min-w-0 overflow-y-auto whitespace-pre-wrap">
          {draft.instructions.trim()}
        </dd>
      </dl>
    </div>
  );
}

function StepTitle({ title, body }: { title: string; body: string }) {
  return (
    <div>
      <h3 className="text-ui font-medium">{title}</h3>
      <p className="mt-0.5 text-meta text-muted-foreground">{body}</p>
    </div>
  );
}
