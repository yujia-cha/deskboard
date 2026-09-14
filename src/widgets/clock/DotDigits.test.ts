import { describe, expect, it } from "vitest";
import { GLYPHS, GLYPH_ROWS, fitCell } from "./DotDigits";

describe("dot glyphs", () => {
  it("every glyph has 7 rows of equal width with only 0/1", () => {
    for (const [ch, rows] of Object.entries(GLYPHS)) {
      expect(rows.length, ch).toBe(GLYPH_ROWS);
      const w = rows[0].length;
      for (const r of rows) { expect(r.length, ch).toBe(w); expect(/^[01]+$/.test(r), ch).toBe(true); }
    }
  });
  it("digits are 5 wide and colon is defined", () => {
    for (let d = 0; d <= 9; d++) expect(GLYPHS[String(d)][0].length).toBe(5);
    expect(GLYPHS[":"]).toBeDefined();
  });
  it("fitCell shrinks with more characters", () => {
    expect(fitCell("12:34:56", 300, 100)).toBeLessThan(fitCell("12:34", 300, 100));
    expect(fitCell("12:34", 300, 100)).toBeGreaterThanOrEqual(3);
  });
});

import { BLOCK_GLYPHS, BLOCK_ROWS } from "./DotDigits";
describe("block glyphs", () => {
  it("digits are 3x5 and colon is one column", () => {
    for (let d = 0; d <= 9; d++) {
      const rows = BLOCK_GLYPHS[String(d)];
      expect(rows.length).toBe(BLOCK_ROWS);
      for (const r of rows) expect(r).toMatch(/^[01]{3}$/);
    }
    expect(BLOCK_GLYPHS[":"].every((r) => r.length === 1)).toBe(true);
  });
  it("blocks fit larger than dots for the same box", () => {
    expect(fitCell("12:34:56", 300, 100, "blocks")).toBeGreaterThan(fitCell("12:34:56", 300, 100, "dots"));
  });
});
