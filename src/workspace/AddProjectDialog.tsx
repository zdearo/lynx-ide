import { useState, type FormEvent } from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { FolderPlus, FolderSearch, type LucideIcon } from "lucide-react";

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
import { useDialogStore } from "@/dialog-store";
import { errorMessage } from "@/lib/error-message";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";
import { openRepositoryFromPicker } from "@/workspace/open-repository";
import { useWorkspaceStore } from "@/workspace/workspace-store";

/** Adds a project: a repository already on disk, or a new empty one. */
export function AddProjectDialog() {
  const isOpen = useDialogStore((state) => state.addProjectOpen);
  const askAddProject = useDialogStore((state) => state.askAddProject);

  return (
    <Dialog open={isOpen} onOpenChange={askAddProject}>
      {isOpen && <AddProjectSteps onClose={() => askAddProject(false)} />}
    </Dialog>
  );
}

/** The chooser first; "Create new project" swaps in the form. Remounts on each open. */
function AddProjectSteps({ onClose }: { onClose: () => void }) {
  const [creating, setCreating] = useState(false);

  return (
    <DialogContent className="sm:max-w-md">
      {creating ? (
        <CreateProjectForm onBack={() => setCreating(false)} onDone={onClose} />
      ) : (
        <ProjectChooser
          onBrowse={() => {
            onClose();
            void openRepositoryFromPicker();
          }}
          onCreate={() => setCreating(true)}
        />
      )}
    </DialogContent>
  );
}

function ProjectChooser({
  onBrowse,
  onCreate,
}: {
  onBrowse: () => void;
  onCreate: () => void;
}) {
  return (
    <>
      <DialogHeader>
        <DialogTitle>{copy.addProject.title}</DialogTitle>
        <DialogDescription>{copy.addProject.description}</DialogDescription>
      </DialogHeader>
      <div className="grid gap-2">
        <ProjectOption
          icon={FolderSearch}
          title={copy.addProject.browse}
          description={copy.addProject.browseDescription}
          onSelect={onBrowse}
        />
        <ProjectOption
          icon={FolderPlus}
          title={copy.addProject.create}
          description={copy.addProject.createDescription}
          onSelect={onCreate}
        />
      </div>
    </>
  );
}

function ProjectOption({
  icon: Icon,
  title,
  description,
  onSelect,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="flex items-start gap-3 rounded-lg border border-border p-3 text-left transition-colors hover:bg-accent focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
    >
      <span className="grid size-8 shrink-0 place-items-center rounded-md bg-muted text-muted-foreground">
        <Icon aria-hidden className="size-4" />
      </span>
      <span className="grid min-w-0 gap-0.5">
        <span className="text-ui font-medium text-foreground">{title}</span>
        <span className="text-nested text-muted-foreground">{description}</span>
      </span>
    </button>
  );
}

function CreateProjectForm({
  onBack,
  onDone,
}: {
  onBack: () => void;
  onDone: () => void;
}) {
  const createProject = useWorkspaceStore((state) => state.createProject);
  const [name, setName] = useState("");
  const [location, setLocation] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const canSubmit = name.trim().length > 0 && location !== null && !busy;

  async function chooseLocation() {
    try {
      const chosen = await pickFolder({ directory: true, multiple: false });
      if (typeof chosen === "string") setLocation(chosen);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit || location === null) return;
    setBusy(true);
    setError(null);
    try {
      await createProject(location, name);
      onDone();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>{copy.addProject.create}</DialogTitle>
        <DialogDescription>{copy.addProject.createSummary}</DialogDescription>
      </DialogHeader>

      <div className="grid gap-1.5">
        <Label htmlFor="project-name">{copy.addProject.nameLabel}</Label>
        <Input
          id="project-name"
          value={name}
          autoComplete="off"
          placeholder={copy.addProject.namePlaceholder}
          aria-invalid={error !== null}
          onChange={(event) => {
            setName(event.target.value);
            setError(null);
          }}
        />
      </div>

      <div className="grid gap-1.5">
        <Label>{copy.addProject.locationLabel}</Label>
        <div className="flex items-center gap-3">
          <p
            id="project-location"
            className={cn(
              "min-w-0 flex-1 text-ui break-all",
              location ? "text-foreground" : "text-muted-foreground",
            )}
          >
            {location ?? copy.addProject.noLocation}
          </p>
          <Button
            type="button"
            variant="outline"
            aria-describedby="project-location"
            onClick={() => void chooseLocation()}
          >
            {copy.addProject.chooseLocation}
          </Button>
        </div>
        <p className="text-meta text-muted-foreground">
          {copy.addProject.locationHint}
        </p>
      </div>

      {error && (
        <p role="alert" className="text-meta text-destructive">
          {error}
        </p>
      )}

      <DialogFooter>
        <Button type="button" variant="ghost" onClick={onBack}>
          {copy.addProject.back}
        </Button>
        <Button type="submit" disabled={!canSubmit}>
          {copy.addProject.submit}
        </Button>
      </DialogFooter>
    </form>
  );
}
