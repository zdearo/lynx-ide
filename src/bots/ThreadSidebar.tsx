import { useState } from "react";
import { toast } from "sonner";

import type { BotSummary, ThreadSummary } from "@/bindings";
import { botThread, threadOf, useBotChatStore } from "@/bots/bot-chat-store";
import { shownThreadOf, useBotsStore } from "@/bots/bots-store";
import { threadAge } from "@/bots/thread-age";
import { DotsGlyph18 } from "@/components/icons/DotsGlyph18";
import { PlusGlyph18 } from "@/components/icons/PlusGlyph18";
import { TooltipIconButton } from "@/components/TooltipIconButton";
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
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { errorMessage } from "@/lib/error-message";
import { useNow } from "@/hooks/use-now";
import { cn } from "@/lib/utils";
import { copy } from "@/ui-copy";

const text = copy.bots.threads;

/** What a thread is called in the list: its title, or that it has none yet. */
const nameOf = (thread: ThreadSummary) => thread.title || text.untitled;

/**
 * The column of a bot's threads beside its chat: start one, search them, pick one, and rename or delete
 * one from the menu of its row. The newest activity comes first.
 */
export function ThreadSidebar({
  bot,
  onPicked,
  className,
}: {
  bot: BotSummary;
  /** Called after a thread is picked, so a column that opens over the chat can close. */
  onPicked?: () => void;
  className?: string;
}) {
  const botId = bot.id;
  const threads = useBotsStore((state) => state.threadsByBot[botId]);
  const shown = useBotsStore((state) => shownThreadOf(state, botId));
  const selectThread = useBotsStore((state) => state.selectThread);
  const createThread = useBotsStore((state) => state.createThread);
  const [renaming, setRenaming] = useState<ThreadSummary | null>(null);
  const [deleting, setDeleting] = useState<ThreadSummary | null>(null);
  const [query, setQuery] = useState("");
  // Ages stay true on a still screen.
  const now = Math.floor(useNow(30_000) / 1000);
  const wanted = query.trim().toLowerCase();
  const listed = (threads ?? []).filter(
    (thread) => !wanted || nameOf(thread).toLowerCase().includes(wanted),
  );

  async function create() {
    try {
      await createThread(botId);
      setQuery("");
      onPicked?.();
    } catch (error) {
      toast.error(text.createFailed, { description: errorMessage(error) });
    }
  }

  return (
    <aside
      aria-label={text.title}
      className={cn(
        "flex min-h-0 flex-col bg-background px-1.5 py-2",
        className,
      )}
    >
      <div className="mb-1.5 flex items-center gap-1 pl-1">
        <Input
          value={query}
          aria-label={text.search}
          placeholder={text.search}
          onChange={(event) => setQuery(event.target.value)}
          className="h-7 flex-1 border-transparent bg-transparent px-1.5 text-nested shadow-none focus-visible:bg-muted/40"
        />
        <TooltipIconButton
          label={text.newThread}
          className="text-muted-foreground"
          onClick={() => void create()}
        >
          <PlusGlyph18 />
        </TooltipIconButton>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {listed.length === 0 && wanted && (
          <p className="px-2 py-1 text-meta text-muted-foreground">
            {text.noMatch}
          </p>
        )}
        <ul
          role="list"
          aria-label={text.title}
          className="grid grid-cols-1 gap-0.5"
        >
          {listed.map((thread) => (
            <ThreadRow
              key={thread.id}
              botId={botId}
              thread={thread}
              age={threadAge(thread.updatedAt, now)}
              selected={thread.id === shown}
              onPick={() => {
                selectThread(botId, thread.id);
                onPicked?.();
              }}
              onRename={() => setRenaming(thread)}
              onDelete={() => setDeleting(thread)}
            />
          ))}
        </ul>
      </div>
      {renaming && (
        <RenameDialog
          botId={botId}
          thread={renaming}
          onClose={() => setRenaming(null)}
        />
      )}
      {deleting && (
        <DeleteDialog
          botId={botId}
          thread={deleting}
          onClose={() => setDeleting(null)}
        />
      )}
    </aside>
  );
}

function ThreadRow({
  botId,
  thread,
  age,
  selected,
  onPick,
  onRename,
  onDelete,
}: {
  botId: string;
  thread: ThreadSummary;
  age: string;
  selected: boolean;
  onPick: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const answering = useBotChatStore(
    (state) => threadOf(state, botThread(botId, thread.id)).thinking,
  );
  return (
    <li className="group/row relative">
      <button
        type="button"
        aria-current={selected ? "true" : undefined}
        onClick={onPick}
        className={cn(
          "flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1 text-left outline-none hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring",
          selected && "bg-muted",
        )}
      >
        <span
          className={cn(
            "min-w-0 flex-1 truncate text-nested",
            !thread.title && "text-muted-foreground",
          )}
        >
          {nameOf(thread)}
        </span>
        {answering ? (
          <>
            <span
              aria-hidden="true"
              className="size-1.5 shrink-0 animate-pulse rounded-full bg-live motion-reduce:animate-none"
            />
            <span className="sr-only">{text.answering}</span>
          </>
        ) : (
          <span className="shrink-0 text-meta text-muted-foreground tabular-nums group-focus-within/row:invisible group-hover/row:invisible">
            {age}
          </span>
        )}
      </button>
      <div className="absolute top-0.5 right-1 opacity-0 transition-opacity group-focus-within/row:opacity-100 group-hover/row:opacity-100">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <TooltipIconButton label={`${text.rename} / ${text.delete}`}>
              <DotsGlyph18 />
            </TooltipIconButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="min-w-36">
            <DropdownMenuItem onSelect={onRename}>
              {text.rename}
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              {text.delete}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </li>
  );
}

function RenameDialog({
  botId,
  thread,
  onClose,
}: {
  botId: string;
  thread: ThreadSummary;
  onClose: () => void;
}) {
  const renameThread = useBotsStore((state) => state.renameThread);
  const [title, setTitle] = useState(thread.title);
  const [saving, setSaving] = useState(false);

  async function save() {
    if (saving) return;
    setSaving(true);
    try {
      await renameThread(botId, thread.id, title);
      onClose();
    } catch (error) {
      toast.error(text.renameFailed, { description: errorMessage(error) });
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{text.renameTitle}</DialogTitle>
        </DialogHeader>
        <form
          className="grid gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <Input
            value={title}
            maxLength={60}
            aria-label={text.renameLabel}
            placeholder={text.renamePlaceholder}
            autoFocus
            onChange={(event) => setTitle(event.target.value)}
          />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {text.cancel}
            </Button>
            <Button type="submit" disabled={saving}>
              {text.renameSave}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function DeleteDialog({
  botId,
  thread,
  onClose,
}: {
  botId: string;
  thread: ThreadSummary;
  onClose: () => void;
}) {
  const deleteThread = useBotsStore((state) => state.deleteThread);

  async function remove() {
    try {
      await deleteThread(botId, thread.id);
    } catch (error) {
      toast.error(text.deleteFailed, { description: errorMessage(error) });
    }
  }

  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {text.deleteTitle(nameOf(thread))}
          </AlertDialogTitle>
          <AlertDialogDescription>{text.deleteBody}</AlertDialogDescription>
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
  );
}
