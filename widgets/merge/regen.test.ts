import { describe, expect, it } from "vitest";
import { formatMMSS, nextRegenIn, regenCycles } from "./regen";

describe("nextRegenIn", () => {
  const anchor = 1_000_000;
  it("막 정산했으면 한 주기", () => {
    expect(nextRegenIn(anchor, 120, anchor)).toBe(120);
  });
  it("초가 흐를수록 줄어든다", () => {
    expect(nextRegenIn(anchor, 120, anchor + 1000)).toBe(119);
    expect(nextRegenIn(anchor, 120, anchor + 119_000)).toBe(1);
    expect(nextRegenIn(anchor, 120, anchor + 119_999)).toBe(1);
  });
  it("주기 경계에서 되돌아간다", () => {
    expect(nextRegenIn(anchor, 120, anchor + 120_000)).toBe(120);
    expect(nextRegenIn(anchor, 120, anchor + 125_000)).toBe(115);
  });
  it("오래 뒤에도 주기 안에 있다", () => {
    expect(nextRegenIn(anchor, 120, anchor + 3 * 3600_000 + 30_000)).toBe(90);
  });
  it("시계가 되감기면 한 주기", () => {
    expect(nextRegenIn(anchor, 120, anchor - 5000)).toBe(120);
  });
});

describe("regenCycles", () => {
  it("지난 주기 수 — 되감기면 0", () => {
    expect(regenCycles(0, 120, 119_999)).toBe(0);
    expect(regenCycles(0, 120, 120_000)).toBe(1);
    expect(regenCycles(0, 120, 250_000)).toBe(2);
    expect(regenCycles(10_000, 120, 0)).toBe(0);
  });
});

describe("formatMMSS", () => {
  it("분:초", () => {
    expect(formatMMSS(0)).toBe("00:00");
    expect(formatMMSS(9)).toBe("00:09");
    expect(formatMMSS(120)).toBe("02:00");
    expect(formatMMSS(754)).toBe("12:34");
    expect(formatMMSS(-3)).toBe("00:00");
  });
});
