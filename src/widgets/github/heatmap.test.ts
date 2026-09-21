import { describe, expect, it } from "vitest";
import { fitGrid, padWeek, ROWS, type Day } from "./heatmap";

const day = (date: string, count = 1): Day => ({ date, count, level: 1 });

describe("padWeek", () => {
  it("keeps a full week as is", () => {
    // 2026-01-04 는 일요일
    const w = ["04", "05", "06", "07", "08", "09", "10"].map((d) => day(`2026-01-${d}`));
    expect(padWeek(w).map((d) => d?.date ?? null)).toEqual(w.map((d) => d.date));
  });

  it("pads the first partial week at the front", () => {
    // 2026-01-01 은 목요일(4) → 앞에 빈칸 4개
    const w = [day("2026-01-01"), day("2026-01-02"), day("2026-01-03")];
    const out = padWeek(w);
    expect(out.length).toBe(ROWS);
    expect(out.slice(0, 4)).toEqual([null, null, null, null]);
    expect(out[4]?.date).toBe("2026-01-01");
  });

  it("pads the last partial week at the end", () => {
    const w = [day("2026-01-04"), day("2026-01-05")];
    const out = padWeek(w);
    expect(out.length).toBe(ROWS);
    expect(out[0]?.date).toBe("2026-01-04");
    expect(out.slice(2).every((d) => d === null)).toBe(true);
  });

  it("survives an empty week", () => {
    expect(padWeek([])).toEqual(Array(ROWS).fill(null));
  });
});

describe("fitGrid", () => {
  it("shrinks the cells in a short widget", () => {
    expect(fitGrid({ w: 300, h: 120 }).cell).toBeLessThan(fitGrid({ w: 300, h: 400 }).cell);
  });

  it("never returns a cell or column count that cannot be drawn", () => {
    const g = fitGrid({ w: 10, h: 10 });
    expect(g.cell).toBeGreaterThanOrEqual(4);
    expect(g.cols).toBeGreaterThanOrEqual(4);
  });

  it("fills a wide widget with more weeks", () => {
    expect(fitGrid({ w: 600, h: 200 }).cols).toBeGreaterThan(fitGrid({ w: 200, h: 200 }).cols);
  });
});
