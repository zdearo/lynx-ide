import { describe, expect, it } from "vitest";

import { PUSH_GAP, pushApart, type PushItem } from "@/team/team-push";

const item = (
  id: string,
  x: number,
  y: number,
  width: number,
  height: number,
): PushItem => ({ id, corner: { x, y }, size: { width, height } });

describe("pushApart", () => {
  it("leaves a layout with no overlap where it is", () => {
    const placed = pushApart([
      item("a", 0, 0, 224, 112),
      item("b", 256, 0, 224, 112),
    ]);
    expect(placed).toEqual(
      new Map([
        ["a", { x: 0, y: 0 }],
        ["b", { x: 256, y: 0 }],
      ]),
    );
  });

  it("pushes a neighbour sideways out of an open terminal", () => {
    const placed = pushApart([
      item("terminal", 0, 0, 480, 300),
      item("next", 256, 0, 224, 112),
    ]);
    const terminal = placed.get("terminal")!;
    const next = placed.get("next")!;
    // The row is centred on where it was, so the pair keeps its gap and moves as one.
    expect(next.x - terminal.x).toBe(480 + PUSH_GAP);
    expect(next.y).toBe(terminal.y);
  });

  it("pushes a card below a terminal under it", () => {
    const placed = pushApart([
      item("terminal", 0, 0, 480, 300),
      item("below", 0, 176, 224, 112),
    ]);
    expect(placed.get("below")).toEqual({ x: 0, y: 300 + PUSH_GAP });
  });

  it("leaves no two nodes overlapping, however many are packed together", () => {
    const items = Array.from({ length: 12 }, (_, i) =>
      item(`n${i}`, (i % 4) * 256, Math.floor(i / 4) * 176, 480, 300),
    );
    const placed = pushApart(items);
    const boxes = items.map((node) => ({
      ...placed.get(node.id)!,
      w: 480,
      h: 300,
    }));
    for (let i = 0; i < boxes.length; i++) {
      for (let j = 0; j < i; j++) {
        const a = boxes[i];
        const b = boxes[j];
        const overlaps =
          a.x < b.x + b.w + PUSH_GAP &&
          b.x < a.x + a.w + PUSH_GAP &&
          a.y < b.y + b.h + PUSH_GAP &&
          b.y < a.y + a.h + PUSH_GAP;
        expect(overlaps).toBe(false);
      }
    }
  });

  it("settles a card wedged between two terminals in one row, instead of swapping it between them", () => {
    const items = [
      item("left", 0, 176, 480, 300),
      item("right", 256, 176, 480, 300),
      item("between", 128, 176, 224, 112),
    ];
    const placed = pushApart(items);
    const boxes = items.map((node) => ({
      ...placed.get(node.id)!,
      w: node.size.width,
      h: node.size.height,
    }));
    for (let i = 0; i < boxes.length; i++) {
      for (let j = 0; j < i; j++) {
        const a = boxes[i];
        const b = boxes[j];
        const overlaps =
          a.x < b.x + b.w + PUSH_GAP &&
          b.x < a.x + a.w + PUSH_GAP &&
          a.y < b.y + b.h + PUSH_GAP &&
          b.y < a.y + a.h + PUSH_GAP;
        expect(overlaps).toBe(false);
      }
    }
    expect(placed.get("left")).toEqual({ x: -248, y: 176 });
  });
});
