import { describe, expect, it } from "vitest";
import { FlipSim } from "./sim";
import { newRaster, rasterize } from "./render";

describe("rasterize", () => {
  const sim = new FlipSim({ res: 32, fill: 0.5, shape: "square" });
  const r = newRaster(32);
  rasterize(sim, r, { r: 100, g: 150, b: 200 });
  const alpha = (x: number, y: number) => r.rgba[(y * 32 + x) * 4 + 3];
  const red = (x: number, y: number) => r.rgba[(y * 32 + x) * 4];

  it("아래 절반은 물, 위는 비어 있다", () => {
    expect(alpha(16, 26)).toBe(255);
    expect(alpha(16, 4)).toBe(0);
  });

  it("물 한가운데에는 구멍이 없다", () => {
    let holes = 0;
    for (let y = 22; y < 28; y++) for (let x = 6; x < 26; x++) if (!alpha(x, y)) holes++;
    expect(holes).toBe(0);
  });

  it("표면은 같은 색을 밝게, 속은 그 색 그대로", () => {
    let surface = -1;
    for (let y = 0; y < 32; y++) if (alpha(16, y)) { surface = y; break; }
    expect(surface).toBeGreaterThan(0);
    expect(red(16, surface)).toBeGreaterThan(100);
    expect(red(16, surface + 3)).toBe(100);
  });
});
