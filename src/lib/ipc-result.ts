import type { IpcError, IpcErrorKind } from "@/bindings";

type IpcResult<T> =
  { status: "ok"; data: T } | { status: "error"; error: IpcError };

export class IpcRequestError extends Error {
  readonly kind: IpcErrorKind;

  constructor(error: IpcError) {
    super(error.message);
    this.name = "IpcRequestError";
    this.kind = error.kind;
  }
}

/** Turns a generated `{ status, ... }` result into a value, or throws `IpcRequestError`. */
export async function unwrap<T>(result: Promise<IpcResult<T>>): Promise<T> {
  const settled = await result;
  if (settled.status === "ok") return settled.data;
  throw new IpcRequestError(settled.error);
}
