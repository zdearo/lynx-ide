import { useEffect, useState } from "react";

import { commands } from "@/bindings";
import { unwrap } from "@/lib/ipc-result";

/** Shells found on this machine. Empty while loading, or when the list cannot be read. */
export function useInstalledShells(): string[] {
  const [shells, setShells] = useState<string[]>([]);

  useEffect(() => {
    let cancelled = false;
    void unwrap(commands.listShells()).then(
      (list) => {
        if (!cancelled) setShells(list);
      },
      // Without the list the field still offers the login shell and a custom path.
      () => undefined,
    );
    return () => {
      cancelled = true;
    };
  }, []);

  return shells;
}
