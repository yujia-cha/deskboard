import { describe, expect, it } from "vitest";
import { BCD_BITS, bcdColumns, FIB_VALUES, fibCombos, fibReadback, fibRoles } from "./oddtime";

const fromBits = (bits: boolean[]) =>
  bits.reduce((n, b, i) => n + (b ? 1 << (bits.length - 1 - i) : 0), 0);

describe("binary clock", () => {
  it("each column reads back to its own digit", () => {
    for (const [h, m, s] of [[0, 0, 0], [23, 59, 59], [9, 30, 7], [12, 5, 45]] as const) {
      const cols = bcdColumns(h, m, s);
      expect(cols.map((c) => fromBits(c.bits))).toEqual(cols.map((c) => c.digit));
    }
  });

  it("reconstructs the original time", () => {
    for (let h = 0; h < 24; h++) {
      for (const m of [0, 7, 30, 59]) {
        const c = bcdColumns(h, m, 42);
        const read = (a: number, b: number) => fromBits(c[a].bits) * 10 + fromBits(c[b].bits);
        expect(read(0, 1)).toBe(h);
        expect(read(2, 3)).toBe(m);
        expect(read(4, 5)).toBe(42);
      }
    }
  });

  it("sizes each column to the digits it must show", () => {
    const cols = bcdColumns(23, 59, 59);
    expect(cols.map((c) => c.bits.length)).toEqual([...BCD_BITS]);
    // 십의 자리는 최대 5 (분/초) 또는 2 (시) 라 4비트가 필요 없다
    expect(cols[0].bits.length).toBe(2);
    expect(cols[2].bits.length).toBe(3);
  });

  it("every bit position is reachable", () => {
    // 절대 켜지지 않는 칸이 있으면 읽기 어렵다 — 하루를 돌려 모든 칸이 한 번은 켜지는지 본다
    const seen = BCD_BITS.map((n) => new Array(n).fill(false));
    for (let h = 0; h < 24; h++)
      for (let m = 0; m < 60; m++)
        bcdColumns(h, m, m).forEach((c, ci) => c.bits.forEach((b, bi) => { if (b) seen[ci][bi] = true; }));
    seen.forEach((col, ci) => col.forEach((hit, bi) => expect(hit, `col ${ci} bit ${bi}`).toBe(true)));
  });

  it("drops the seconds columns when asked", () => {
    expect(bcdColumns(1, 2, 3, false)).toHaveLength(4);
    expect(bcdColumns(1, 2, 3, false).every((c) => c.unit !== "s")).toBe(true);
  });

  it("labels each column with its unit", () => {
    expect(bcdColumns(1, 2, 3).map((c) => c.unit)).toEqual(["h", "h", "m", "m", "s", "s"]);
  });
});

describe("fibonacci clock", () => {
  it("uses the fibonacci square sizes", () => {
    expect([...FIB_VALUES]).toEqual([1, 1, 2, 3, 5]);
    // 1..12 를 전부 표현할 수 있어야 시(1~12)를 그릴 수 있다
    for (let n = 1; n <= 12; n++) expect(fibCombos(n).length, `sum ${n}`).toBeGreaterThan(0);
    expect(fibCombos(0)).toContain(0);
  });

  it("round-trips every time it can display", () => {
    for (let h = 1; h <= 12; h++) {
      for (let m = 0; m < 60; m += 5) {
        const back = fibReadback(fibRoles(h, m));
        expect(back.hour, `${h}:${m}`).toBe(h);
        expect(back.minute, `${h}:${m}`).toBe(m);
      }
    }
  });

  it("treats 0 and 12 o'clock as twelve", () => {
    expect(fibReadback(fibRoles(0, 0)).hour).toBe(12);
    expect(fibReadback(fibRoles(12, 0)).hour).toBe(12);
    expect(fibReadback(fibRoles(24, 0)).hour).toBe(12);
  });

  it("rounds minutes down to the 5 minute slot", () => {
    for (let m = 20; m < 25; m++) expect(fibReadback(fibRoles(3, m)).minute).toBe(20);
  });

  it("variants give different layouts but the same time", () => {
    const seen = new Set<string>();
    for (let v = 0; v < 6; v++) {
      const roles = fibRoles(9, 25, v);
      expect(fibReadback(roles)).toEqual({ hour: 9, minute: 25 });
      seen.add(roles.join(","));
    }
    // 9시는 조합이 하나뿐이 아니므로 배치가 최소 두 가지는 나온다
    expect(seen.size).toBeGreaterThan(1);
  });

  it("marks squares used by both hand as 'both'", () => {
    // 5시 25분 → 분 슬롯 5, 시 5 : 같은 칸(5)을 둘 다 쓴다
    expect(fibRoles(5, 25)).toContain("both");
  });

  it("never returns an unknown role", () => {
    for (let h = 0; h < 24; h++)
      for (let m = 0; m < 60; m++)
        for (const r of fibRoles(h, m, m))
          expect(["off", "hour", "minute", "both"]).toContain(r);
  });
});
