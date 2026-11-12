import { useEffect, useState } from "react";
import { toast } from "sonner";

import { commands, type PullRequestForm } from "@/bindings";
import { SparkleIcon, SpinnerGapIcon } from "@phosphor-icons/react";
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
import { refreshWorktreeGit } from "@/changes/refresh-worktree-git";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";
import {
  draftFieldsOf,
  useCreatePullRequestStore,
  type PullRequestDraftFields,
} from "@/pull-request/create-pull-request-store";
import { usePullRequestStore } from "@/pull-request/pull-request-store";
import { copy } from "@/ui-copy";

const text = copy.pullRequest.create;

/** What a refused request says, by the kind the backend named. */
function failureText(error: unknown, fallback: string): string {
  if (error instanceof IpcRequestError) {
    switch (error.kind) {
      case "ghNotInstalled":
        return text.error.notInstalled;
      case "ghNotLoggedIn":
        return text.error.notLoggedIn;
      case "invalidPullRequest":
        return text.error.invalid;
      case "pullRequestExists":
        return text.error.exists;
      case "noDifference":
        return text.error.noDifference;
      default:
        return `${fallback}: ${error.message}`;
    }
  }
  return `${fallback}: ${errorMessage(error)}`;
}

/**
 * The pull request form of a branch that has none: base, title, description and draft, with the
 * push done first when the remote lacks the branch. Fixed at the top of the tab, like the commit box.
 */
export function CreatePullRequestForm({
  worktreePath,
}: {
  worktreePath: string;
}) {
  const info = useCreatePullRequestStore((state) => state.infos[worktreePath]);
  const fields = useCreatePullRequestStore((state) =>
    draftFieldsOf(state.drafts, worktreePath),
  );
  const [busy, setBusy] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    void useCreatePullRequestStore.getState().loadInfo(worktreePath);
  }, [worktreePath]);

  if (!info || info.status === "loading") {
    return (
      <p className="px-3 pt-1 text-ui text-muted-foreground" aria-busy="true">
        {copy.pullRequest.loading}
      </p>
    );
  }
  if (info.status === "error") {
    return (
      <div className="px-3 pt-1" role="alert">
        <p className="text-ui text-destructive">{text.loadFailed}</p>
        <p className="mt-1 text-meta break-words text-muted-foreground">
          {info.message}
        </p>
      </div>
    );
  }

  const form: PullRequestForm = info.form;
  const base = fields.base ?? form.defaultBase;
  const title = fields.title ?? form.suggestedTitle;
  const update = (change: Partial<PullRequestDraftFields>) =>
    useCreatePullRequestStore.getState().update(worktreePath, change);
  const sameBranch = base === form.headBranch;
  const canCreate =
    title.trim() !== "" && base !== "" && !sameBranch && !busy && !generating;
  const bases = form.bases.includes(base) ? form.bases : [base, ...form.bases];

  async function generate() {
    if (generating || busy || sameBranch) return;
    setGenerating(true);
    setError(null);
    setNotice(null);
    try {
      const drafted = await unwrap(
        commands.draftPullRequestText(worktreePath, base),
      );
      update({ title: drafted.title, body: drafted.body });
      setNotice(text.draftedWith(drafted.agentLabel));
    } catch (caught) {
      setError(failureText(caught, text.error.generateFailed));
    } finally {
      setGenerating(false);
    }
  }

  async function submit() {
    if (!canCreate) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await unwrap(
        commands.createPullRequest(worktreePath, {
          base,
          title,
          body: fields.body,
          draft: fields.draft,
        }),
      );
      toast.success(text.created);
      useCreatePullRequestStore.getState().clear(worktreePath);
      await Promise.all([
        usePullRequestStore.getState().refresh(worktreePath),
        refreshWorktreeGit(worktreePath),
      ]);
    } catch (caught) {
      setError(failureText(caught, text.error.failed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
      className="space-y-2 px-3 pt-1 pb-3"
    >
      <h3 className="text-meta font-medium tracking-wide text-muted-foreground uppercase">
        {text.heading}
      </h3>

      <div className="grid gap-1">
        <Label
          htmlFor="pull-request-base"
          className="text-meta text-muted-foreground"
        >
          {text.base}
        </Label>
        <Select
          value={base}
          onValueChange={(value) => update({ base: value })}
          disabled={busy}
        >
          <SelectTrigger id="pull-request-base" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {bases.map((name) => (
              <SelectItem key={name} value={name}>
                {name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {sameBranch && (
          <p className="text-meta text-destructive">
            {form.bases.some((name) => name !== form.headBranch)
              ? text.sameBranch
              : text.noOtherBase(form.headBranch)}
          </p>
        )}
      </div>

      <div className="relative">
        <Input
          aria-label={text.titleLabel}
          placeholder={text.titlePlaceholder}
          value={title}
          className="pr-9"
          onChange={(event) => update({ title: event.target.value })}
        />
        <TooltipIconButton
          label={text.generate}
          tooltipSide="top"
          variant="outline"
          disabled={generating || busy || sameBranch}
          aria-busy={generating}
          className="absolute top-1/2 right-1 size-6 -translate-y-1/2 bg-background"
          onClick={() => void generate()}
        >
          {generating ? (
            <SpinnerGapIcon className="animate-spin" />
          ) : (
            <SparkleIcon />
          )}
        </TooltipIconButton>
      </div>

      <Textarea
        aria-label={text.bodyLabel}
        placeholder={text.bodyPlaceholder}
        rows={5}
        value={fields.body}
        onChange={(event) => update({ body: event.target.value })}
      />

      <div className="flex items-center gap-2">
        <Switch
          id="pull-request-draft"
          checked={fields.draft}
          onCheckedChange={(draft) => update({ draft })}
          disabled={busy}
        />
        <Label htmlFor="pull-request-draft" className="text-ui">
          {text.draft}
        </Label>
        <Button
          type="submit"
          size="sm"
          className="ml-auto"
          disabled={!canCreate}
        >
          {busy
            ? text.creating
            : form.needsPush
              ? text.pushAndSubmit
              : text.submit}
        </Button>
      </div>

      <p role="status" className="truncate text-meta text-muted-foreground">
        {generating
          ? text.generating
          : (notice ??
            (form.needsPush
              ? text.pushHint(form.headBranch, form.remote)
              : null))}
      </p>
      {error && (
        <p role="alert" className="text-meta break-words text-destructive">
          {error}
        </p>
      )}
    </form>
  );
}
