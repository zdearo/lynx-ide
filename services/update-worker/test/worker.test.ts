import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import worker, { type Env } from "../src/index";

const HOST = "updates-staging.test";
const STABLE = "channels/stable.json";

// Stands in for R2: objects are stored as text, like the real bucket returns.
function fakeBucket(objects: Record<string, string>): R2Bucket {
  return {
    get: async (key: string) => {
      if (!(key in objects)) return null;
      const text = objects[key];
      return { json: async () => JSON.parse(text) };
    },
  } as unknown as R2Bucket;
}

function envWith(objects: Record<string, string>, hosts = HOST): Env {
  return { RELEASES: fakeBucket(objects), DOWNLOAD_HOSTS: hosts };
}

function release(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    version: "0.1.1",
    pub_date: "2026-10-21T12:00:00Z",
    notes: "Notas",
    status: "available",
    platforms: {
      "linux-x86_64": {
        url: `https://${HOST}/v0.1.1/Lynx.AppImage`,
        signature: "sig",
        sha256: "a".repeat(64),
      },
    },
    ...overrides,
  });
}

function get(path: string, env: Env, method = "GET"): Promise<Response> {
  return worker.fetch(
    new Request(`https://updates.test${path}`, { method }),
    env,
  );
}

const CURRENT = "/api/update/linux/x86_64/0.1.0?channel=stable";

describe("update worker", () => {
  beforeEach(() => {
    // The worker logs one line per request. The tests read the responses.
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("serves an offer in the updater's shape, with a short cache", async () => {
    const res = await get(CURRENT, envWith({ [STABLE]: release() }));

    expect(res.status).toBe(200);
    expect(res.headers.get("Cache-Control")).toBe("public, max-age=300");
    expect(await res.json()).toEqual({
      version: "0.1.1",
      url: `https://${HOST}/v0.1.1/Lynx.AppImage`,
      signature: "sig",
      notes: "Notas",
      pub_date: "2026-10-21T12:00:00Z",
    });
  });

  it("answers 204 when the app is current", async () => {
    const res = await get(
      "/api/update/linux/x86_64/0.1.1",
      envWith({ [STABLE]: release() }),
    );
    expect(res.status).toBe(204);
    expect(await res.text()).toBe("");
  });

  it("answers 204 for a held release, without caching a refusal", async () => {
    const res = await get(
      CURRENT,
      envWith({ [STABLE]: release({ status: "held" }) }),
    );
    expect(res.status).toBe(204);
  });

  it("answers 400 for a request it cannot parse", async () => {
    const res = await get(
      "/api/update/linux/x86_64/nope",
      envWith({ [STABLE]: release() }),
    );
    expect(res.status).toBe(400);
  });

  it("answers 405 to anything but GET", async () => {
    const res = await get(CURRENT, envWith({ [STABLE]: release() }), "POST");
    expect(res.status).toBe(405);
    expect(res.headers.get("Allow")).toBe("GET");
  });

  it("answers 503 when the channel pointer is missing", async () => {
    const res = await get(CURRENT, envWith({}));
    expect(res.status).toBe(503);
    expect(res.headers.get("Cache-Control")).toBeNull();
  });

  it("answers 503 when the channel pointer is not JSON", async () => {
    const res = await get(CURRENT, envWith({ [STABLE]: "{not json" }));
    expect(res.status).toBe(503);
  });

  it("answers 503 when the manifest fails validation", async () => {
    const elsewhere = release({
      platforms: {
        "linux-x86_64": {
          url: "https://evil.test/Lynx.AppImage",
          signature: "sig",
          sha256: "a".repeat(64),
        },
      },
    });
    const res = await get(CURRENT, envWith({ [STABLE]: elsewhere }));
    expect(res.status).toBe(503);
  });

  it("refuses everything when no download host is configured", async () => {
    const res = await get(CURRENT, envWith({ [STABLE]: release() }, ""));
    expect(res.status).toBe(503);
  });
});
