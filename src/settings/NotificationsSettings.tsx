import { SwitchSetting } from "@/settings/SettingFields";
import { useSettings } from "@/settings/use-settings";
import { copy } from "@/ui-copy";

export function NotificationsSettings() {
  const { config, disabled, save } = useSettings();
  if (!config) return null;
  const notifications = config.notifications;

  return (
    <>
      <p className="text-nested text-muted-foreground">
        {copy.settings.notifications.intro}
      </p>
      <SwitchSetting
        id="notifications-done"
        label={copy.settings.notifications.done.label}
        hint={copy.settings.notifications.done.hint}
        checked={notifications.onDone}
        disabled={disabled}
        onCommit={(onDone) =>
          save({ ...config, notifications: { ...notifications, onDone } })
        }
      />
      <SwitchSetting
        id="notifications-needs-you"
        label={copy.settings.notifications.needsYou.label}
        hint={copy.settings.notifications.needsYou.hint}
        checked={notifications.onNeedsYou}
        disabled={disabled}
        onCommit={(onNeedsYou) =>
          save({ ...config, notifications: { ...notifications, onNeedsYou } })
        }
      />
    </>
  );
}
