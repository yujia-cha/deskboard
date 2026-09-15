import { describe, expect, it } from "vitest";
import { findDropTarget, toLogical, type DropCandidate } from "./dropTarget";

describe("findDropTarget", () => {
  const candidates: DropCandidate[] = [
    { instanceId: "a", rect: { x: 0, y: 0, w: 80, h: 80 } },
    { instanceId: "b", rect: { x: 200, y: 200, w: 80, h: 80 }, popupRect: { x: 200, y: 280, w: 240, h: 200 } },
  ];

  it("hits the icon rect", () => {
    expect(findDropTarget({ x: 10, y: 10 }, candidates)).toBe("a");
  });

  it("prefers an open popup over overlapping icon rects", () => {
    expect(findDropTarget({ x: 210, y: 300 }, candidates)).toBe("b");
  });

  it("returns null when nothing is hit", () => {
    expect(findDropTarget({ x: 999, y: 999 }, candidates)).toBeNull();
  });

  it("still matches the icon rect when no popup is open", () => {
    expect(findDropTarget({ x: 210, y: 210 }, candidates)).toBe("b");
  });
});

describe("toLogical", () => {
  it("divides physical position by the scale factor", () => {
    expect(toLogical({ x: 300, y: 150 }, 1.5)).toEqual({ x: 200, y: 100 });
  });

  it("is a no-op at scale factor 1", () => {
    expect(toLogical({ x: 42, y: 7 }, 1)).toEqual({ x: 42, y: 7 });
  });
});
