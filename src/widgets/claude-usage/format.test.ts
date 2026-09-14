import { describe, expect, it } from "vitest";
import { fmtTokens, fmtUsd, shortModel } from "./ClaudeUsage";

describe("claude usage formatting", () => {
  it("formats tokens with K/M/B", () => {
    expect(fmtTokens(999)).toBe("999");
    expect(fmtTokens(6400)).toBe("6.4K");
    expect(fmtTokens(183_740_000)).toBe("183.74M");
    expect(fmtTokens(2_000_000_000)).toBe("2.00B");
  });

  it("formats usd with adaptive precision", () => {
    expect(fmtUsd(0.734)).toBe("$0.73");
    expect(fmtUsd(32.61)).toBe("$32.6");
    expect(fmtUsd(128.4)).toBe("$128");
  });

  it("shortens model ids", () => {
    expect(shortModel("claude-opus-5")).toBe("opus-5");
    expect(shortModel("claude-sonnet-4-6-20260101")).toBe("sonnet-4-6");
  });
});
