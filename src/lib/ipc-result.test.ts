import { describe, expect, it } from "vitest";
import { IpcRequestError, unwrap } from "@/lib/ipc-result";

describe("unwrap", () => {
  it("returns the data of an ok result", async () => {
    await expect(
      unwrap(Promise.resolve({ status: "ok" as const, data: 42 })),
    ).resolves.toBe(42);
  });

  it("throws an IpcRequestError carrying the kind and message", async () => {
    const failure = unwrap(
      Promise.resolve({
        status: "error" as const,
        error: {
          kind: "repoNotFound" as const,
          message: "repository not found: x",
        },
      }),
    );

    await expect(failure).rejects.toBeInstanceOf(IpcRequestError);
    await expect(failure).rejects.toMatchObject({
      kind: "repoNotFound",
      message: "repository not found: x",
    });
  });
});
