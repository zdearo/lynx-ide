import type { ReleaseManifest } from "./release-manifest";
import type { UpdateRequest } from "./update-request";
import { compareVersions, parseVersion } from "./version-compare";

// The body of a 200, in the shape the Tauri updater reads.
export interface UpdateOffer {
  version: string;
  url: string;
  signature: string;
  notes?: string;
  pub_date: string;
}

export type Decision =
  | { kind: "offer"; offer: UpdateOffer }
  | {
      kind: "none";
      reason: "held" | "withdrawn" | "up-to-date" | "no-artifact";
    };

export function decideUpdate(
  request: UpdateRequest,
  manifest: ReleaseManifest,
): Decision {
  if (manifest.status !== "available") {
    return { kind: "none", reason: manifest.status };
  }

  // Both versions were validated before this point.
  const latest = parseVersion(manifest.version);
  const current = parseVersion(request.currentVersion);
  if (
    latest === null ||
    current === null ||
    compareVersions(latest, current) <= 0
  ) {
    return { kind: "none", reason: "up-to-date" };
  }

  const artifact = manifest.platforms[`${request.target}-${request.arch}`];
  if (artifact === undefined) return { kind: "none", reason: "no-artifact" };

  return {
    kind: "offer",
    offer: {
      version: manifest.version,
      url: artifact.url,
      signature: artifact.signature,
      notes: manifest.notes,
      pub_date: manifest.pub_date,
    },
  };
}
