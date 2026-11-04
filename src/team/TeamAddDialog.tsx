import { useState, type FormEvent } from "react";

import type { HarnessId } from "@/bindings";
import { TERMINAL_PRESETS } from "@/agents/agent-presets";
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
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  useTeamMembers,
  useTeamStore,
  type MemberDraft,
} from "@/team/team-store";
import { copy } from "@/ui-copy";

const AGENTS = TERMINAL_PRESETS.flatMap((preset) =>
  preset.harness ? [{ harness: preset.harness, label: preset.label }] : [],
);
const text = copy.team.add;

/**
 * Adds a member to the team of the worktree on the sidebar. An agent gets its role, its agent and whether it
 * orchestrates; it answers to no one until an orchestrator takes it, from its menu. A bot is picked from the Bots
 * list: it only orchestrates.
 */
export function TeamAddDialog() {
  const open = useTeamStore((state) => state.addOpen);
  const members = useTeamMembers();
  const bots = useBotsStore((state) => state.bots);
  // A bot joins a worktree's team once, so the bots already in this team are not offered.
  const inTeam = new Set(
    members.flatMap((member) => (member.kind === "bot" ? [member.botId] : [])),
  );
  const freeBots = (bots ?? []).filter((bot) => !inTeam.has(bot.id));

  const [kind, setKind] = useState<"agent" | "bot">("agent");
  const [pickedBot, setPickedBot] = useState<string | null>(null);
  const [role, setRole] = useState("");
  const [harness, setHarness] = useState<HarnessId>(AGENTS[0].harness);
  const [orchestrator, setOrchestrator] = useState(false);

  // Defaults to the first bot that can still join the team.
  const bot = freeBots.find((item) => item.id === pickedBot) ?? freeBots[0];

  // A refused member keeps what was typed, so it can be fixed; the store closes the dialog once one is added.
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (kind === "bot") {
      if (!bot) return;
      const added = await useTeamStore.getState().addMember({
        kind: "bot",
        botId: bot.id,
        role: bot.name,
      });
      if (added) setPickedBot(null);
      return;
    }
    if (!role.trim()) return;
    const draft: MemberDraft = {
      kind: "agent",
      role: role.trim(),
      harness,
      parentId: null,
      orchestrator,
    };
    const added = await useTeamStore.getState().addMember(draft);
    if (!added) return;
    setRole("");
    setOrchestrator(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => useTeamStore.getState().setAddOpen(next)}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>{text.description}</DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="grid gap-4">
          <div className="grid gap-1.5">
            <Label htmlFor="team-add-kind">{text.kind}</Label>
            <Select
              value={kind}
              onValueChange={(value) => setKind(value as "agent" | "bot")}
            >
              <SelectTrigger id="team-add-kind" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="agent">{text.agent}</SelectItem>
                <SelectItem value="bot">{text.bot}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {kind === "agent" && (
            <>
              <div className="grid gap-1.5">
                <Label htmlFor="team-add-role">{text.role}</Label>
                <Input
                  id="team-add-role"
                  value={role}
                  placeholder={text.rolePlaceholder}
                  onChange={(event) => setRole(event.target.value)}
                  autoFocus
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="team-add-agent">{text.agent}</Label>
                <Select
                  value={harness}
                  onValueChange={(value) => setHarness(value as HarnessId)}
                >
                  <SelectTrigger id="team-add-agent" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {AGENTS.map((agent) => (
                      <SelectItem key={agent.harness} value={agent.harness}>
                        {agent.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex items-center justify-between">
                <Label htmlFor="team-add-orchestrator">
                  {copy.team.orchestrator}
                </Label>
                <Switch
                  id="team-add-orchestrator"
                  checked={orchestrator}
                  onCheckedChange={setOrchestrator}
                />
              </div>
            </>
          )}
          {kind === "bot" && (
            <>
              <div className="grid gap-1.5">
                <Label htmlFor="team-add-bot">{text.bot}</Label>
                {bot ? (
                  <Select value={bot.id} onValueChange={setPickedBot}>
                    <SelectTrigger id="team-add-bot" className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {freeBots.map((item) => (
                        <SelectItem key={item.id} value={item.id}>
                          {item.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <p className="text-nested text-muted-foreground">
                    {text.noBots}
                  </p>
                )}
              </div>
              {/* A bot is always an orchestrator, so the switch shows it on and cannot change. */}
              <div className="flex items-center justify-between">
                <Label htmlFor="team-add-orchestrator">
                  {copy.team.orchestrator}
                </Label>
                <Switch id="team-add-orchestrator" checked disabled />
              </div>
            </>
          )}
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => useTeamStore.getState().setAddOpen(false)}
            >
              {text.cancel}
            </Button>
            <Button
              type="submit"
              disabled={kind === "bot" ? !bot : !role.trim()}
            >
              {text.submit}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
