import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";

import { commands, type GitHubAccount, type RepoVisibility } from "@/bindings";
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
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useDialogStore } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";
import {
  isValidOwner,
  isValidRepoName,
} from "@/pull-request/create-remote-form";
import { usePullRequestStore } from "@/pull-request/pull-request-store";
import { copy } from "@/ui-copy";
import { useSelectedWorktree } from "@/workspace/worktrees/use-selected-worktree";

const text = copy.pullRequest.createRemote;

/** What a refused creation says, by the kind the backend named. */
function failureText(error: unknown): string {
  if (error instanceof IpcRequestError) {
    switch (error.kind) {
      case "ghNotInstalled":
        return text.error.notInstalled;
      case "ghNotLoggedIn":
        return text.error.notLoggedIn;
      case "invalidRepoName":
        return text.error.invalidName;
      case "remoteNameTaken":
        return text.error.nameTaken;
      case "remoteAlreadyExists":
        return text.error.remoteExists;
      case "gitHub":
        return `${text.error.failed}: ${error.message}`;
      default:
        return `${text.error.failed}: ${error.message}`;
    }
  }
  return `${text.error.failed}: ${errorMessage(error)}`;
}

/**
 * Asks for the GitHub account, the name and the visibility, then creates the repository and
 * connects it as origin. Nothing is created until the user confirms.
 */
export function CreateRemoteDialog() {
  const worktreePath = useDialogStore((state) => state.createRemoteFor);
  const selected = useSelectedWorktree();
  const close = () => useDialogStore.getState().askCreateRemote(null);

  const [account, setAccount] = useState<GitHubAccount | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [owner, setOwner] = useState("");
  const [name, setName] = useState(selected?.repo.name ?? "");
  const [visibility, setVisibility] = useState<RepoVisibility>("private");
  const [push, setPush] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    unwrap(commands.getGithubAccount())
      .then((found) => {
        if (cancelled) return;
        setAccount(found);
        setOwner(found.login);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setLoadError(errorMessage(caught));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (worktreePath === null) return null;

  const owners = account ? [account.login, ...account.organizations] : [];
  const valid = isValidOwner(owner) && isValidRepoName(name);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!valid || busy || worktreePath === null) return;
    setBusy(true);
    setError(null);
    try {
      await unwrap(
        commands.createGithubRemote(worktreePath, {
          owner,
          name,
          visibility,
          push,
        }),
      );
      toast.success(text.created);
      await usePullRequestStore.getState().refresh(worktreePath);
      close();
    } catch (caught) {
      setError(failureText(caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>{text.description}</DialogDescription>
        </DialogHeader>

        <form onSubmit={(event) => void submit(event)} className="grid gap-4">
          <div className="grid gap-1.5">
            <Label htmlFor="create-remote-owner">{text.account}</Label>
            <Select
              value={owner}
              onValueChange={setOwner}
              disabled={account === null || busy}
            >
              <SelectTrigger id="create-remote-owner" className="w-full">
                <SelectValue placeholder={loadError ?? ""} />
              </SelectTrigger>
              <SelectContent>
                {owners.map((item) => (
                  <SelectItem key={item} value={item}>
                    {item}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {loadError && (
              <p role="alert" className="text-meta text-destructive">
                {text.loadFailed}: {loadError}
              </p>
            )}
          </div>

          <div className="grid gap-1.5">
            <Label htmlFor="create-remote-name">{text.name}</Label>
            <Input
              id="create-remote-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={busy}
              autoComplete="off"
              spellCheck={false}
              aria-invalid={name !== "" && !isValidRepoName(name)}
              aria-describedby="create-remote-name-hint"
            />
            <p
              id="create-remote-name-hint"
              className="text-meta text-muted-foreground"
            >
              {text.nameHint}
            </p>
          </div>

          <fieldset className="grid gap-1.5">
            <legend className="mb-1 text-ui font-medium">
              {text.visibility}
            </legend>
            {(["private", "public"] as const).map((value) => (
              <label
                key={value}
                className="flex cursor-pointer items-start gap-2 rounded-md px-1 py-1 hover:bg-muted"
              >
                <input
                  type="radio"
                  name="create-remote-visibility"
                  value={value}
                  checked={visibility === value}
                  onChange={() => setVisibility(value)}
                  disabled={busy}
                  className="mt-0.5 accent-foreground"
                />
                <span className="grid gap-0.5">
                  <span className="text-ui">
                    {text.visibilityOptions[value]}
                  </span>
                  <span className="text-meta text-muted-foreground">
                    {text.visibilityHints[value]}
                  </span>
                </span>
              </label>
            ))}
          </fieldset>

          <div className="flex items-start justify-between gap-3">
            <div className="grid gap-0.5">
              <Label htmlFor="create-remote-push">{text.push}</Label>
              <span className="text-meta text-muted-foreground">
                {text.pushHint}
              </span>
            </div>
            <Switch
              id="create-remote-push"
              checked={push}
              onCheckedChange={setPush}
              disabled={busy}
            />
          </div>

          {error && (
            <p role="alert" className="text-meta break-words text-destructive">
              {error}
            </p>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={close}
              disabled={busy}
            >
              {text.cancel}
            </Button>
            <Button type="submit" disabled={!valid || busy || account === null}>
              {busy ? text.creating : text.confirm}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
