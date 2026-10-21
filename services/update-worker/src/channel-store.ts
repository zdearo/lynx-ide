import type { Channel } from "./update-request";

export type ChannelRead =
  { ok: true; raw: unknown } | { ok: false; reason: string };

// A channel is a pointer: CI rewrites channels/<name>.json with the manifest
// of the version being promoted.
export async function readChannel(
  bucket: R2Bucket,
  channel: Channel,
): Promise<ChannelRead> {
  try {
    const object = await bucket.get(`channels/${channel}.json`);
    if (object === null) return { ok: false, reason: "channel missing" };
    return { ok: true, raw: await object.json<unknown>() };
  } catch {
    return { ok: false, reason: "channel unreadable" };
  }
}
