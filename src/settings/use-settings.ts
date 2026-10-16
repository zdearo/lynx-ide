import type { AppConfig } from "@/bindings";
import { useConfigStore } from "@/config/config-store";

/** Everything a settings panel needs: the effective config, whether it can be edited, and how to save. */
export function useSettings() {
  const snapshot = useConfigStore((state) => state.snapshot);
  const save = useConfigStore((state) => state.save);
  return {
    config: snapshot?.config ?? null,
    disabled: snapshot === null || snapshot.fileBroken,
    save: (next: AppConfig) => save(next),
  };
}
