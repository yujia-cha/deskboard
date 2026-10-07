import { describe, expect, it } from "vitest";
import { computePopupRect } from "./placement";

describe("computePopupRect", () => {
  const viewport = { w: 1000, h: 800 };

  it("opens below the anchor when there is room", () => {
    const anchor = { x: 100, y: 100, w: 80, h: 80 };
    const r = computePopupRect(anchor, { w: 200, h: 240 }, viewport);
    expect(r.y).toBe(anchor.y + anchor.h + 8);
    expect(r.x).toBe(anchor.x);
  });

  it("opens above when below is too cramped but above has more room", () => {
    const anchor = { x: 100, y: 700, w: 80, h: 80 };
    const r = computePopupRect(anchor, { w: 200, h: 300 }, viewport);
    expect(r.y).toBeLessThan(anchor.y);
    expect(r.y + r.h).toBeLessThanOrEqual(anchor.y - 8 + 1);
  });

  it("clamps within the viewport on the right/bottom edges", () => {
    const anchor = { x: 950, y: 750, w: 40, h: 40 };
    const r = computePopupRect(anchor, { w: 200, h: 200 }, viewport);
    expect(r.x + r.w).toBeLessThanOrEqual(viewport.w);
    expect(r.y).toBeGreaterThanOrEqual(0);
  });

  it("clamps within the viewport on the left/top edges", () => {
    const anchor = { x: -50, y: -50, w: 40, h: 40 };
    const r = computePopupRect(anchor, { w: 200, h: 200 }, viewport);
    expect(r.x).toBeGreaterThanOrEqual(0);
    expect(r.y).toBeGreaterThanOrEqual(0);
  });
});
