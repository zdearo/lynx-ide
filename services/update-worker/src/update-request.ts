import { parseVersion } from "./version-compare";

const TARGETS = ["linux", "windows", "darwin"] as const;
const ARCHES = ["x86_64", "aarch64"] as const;
// Only stable for now. A new channel also needs its channels/<name>.json.
const CHANNELS = ["stable"] as const;

export type Target = (typeof TARGETS)[number];
export type Arch = (typeof ARCHES)[number];
export type Channel = (typeof CHANNELS)[number];

export interface UpdateRequest {
  target: Target;
  arch: Arch;
  currentVersion: string;
  channel: Channel;
}

export type ParsedRequest =
  { ok: true; value: UpdateRequest } | { ok: false; reason: string };

// The updater fills the placeholders in:
// /api/update/{{target}}/{{arch}}/{{current_version}}?channel=stable
export function parseUpdateRequest(url: URL): ParsedRequest {
  const parts = url.pathname.split("/");
  if (parts.length !== 6 || parts[1] !== "api" || parts[2] !== "update") {
    return { ok: false, reason: "path" };
  }

  const [, , , target, arch, currentVersion] = parts;
  if (!isOneOf(TARGETS, target)) return { ok: false, reason: "target" };
  if (!isOneOf(ARCHES, arch)) return { ok: false, reason: "arch" };
  if (parseVersion(currentVersion) === null) {
    return { ok: false, reason: "version" };
  }

  const channel = url.searchParams.get("channel") ?? "stable";
  if (!isOneOf(CHANNELS, channel)) return { ok: false, reason: "channel" };

  return { ok: true, value: { target, arch, currentVersion, channel } };
}

function isOneOf<T extends string>(
  values: readonly T[],
  value: string,
): value is T {
  return (values as readonly string[]).includes(value);
}
