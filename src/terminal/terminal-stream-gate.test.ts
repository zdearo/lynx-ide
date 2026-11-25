import { describe, expect, it } from "vitest";
import {
  REPLAY_SLICE,
  TerminalStreamGate,
} from "@/terminal/terminal-stream-gate";

function setup() {
  const written: Array<string | Uint8Array> = [];
  const exits: Array<number | null> = [];
  const gate = new TerminalStreamGate(
    (data) => written.push(data),
    (code) => exits.push(code),
  );
  return { gate, written, exits };
}

const snapshot = (data: string, lastSeq: number, running = true) => ({
  data,
  lastSeq,
  running,
  exitCode: null,
});

describe("TerminalStreamGate", () => {
  it("holds events back until the snapshot is applied, then replays them in order", () => {
    const { gate, written } = setup();
    gate.pushOutput({ seq: 3, data: "C" });
    gate.pushOutput({ seq: 4, data: "D" });

    expect(written).toEqual([]);
    gate.attach(snapshot("AB", 2));

    expect(written).toEqual(["AB", "C", "D"]);
  });

  it("skips chunks the snapshot already contains", () => {
    const { gate, written } = setup();
    gate.pushOutput({ seq: 1, data: "A" });
    gate.pushOutput({ seq: 2, data: "B" });
    gate.pushOutput({ seq: 3, data: "C" });

    gate.attach(snapshot("AB", 2));

    expect(written).toEqual(["AB", "C"]);
  });

  it("writes live chunks straight through after attaching and ignores repeats", () => {
    const { gate, written } = setup();
    gate.attach(snapshot("", 0));

    gate.pushOutput({ seq: 1, data: "A" });
    gate.pushOutput({ seq: 1, data: "A" });
    gate.pushOutput({ seq: 2, data: "B" });

    expect(written).toEqual(["A", "B"]);
  });

  it("does not write an empty snapshot", () => {
    const { gate, written } = setup();
    gate.attach(snapshot("", 0));
    expect(written).toEqual([]);
  });

  it("reports an exit that happened before attaching, once", () => {
    const { gate, exits } = setup();
    gate.pushExit(7);

    gate.attach({ data: "bye", lastSeq: 1, running: false, exitCode: 7 });

    expect(exits).toEqual([7]);
  });

  it("reports an exit seen only in the snapshot", () => {
    const { gate, exits } = setup();
    gate.attach({ data: "", lastSeq: 0, running: false, exitCode: 0 });
    expect(exits).toEqual([0]);
  });

  it("reports an exit that arrives while attached, after the output before it", () => {
    const { gate, written, exits } = setup();
    gate.attach(snapshot("", 0));

    gate.pushOutput({ seq: 1, data: "A" });
    gate.pushExit(null);
    gate.pushExit(null);

    expect(written).toEqual(["A"]);
    expect(exits).toEqual([null]);
  });

  it("ignores a second attach", () => {
    const { gate, written } = setup();
    gate.attach(snapshot("A", 1));
    gate.attach(snapshot("A", 1));
    expect(written).toEqual(["A"]);
  });

  it("passes output bytes through untouched and skips a repeat by its sequence number", () => {
    const { gate, written } = setup();
    gate.attach(snapshot("old", 3));

    const bytes = new TextEncoder().encode("ção");
    gate.pushOutput({ seq: 3, data: new TextEncoder().encode("repeat") });
    gate.pushOutput({ seq: 4, data: bytes });

    expect(written).toEqual(["old", bytes]);
  });

  it("writes a long replay in pieces, one per step, and holds live output until the last piece", () => {
    const written: string[] = [];
    const steps: Array<() => void> = [];
    const doneCallbacks: Array<() => void> = [];
    let replayed = 0;
    const gate = new TerminalStreamGate(
      (data, done) => {
        written.push(data as string);
        if (done) doneCallbacks.push(done);
      },
      () => {},
      (step) => {
        steps.push(step);
        return () => {};
      },
    );
    const data = "a".repeat(REPLAY_SLICE * 2 + 10);
    gate.attach(snapshot(data, 4), () => (replayed += 1));
    gate.pushOutput({ seq: 5, data: "Z" });

    expect(written).toEqual([]);
    while (steps.length > 0) steps.shift()?.();
    expect(written.map((piece) => piece.length)).toEqual([
      REPLAY_SLICE,
      REPLAY_SLICE,
      10,
      1,
    ]);
    expect(written.join("")).toBe(data + "Z");
    expect(replayed).toBe(0);
    doneCallbacks.forEach((done) => done());
    expect(replayed).toBe(1);
  });

  it("never cuts a surrogate pair between two pieces", () => {
    const written: string[] = [];
    const steps: Array<() => void> = [];
    const gate = new TerminalStreamGate(
      (data) => written.push(data as string),
      () => {},
      (step) => {
        steps.push(step);
        return () => {};
      },
    );
    const data = "a".repeat(REPLAY_SLICE - 1) + "😀" + "b";
    gate.attach(snapshot(data, 1));
    while (steps.length > 0) steps.shift()?.();

    expect(written[0]).toHaveLength(REPLAY_SLICE - 1);
    expect(written.join("")).toBe(data);
  });
});
