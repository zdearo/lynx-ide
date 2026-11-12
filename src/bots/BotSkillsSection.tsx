import { useEffect, useState } from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { PlusIcon, XIcon } from "@phosphor-icons/react";
import { toast } from "sonner";

import { commands, type BotSkill } from "@/bindings";
import { DetailRow, DetailSection } from "@/bots/DetailSection";
import { TooltipIconButton } from "@/components/TooltipIconButton";
import { errorMessage } from "@/lib/error-message";
import { unwrap } from "@/lib/ipc-result";
import { copy } from "@/ui-copy";

/** The skills a bot has, with a way to add one from a folder and to take each one back. */
export function BotSkillsSection({ botId }: { botId: string }) {
  const [skills, setSkills] = useState<BotSkill[] | null>(null);
  const text = copy.bots.skills;

  useEffect(() => {
    let current = true;
    unwrap(commands.botsSkills(botId))
      .then((found) => {
        if (current) setSkills(found);
      })
      .catch((error: unknown) =>
        toast.error(text.loadFailed, { description: errorMessage(error) }),
      );
    return () => {
      current = false;
    };
  }, [botId, text.loadFailed]);

  async function add() {
    try {
      const chosen = await pickFolder({ directory: true, multiple: false });
      if (typeof chosen !== "string") return;
      setSkills(await unwrap(commands.botsSkillAdd(botId, chosen)));
    } catch (error) {
      toast.error(text.addFailed, { description: errorMessage(error) });
    }
  }

  async function remove(name: string) {
    try {
      setSkills(await unwrap(commands.botsSkillRemove(botId, name)));
    } catch (error) {
      toast.error(text.removeFailed, { description: errorMessage(error) });
    }
  }

  return (
    <DetailSection
      title={text.title}
      hint={text.hint}
      actions={
        <TooltipIconButton label={text.add} onClick={() => void add()}>
          <PlusIcon />
        </TooltipIconButton>
      }
    >
      {skills !== null &&
        (skills.length === 0 ? (
          <p className="text-ui text-muted-foreground">{text.none}</p>
        ) : (
          <ul
            role="list"
            aria-label={text.title}
            className="grid grid-cols-1 gap-1"
          >
            {skills.map((skill) => (
              <DetailRow
                key={skill.name}
                title={skill.name}
                subtitle={skill.description}
                action={
                  <TooltipIconButton
                    label={text.removeLabel(skill.name)}
                    onClick={() => void remove(skill.name)}
                  >
                    <XIcon />
                  </TooltipIconButton>
                }
              />
            ))}
          </ul>
        ))}
    </DetailSection>
  );
}
