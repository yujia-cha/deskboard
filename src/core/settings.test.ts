import { describe, expect, it } from "vitest";
import { defaultInstances, findFreeSlot, overlaps, resolvePalette } from "./settings";

describe("layout", () => {
  it("default widgets do not overlap", () => {
    const list = defaultInstances();
    for (let i = 0; i < list.length; i++)
      for (let j = i + 1; j < list.length; j++)
        expect(overlaps(list[i], list[j]), `${list[i].widgetId} vs ${list[j].widgetId}`).toBe(false);
  });

  it("findFreeSlot avoids existing widgets", () => {
    const existing = defaultInstances();
    const { x, y } = findFreeSlot(existing, 300, 200, { w: 1920, h: 1040 });
    expect(existing.some((e) => overlaps({ x, y, w: 300, h: 200 }, e))).toBe(false);
  });

  it("findFreeSlot falls back when nothing fits", () => {
    expect(findFreeSlot([{ x: 0, y: 0, w: 5000, h: 5000 }], 300, 200, { w: 1920, h: 1040 })).toEqual({ x: 24, y: 24 });
  });
});

describe("palette", () => {
  it("passes explicit choices through", () => {
    expect(resolvePalette("dark", false)).toBe("dark");
    expect(resolvePalette("light", true)).toBe("light");
  });

  it('resolves "auto" from the OS preference', () => {
    expect(resolvePalette("auto", true)).toBe("dark");
    expect(resolvePalette("auto", false)).toBe("light");
  });

  it("defaults to dark when the OS preference is unreadable", () => {
    // matchMedia 가 없는 환경에서 호출하면 다크로 떨어진다
    expect(resolvePalette("auto")).toBe("dark");
  });
});
