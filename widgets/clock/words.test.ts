import { describe, expect, it } from "vitest";
import { EN, KO, GRIDS, litCells, readout } from "./words";

describe("word clock grids", () => {
  it("every row has the same width", () => {
    for (const [name, g] of Object.entries(GRIDS)) {
      const w = g.rows[0].length;
      for (const r of g.rows) expect(r.length, `${name}: ${r}`).toBe(w);
    }
  });

  it("every span stays inside the grid", () => {
    for (const [name, g] of Object.entries(GRIDS)) {
      for (let h = 0; h < 24; h++) {
        for (let m = 0; m < 60; m++) {
          for (const [r, c, len] of g.spansFor(h, m)) {
            expect(g.rows[r], `${name} ${h}:${m} row ${r}`).toBeDefined();
            expect(c + len, `${name} ${h}:${m} span [${r},${c},${len}]`).toBeLessThanOrEqual(g.rows[r].length);
            expect(c).toBeGreaterThanOrEqual(0);
          }
        }
      }
    }
  });

  it("never returns an empty reading", () => {
    for (const [name, g] of Object.entries(GRIDS))
      for (let h = 0; h < 24; h++)
        for (let m = 0; m < 60; m += 5)
          expect(readout(g, h, m).length, `${name} ${h}:${m}`).toBeGreaterThan(0);
  });
});

describe("English readings", () => {
  const r = (h: number, m: number) => readout(EN, h, m);

  it("reads o'clock on the hour", () => {
    expect(r(10, 0)).toBe("IT IS TEN OCLOCK");
    expect(r(0, 0)).toBe("IT IS TWELVE OCLOCK");
    expect(r(13, 0)).toBe("IT IS ONE OCLOCK");
  });

  it("reads past for the first half hour", () => {
    expect(r(10, 5)).toBe("IT IS FIVE PAST TEN");
    expect(r(10, 15)).toBe("IT IS QUARTER PAST TEN");
    expect(r(10, 25)).toBe("IT IS TWENTY FIVE PAST TEN");
    expect(r(10, 30)).toBe("IT IS HALF PAST TEN");
  });

  it("switches to the next hour after half past", () => {
    expect(r(10, 35)).toBe("IT IS TWENTY FIVE TO ELEVEN");
    expect(r(10, 45)).toBe("IT IS QUARTER TO ELEVEN");
    expect(r(10, 55)).toBe("IT IS FIVE TO ELEVEN");
  });

  it("wraps 12 correctly when counting to the next hour", () => {
    expect(r(11, 40)).toBe("IT IS TWENTY TO TWELVE");
    expect(r(23, 50)).toBe("IT IS TEN TO TWELVE");
    expect(r(12, 50)).toBe("IT IS TEN TO ONE");
  });

  it("rounds down to the 5 minute slot", () => {
    for (let m = 5; m < 10; m++) expect(r(10, m)).toBe("IT IS FIVE PAST TEN");
    for (let m = 0; m < 5; m++) expect(r(10, m)).toBe("IT IS TEN OCLOCK");
  });
});

describe("Korean readings", () => {
  const r = (h: number, m: number) => readout(KO, h, m);

  it("puts the hour first and never uses a 'to' form", () => {
    expect(r(10, 0)).toBe("지금은 열 시 정 각");
    expect(r(10, 30)).toBe("지금은 열 시 삼십 분");
    // 영어는 10:35 를 "eleven 기준"으로 읽지만 한국어는 그대로 열시다
    expect(r(10, 35)).toBe("지금은 열 시 삼십오 분");
    expect(r(10, 55)).toBe("지금은 열 시 오십오 분");
  });

  it("uses native-Korean hour words", () => {
    expect(r(1, 0)).toBe("지금은 한 시 정 각");
    expect(r(7, 0)).toBe("지금은 일 곱 시 정 각");
    expect(r(11, 0)).toBe("지금은 열한 시 정 각");
    expect(r(12, 0)).toBe("지금은 열두 시 정 각");
    expect(r(0, 0)).toBe("지금은 열두 시 정 각");
  });

  it("covers every 5 minute slot", () => {
    for (let m = 5; m < 60; m += 5) expect(r(9, m)).toMatch(/분$/);
  });
});

describe("litCells", () => {
  it("expands spans into individual cells", () => {
    const cells = litCells(EN, 10, 30);
    // HALF = 행 3, 열 0~3
    for (let c = 0; c < 4; c++) expect(cells.has(`3,${c}`)).toBe(true);
    expect(cells.has("3,4")).toBe(false);
  });

  it("lights fewer cells than the whole grid", () => {
    const total = EN.rows.length * EN.rows[0].length;
    expect(litCells(EN, 10, 30).size).toBeLessThan(total);
    expect(litCells(EN, 10, 30).size).toBeGreaterThan(0);
  });
});
