import { readChannel } from "./channel-store";
import { validateManifest } from "./release-manifest";
import { decideUpdate } from "./update-decision";
import { parseUpdateRequest, type UpdateRequest } from "./update-request";

export interface Env {
  RELEASES: R2Bucket;
  // Hosts that a manifest's download URLs may use, comma-separated.
  DOWNLOAD_HOSTS: string;
}

// Five minutes: a promotion reaches installed apps within that window.
const CACHE_CONTROL = "public, max-age=300";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method !== "GET") {
      return new Response(null, { status: 405, headers: { Allow: "GET" } });
    }

    const parsed = parseUpdateRequest(new URL(request.url));
    if (!parsed.ok) {
      log({ outcome: "bad-request", reason: parsed.reason });
      return new Response(null, { status: 400 });
    }
    const update = parsed.value;

    const channel = await readChannel(env.RELEASES, update.channel);
    if (!channel.ok) return unavailable(update, channel.reason);

    const hosts = env.DOWNLOAD_HOSTS.split(",")
      .map((host) => host.trim())
      .filter((host) => host !== "");
    const manifest = validateManifest(channel.raw, hosts);
    if (!manifest.ok) return unavailable(update, manifest.reason);

    const decision = decideUpdate(update, manifest.value);
    if (decision.kind === "none") {
      log({ ...summary(update), outcome: decision.reason });
      return new Response(null, {
        status: 204,
        headers: { "Cache-Control": CACHE_CONTROL },
      });
    }

    log({
      ...summary(update),
      outcome: "offer",
      offered: decision.offer.version,
    });
    return Response.json(decision.offer, {
      headers: { "Cache-Control": CACHE_CONTROL },
    });
  },
} satisfies ExportedHandler<Env>;

function unavailable(update: UpdateRequest, reason: string): Response {
  log({ ...summary(update), outcome: "unavailable", reason });
  // Errors are not cached, so the next check reads the channel again.
  return new Response(null, { status: 503, headers: { "Retry-After": "300" } });
}

// Only what the updater sent. No IP and no install ID, because there is none.
function summary(update: UpdateRequest): Record<string, string> {
  return {
    target: update.target,
    arch: update.arch,
    channel: update.channel,
    current: update.currentVersion,
  };
}

function log(fields: Record<string, string>): void {
  console.log(JSON.stringify(fields));
}
