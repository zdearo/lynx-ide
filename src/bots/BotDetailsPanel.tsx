import { useEffect, useState } from "react";
import { XmarkGlyph18 } from "@/components/icons/XmarkGlyph18";
import { toast } from "sonner";

import { commands, type BotDetail, type Grant } from "@/bindings";
import { BotAvatar } from "@/bots/BotAvatar";
import { BotCommandsSection } from "@/bots/BotCommandsSection";
import { BotMcpSection } from "@/bots/BotMcpSection";
import { BotSkillsSection } from "@/bots/BotSkillsSection";
import type { BotProvider } from "@/bots/bot-colors";
import { botAnswering, useBotChatStore } from "@/bots/bot-chat-store";
import { BotDeleteImpact } from "@/bots/BotDeleteImpact";
import { ChannelDetails } from "@/bots/ChannelDetails";
import { DetailRow, DetailSection } from "@/bots/DetailSection";
import { EnginePicker } from "@/bots/EnginePicker";
import {
  selectedBotOf,
  selectedChannelOf,
  useBotsStore,
} from "@/bots/bots-store";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

// The same limit the backend enforces on instructions.
const MAX_INSTRUCTIONS = 8000;

/** The right panel's details tab: the picked channel's members, or the picked bot's setup, instructions and folder. */
export function BotDetailsPanel() {
  const channel = useBotsStore(selectedChannelOf);
  if (channel) return <ChannelDetails channel={channel} />;
  return <PickedBotDetails />;
}

/** The repositories a bot may read, with a way to take each one back. */
function BotAccess({ botId }: { botId: string }) {
  const [grants, setGrants] = useState<Grant[] | null>(null);
  const text = copy.bots.access;

  useEffect(() => {
    let current = true;
    unwrap(commands.botsGrants(botId))
      .then((found) => {
        if (current) setGrants(found);
      })
      .catch((error: unknown) =>
        toast.error(text.revokeFailed, { description: errorMessage(error) }),
      );
    return () => {
      current = false;
    };
  }, [botId, text.revokeFailed]);

  async function revoke(repoId: string) {
    try {
      setGrants(await unwrap(commands.botsRevokeAccess(botId, repoId)));
    } catch (error) {
      toast.error(text.revokeFailed, { description: errorMessage(error) });
    }
  }

  return (
    <DetailSection title={text.granted}>
      {grants !== null &&
        (grants.length === 0 ? (
          <p className="text-ui text-muted-foreground">{text.none}</p>
        ) : (
          <ul
            role="list"
            aria-label={text.granted}
            className="grid grid-cols-1 gap-1"
          >
            {grants.map((grant) => (
              <DetailRow
                key={grant.id}
                title={grant.name}
                action={
                  <TooltipIconButton
                    label={text.revokeLabel(grant.name)}
                    onClick={() => void revoke(grant.id)}
                  >
                    <XmarkGlyph18 />
                  </TooltipIconButton>
                }
              />
            ))}
          </ul>
        ))}
    </DetailSection>
  );
}

/** The repositories where a bot may start agents, with a way to take each one back. */
function BotWorkerAccess({ botId }: { botId: string }) {
  const [grants, setGrants] = useState<Grant[] | null>(null);
  const text = copy.bots.access;

  useEffect(() => {
    let current = true;
    unwrap(commands.botsWorkerGrants(botId))
      .then((found) => {
        if (current) setGrants(found);
      })
      .catch((error: unknown) =>
        toast.error(text.revokeFailed, { description: errorMessage(error) }),
      );
    return () => {
      current = false;
    };
  }, [botId, text.revokeFailed]);

  async function revoke(repoId: string) {
    try {
      setGrants(await unwrap(commands.botsRevokeWorkers(botId, repoId)));
    } catch (error) {
      toast.error(text.revokeFailed, { description: errorMessage(error) });
    }
  }

  return (
    <DetailSection title={text.workersTitle} hint={text.workersHint}>
      {grants !== null &&
        (grants.length === 0 ? (
          <p className="text-ui text-muted-foreground">{text.workersNone}</p>
        ) : (
          <ul
            role="list"
            aria-label={text.workersTitle}
            className="grid grid-cols-1 gap-1"
          >
            {grants.map((grant) => (
              <DetailRow
                key={grant.id}
                title={grant.name}
                action={
                  <TooltipIconButton
                    label={text.revokeWorkersLabel(grant.name)}
                    onClick={() => void revoke(grant.id)}
                  >
                    <XmarkGlyph18 />
                  </TooltipIconButton>
                }
              />
            ))}
          </ul>
        ))}
    </DetailSection>
  );
}

function PickedBotDetails() {
  const bot = useBotsStore(selectedBotOf);
  const botId = bot?.id ?? null;
  // The detail is kept with the id it belongs to, so the panel never shows another bot's detail.
  const [loaded, setLoaded] = useState<{
    id: string;
    detail: BotDetail;
  } | null>(null);
  const detail = loaded && loaded.id === botId ? loaded.detail : null;
  // A bot that is answering cannot be deleted, so the panel says so instead of failing.
  const answering = useBotChatStore((state) =>
    botId ? botAnswering(state, botId) : false,
  );
  const text = copy.bots.details;

  useEffect(() => {
    if (botId === null) return;
    let current = true;
    unwrap(commands.botsGet(botId))
      .then((found) => {
        if (current) setLoaded({ id: botId, detail: found });
      })
      .catch((error: unknown) => {
        if (current)
          toast.error(text.loadFailed, { description: errorMessage(error) });
      });
    return () => {
      current = false;
    };
  }, [botId, text.loadFailed]);

  if (!bot) {
    return (
      <p className="p-4 text-center text-ui text-muted-foreground">
        {copy.bots.chat.pickBot}
      </p>
    );
  }
  if (!detail) return null;

  return (
    <BotDetails
      key={detail.id}
      detail={detail}
      answering={answering}
      onSaved={(saved) => setLoaded({ id: saved.id, detail: saved })}
    />
  );
}

function BotDetails({
  detail,
  answering,
  onSaved,
}: {
  detail: BotDetail;
  answering: boolean;
  onSaved: (saved: BotDetail) => void;
}) {
  const text = copy.bots.details;
  const wizard = copy.bots.wizard;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [asking, setAsking] = useState(false);
  const [editingSetup, setEditingSetup] = useState(false);
  const [provider, setProvider] = useState<BotProvider>(
    detail.provider as BotProvider,
  );
  const [model, setModel] = useState("");
  const [team, setTeam] = useState("");
  const [savingSetup, setSavingSetup] = useState(false);

  function startEditing() {
    setDraft(detail.instructions.trim());
    setEditing(true);
  }

  function startEditingSetup() {
    setProvider(detail.provider as BotProvider);
    setModel(detail.model ?? "");
    setTeam(detail.team ?? "");
    setEditingSetup(true);
  }

  async function save() {
    const instructions = draft.trim();
    if (!instructions || saving) return;
    setSaving(true);
    try {
      const saved = await unwrap(
        commands.botsUpdateInstructions(detail.id, instructions),
      );
      onSaved(saved);
      setEditing(false);
    } catch (error) {
      toast.error(text.saveFailed, { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  }

  async function saveSetup() {
    if (savingSetup) return;
    setSavingSetup(true);
    try {
      const saved = await useBotsStore.getState().updateSetup(detail.id, {
        provider,
        model: model.trim() || null,
        team: team.trim() || null,
      });
      onSaved(saved);
      setEditingSetup(false);
    } catch (error) {
      toast.error(text.saveSetupFailed, { description: errorMessage(error) });
    } finally {
      setSavingSetup(false);
    }
  }

  async function remove() {
    try {
      await useBotsStore.getState().remove(detail.id);
    } catch (error) {
      toast.error(text.deleteFailed, { description: errorMessage(error) });
    }
  }

  const row = "grid grid-cols-[4.5rem_minmax(0,1fr)] items-baseline gap-x-3";
  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto p-4">
      <div className="flex items-center gap-3">
        <BotAvatar name={detail.name} color={detail.color} size="lg" />
        <div className="min-w-0">
          <p className="truncate text-ui font-medium">{detail.name}</p>
          <p className="truncate text-meta text-muted-foreground">
            {copy.bots.providers[detail.provider as BotProvider] ??
              detail.provider}
          </p>
        </div>
      </div>

      <DetailSection
        title={text.setup}
        actions={
          !editingSetup && (
            <Button
              variant="ghost"
              size="xs"
              aria-label={text.editSetup}
              onClick={startEditingSetup}
            >
              {text.edit}
            </Button>
          )
        }
      >
        {editingSetup ? (
          <div className="grid gap-3">
            <EnginePicker value={provider} onChange={setProvider} />
            <label className="grid gap-1.5 text-meta text-muted-foreground">
              {wizard.engine.model}
              <Input
                value={model}
                maxLength={80}
                placeholder={wizard.engine.modelPlaceholder}
                onChange={(event) => setModel(event.target.value)}
              />
            </label>
            <label className="grid gap-1.5 text-meta text-muted-foreground">
              {wizard.name.team}
              <Input
                value={team}
                maxLength={24}
                placeholder={wizard.name.teamPlaceholder}
                onChange={(event) => setTeam(event.target.value)}
              />
            </label>
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setEditingSetup(false)}
              >
                {text.cancel}
              </Button>
              <Button
                size="sm"
                disabled={savingSetup}
                onClick={() => void saveSetup()}
              >
                {text.save}
              </Button>
            </div>
          </div>
        ) : (
          <dl className="grid gap-1.5 text-ui">
            <div className={row}>
              <dt className="text-meta text-muted-foreground">
                {text.provider}
              </dt>
              <dd className="truncate">
                {copy.bots.providers[detail.provider as BotProvider] ??
                  detail.provider}
              </dd>
            </div>
            <div className={row}>
              <dt className="text-meta text-muted-foreground">{text.model}</dt>
              <dd className="truncate">{detail.model ?? text.modelDefault}</dd>
            </div>
            <div className={row}>
              <dt className="text-meta text-muted-foreground">{text.team}</dt>
              <dd className="truncate">{detail.team ?? text.noTeam}</dd>
            </div>
            <div className={row}>
              <dt className="text-meta text-muted-foreground">{text.folder}</dt>
              <dd
                title={detail.folder}
                className="truncate font-mono text-meta"
              >
                {detail.folder}
              </dd>
            </div>
          </dl>
        )}
      </DetailSection>

      <DetailSection
        title={text.instructions}
        actions={
          !editing && (
            <Button variant="ghost" size="xs" onClick={startEditing}>
              {text.edit}
            </Button>
          )
        }
      >
        {editing ? (
          <div className="grid gap-2">
            <Textarea
              value={draft}
              rows={8}
              maxLength={MAX_INSTRUCTIONS}
              aria-label={text.instructions}
              autoFocus
              onChange={(event) => setDraft(event.target.value)}
            />
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setEditing(false)}
              >
                {text.cancel}
              </Button>
              <Button
                size="sm"
                disabled={!draft.trim() || saving}
                onClick={() => void save()}
              >
                {text.save}
              </Button>
            </div>
          </div>
        ) : (
          <p className="line-clamp-6 text-ui break-words whitespace-pre-wrap">
            {detail.instructions.trim()}
          </p>
        )}
      </DetailSection>

      <BotAccess botId={detail.id} />

      <BotWorkerAccess botId={detail.id} />

      <BotCommandsSection botId={detail.id} />

      <BotSkillsSection botId={detail.id} />

      <BotMcpSection botId={detail.id} />

      <div className="mt-auto grid gap-2 border-t border-border pt-4">
        {answering && (
          <p className="text-meta text-muted-foreground">{text.deleteBusy}</p>
        )}
        <Button
          variant="destructive"
          size="sm"
          disabled={answering}
          onClick={() => setAsking(true)}
        >
          {text.delete}
        </Button>
      </div>

      <AlertDialog open={asking} onOpenChange={setAsking}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{text.deleteTitle(detail.name)}</AlertDialogTitle>
            <AlertDialogDescription>{text.deleteBody}</AlertDialogDescription>
            <BotDeleteImpact botId={detail.id} />
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{text.cancel}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void remove()}
            >
              {text.deleteConfirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
