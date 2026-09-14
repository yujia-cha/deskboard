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
