import { useState, type FormEvent } from "react";

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
import { useTeamStore } from "@/team/team-store";
import { copy } from "@/ui-copy";

const text = copy.team.runs;

/** Starts a run: its goal is all it asks, and it begins with an orchestrator at the top. */
export function TeamRunDialog() {
  const open = useTeamStore((state) => state.newRunOpen);
  const [title, setTitle] = useState("");

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!title.trim()) return;
    // A refused goal keeps the text, so it can be fixed.
    if (await useTeamStore.getState().addRun(title.trim())) setTitle("");
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => useTeamStore.getState().setNewRunOpen(next)}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>{text.description}</DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="grid gap-4">
          <div className="grid gap-1.5">
            <Label htmlFor="team-run-goal">{text.goal}</Label>
            <Input
              id="team-run-goal"
              value={title}
              placeholder={text.placeholder}
              onChange={(event) => setTitle(event.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => useTeamStore.getState().setNewRunOpen(false)}
            >
              {copy.team.add.cancel}
            </Button>
            <Button type="submit" disabled={!title.trim()}>
              {text.submit}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
