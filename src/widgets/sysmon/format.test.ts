import { describe, expect, it } from "vitest";
import { formatBytes, formatRate, rateCeiling, shortProcName } from "./format";

describe("formatRate", () => {
  it("picks the right unit", () => {
    expect(formatRate(500)).toBe("500 B/s");
    expect(formatRate(2048)).toBe("2.0 KB/s");
    expect(formatRate(1024 * 512)).toBe("512 KB/s");
    expect(formatRate(1024 * 1024 * 2.5)).toBe("2.5 MB/s");
    expect(formatRate(1024 ** 3 * 1.5)).toBe("1.5 GB/s");
  });

  it("keeps one decimal only below 10 so the width stays steady", () => {
    expect(formatRate(1024 * 9.5)).toBe("9.5 KB/s");
    expect(formatRate(1024 * 99)).toBe("99 KB/s");
  });

  it("shows an idle link as zero bytes, not zero kilobytes", () => {
    expect(formatRate(0)).toBe("0 B/s");
  });

  it("survives nonsense input", () => {
    expect(formatRate(-5)).toBe("0 KB/s");
    expect(formatRate(NaN)).toBe("0 KB/s");
    expect(formatRate(Infinity)).toBe("0 KB/s");
  });
});

describe("formatBytes", () => {
  it("shows MB then GB", () => {
    expect(formatBytes(1024 ** 2 * 300)).toBe("300 MB");
    expect(formatBytes(1024 ** 3 * 2)).toBe("2.0 GB");
    expect(formatBytes(1024 ** 2 * 5.5)).toBe("5.5 MB");
  });

  it("survives nonsense input", () => {
    expect(formatBytes(0)).toBe("0 MB");
    expect(formatBytes(-1)).toBe("0 MB");
    expect(formatBytes(NaN)).toBe("0 MB");
  });
});

describe("rateCeiling", () => {
  it("never goes below a readable floor", () => {
    expect(rateCeiling([0, 0, 0])).toBe(64 * 1024);
    expect(rateCeiling([])).toBe(64 * 1024);
  });

  it("doubles until it covers the peak", () => {
    expect(rateCeiling([100 * 1024])).toBe(128 * 1024);
    expect(rateCeiling([200 * 1024])).toBe(256 * 1024);
  });

  it("is stable while traffic stays in the same band", () => {
    // 같은 구간 안에서 움직이면 눈금이 바뀌지 않아야 그래프가 출렁이지 않는다
    const a = rateCeiling([70 * 1024]);
    const b = rateCeiling([120 * 1024]);
    expect(a).toBe(b);
  });

  it("always covers the peak", () => {
    for (const peak of [1, 1e3, 1e5, 1e7, 1e9]) {
      expect(rateCeiling([peak])).toBeGreaterThanOrEqual(peak);
    }
  });
});

describe("shortProcName", () => {
  it("drops the .exe", () => {
    expect(shortProcName("chrome.exe")).toBe("chrome");
    expect(shortProcName("Code.EXE")).toBe("Code");
  });

  it("truncates long names", () => {
    expect(shortProcName("averyveryverylongprocessname.exe", 10)).toBe("averyvery…");
  });

  it("leaves short names alone", () => {
    expect(shortProcName("node")).toBe("node");
  });
});
