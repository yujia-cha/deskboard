import { describe, expect, it } from "vitest";
import { clampVolume, computeVolumeBarRect, toggleMute, volumeFromPointerY, volumeIcon } from "./volume";

describe("clampVolume", () => {
  it("clamps to 0..100 and rounds", () => {
    expect(clampVolume(-5)).toBe(0);
    expect(clampVolume(150)).toBe(100);
    expect(clampVolume(42.6)).toBe(43);
    expect(clampVolume(NaN)).toBe(0);
  });
});

describe("volumeFromPointerY", () => {
  it("bottom of track = 0, top of track = 100", () => {
    expect(volumeFromPointerY(200, 0, 200)).toBe(0);
    expect(volumeFromPointerY(0, 0, 200)).toBe(100);
    expect(volumeFromPointerY(100, 0, 200)).toBe(50);
  });
  it("clamps outside the track", () => {
    expect(volumeFromPointerY(-50, 0, 200)).toBe(100);
    expect(volumeFromPointerY(250, 0, 200)).toBe(0);
  });
  it("degenerate track height returns 0", () => {
    expect(volumeFromPointerY(10, 0, 0)).toBe(0);
  });
});

describe("toggleMute", () => {
  it("mutes and remembers the previous volume", () => {
    expect(toggleMute({ volume: 70, prevVolume: null })).toEqual({ volume: 0, prevVolume: 70 });
  });
  it("unmutes to the remembered volume", () => {
    expect(toggleMute({ volume: 0, prevVolume: 70 })).toEqual({ volume: 70, prevVolume: null });
  });
  it("unmutes to 50 when there is no remembered volume", () => {
    expect(toggleMute({ volume: 0, prevVolume: null })).toEqual({ volume: 50, prevVolume: null });
  });
  it("unmutes to 50 when the remembered volume was itself 0", () => {
    expect(toggleMute({ volume: 0, prevVolume: 0 })).toEqual({ volume: 50, prevVolume: null });
  });
});

describe("volumeIcon", () => {
  it("picks an icon per range", () => {
    expect(volumeIcon(0)).toBe("🔇");
    expect(volumeIcon(20)).toBe("🔈");
    expect(volumeIcon(50)).toBe("🔉");
    expect(volumeIcon(90)).toBe("🔊");
  });
});

describe("computeVolumeBarRect", () => {
  const viewport = { w: 1000, h: 800 };
  it("opens above the icon when there is room", () => {
    const r = computeVolumeBarRect({ x: 100, y: 300, w: 24, h: 24 }, { w: 40, h: 120 }, viewport);
    expect(r.y).toBeLessThan(300);
    expect(r.y + r.h).toBeLessThanOrEqual(300 - 8 + 1);
  });
  it("opens below when there is not enough room above", () => {
    const r = computeVolumeBarRect({ x: 100, y: 20, w: 24, h: 24 }, { w: 40, h: 120 }, viewport);
    expect(r.y).toBeGreaterThan(20);
  });
  it("stays inside the viewport", () => {
    const r = computeVolumeBarRect({ x: 990, y: 10, w: 24, h: 24 }, { w: 40, h: 120 }, viewport);
    expect(r.x).toBeGreaterThanOrEqual(0);
    expect(r.x + r.w).toBeLessThanOrEqual(viewport.w);
    expect(r.y).toBeGreaterThanOrEqual(0);
    expect(r.y + r.h).toBeLessThanOrEqual(viewport.h);
  });
});
