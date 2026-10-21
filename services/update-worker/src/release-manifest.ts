import { parseVersion } from "./version-compare";

export const RELEASE_STATUSES = ["available", "held", "withdrawn"] as const;
export type ReleaseStatus = (typeof RELEASE_STATUSES)[number];

export interface PlatformArtifact {
  url: string;
  signature: string;
  sha256: string;
}

export interface ReleaseManifest {
  version: string;
  pub_date: string;
  notes?: string;
  status: ReleaseStatus;
  // Keyed by `${target}-${arch}`, the names the updater asks for.
  platforms: Record<string, PlatformArtifact>;
}

type Checked<T> = { ok: true; value: T } | { ok: false; reason: string };

const PLATFORM_KEY = /^(linux|windows|darwin)-(x86_64|aarch64)$/;
const SHA256 = /^[0-9a-f]{64}$/;

// Runs on every read, not only in CI, so a broken manifest is never served.
export function validateManifest(
  raw: unknown,
  allowedHosts: readonly string[],
): Checked<ReleaseManifest> {
  if (!isRecord(raw)) return fail("not an object");

  const { version, pub_date, notes, status, platforms } = raw;
  if (typeof version !== "string" || parseVersion(version) === null) {
    return fail("version");
  }
  if (typeof pub_date !== "string" || Number.isNaN(Date.parse(pub_date))) {
    return fail("pub_date");
  }
  if (notes !== undefined && typeof notes !== "string") return fail("notes");
  if (!isReleaseStatus(status)) return fail("status");
  if (!isRecord(platforms) || Object.keys(platforms).length === 0) {
    return fail("platforms");
  }

  const checked: Record<string, PlatformArtifact> = {};
  for (const [key, artifact] of Object.entries(platforms)) {
    if (!PLATFORM_KEY.test(key)) return fail(`platform key ${key}`);
    const result = parseArtifact(artifact, allowedHosts);
    if (!result.ok) return fail(`${key}: ${result.reason}`);
    checked[key] = result.value;
  }

  return {
    ok: true,
    value: { version, pub_date, notes, status, platforms: checked },
  };
}

function parseArtifact(
  raw: unknown,
  allowedHosts: readonly string[],
): Checked<PlatformArtifact> {
  if (!isRecord(raw)) return fail("not an object");

  const { url, signature, sha256 } = raw;
  if (typeof url !== "string" || !isAllowedUrl(url, allowedHosts)) {
    return fail("url");
  }
  if (typeof signature !== "string" || signature.trim() === "") {
    return fail("signature");
  }
  if (typeof sha256 !== "string" || !SHA256.test(sha256)) {
    return fail("sha256");
  }

  return { ok: true, value: { url, signature, sha256 } };
}

// Download URLs must be https and point at a host we publish from.
function isAllowedUrl(text: string, allowedHosts: readonly string[]): boolean {
  try {
    const url = new URL(text);
    return url.protocol === "https:" && allowedHosts.includes(url.hostname);
  } catch {
    return false;
  }
}

function isReleaseStatus(value: unknown): value is ReleaseStatus {
  return (
    typeof value === "string" &&
    (RELEASE_STATUSES as readonly string[]).includes(value)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function fail(reason: string): { ok: false; reason: string } {
  return { ok: false, reason };
}
