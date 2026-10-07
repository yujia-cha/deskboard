import { describe, expect, it } from "vitest";
import {
  addExtra, daysAgo, duration, durationShort, hhmm, localDay, parseExtras,
  pickCandidates, pickTracked, prettyExe, removeExtra, share, sliceColor, weekStart,
} from "./activity";

describe("localDay", () => {
  it("uses the local date, not UTC", () => {
    // 2026-09-21 00:30 로컬. toISOString() 을 썼다면 UTC 로 밀려 9/20 이 나온다.
    const d = new Date(2026, 8, 21, 0, 30);
    expect(localDay(d)).toBe("2026-09-21");
  });

  it("pads month and day", () => {
    expect(localDay(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});

describe("daysAgo", () => {
  it("walks back and crosses month boundaries", () => {
    expect(daysAgo(0, new Date(2026, 8, 21))).toBe("2026-09-21");
    expect(daysAgo(1, new Date(2026, 8, 21))).toBe("2026-09-20");
    expect(daysAgo(21, new Date(2026, 8, 21))).toBe("2026-08-31");
  });
});

describe("weekStart", () => {
  it("returns Monday for any day of that week", () => {
    // 2026-09-21 은 월요일
    const monday = "2026-09-21";
    expect(weekStart(new Date(2026, 8, 21))).toBe(monday); // 월
    expect(weekStart(new Date(2026, 8, 24))).toBe(monday); // 목
    expect(weekStart(new Date(2026, 8, 27))).toBe(monday); // 일
  });

  it("treats Sunday as the end of the week, not the start", () => {
    // 일요일(9/27)의 주 시작은 그 주 월요일(9/21)이어야 한다
    expect(weekStart(new Date(2026, 8, 27))).toBe("2026-09-21");
  });
});

describe("duration", () => {
  it("reads naturally in Korean", () => {
    expect(duration(3600)).toBe("1시간");
    expect(duration(3600 + 14 * 60)).toBe("1시간 14분");
    expect(duration(45 * 60)).toBe("45분");
  });

  it("does not pretend a few seconds is zero minutes", () => {
    expect(duration(5)).toBe("1분 미만");
    expect(duration(0)).toBe("1분 미만");
    expect(duration(-10)).toBe("1분 미만");
  });

  it("drops the minutes when they round away", () => {
    expect(duration(7200)).toBe("2시간");
  });
});

describe("durationShort", () => {
  it("is compact", () => {
    expect(durationShort(3600)).toBe("1:00");
    expect(durationShort(3600 + 5 * 60)).toBe("1:05");
    expect(durationShort(30 * 60)).toBe("30분");
    expect(durationShort(0)).toBe("0분");
  });
});

describe("hhmm", () => {
  it("pulls the time out of an ISO stamp", () => {
    expect(hhmm("2026-09-21T14:05:09")).toBe("14:05");
  });

  it("survives a malformed stamp", () => {
    expect(hhmm("2026-09-21")).toBe("");
    expect(hhmm("")).toBe("");
  });
});

describe("prettyExe", () => {
  it("renames the ones worth renaming", () => {
    expect(prettyExe("code")).toBe("VS Code");
    expect(prettyExe("eldenring")).toBe("Elden Ring");
    expect(prettyExe("CODE")).toBe("VS Code");
  });

  it("leaves unknown programs as they are", () => {
    expect(prettyExe("some-tool")).toBe("some-tool");
  });
});

describe("share", () => {
  it("is a plain ratio", () => {
    expect(share(25, 100)).toBe(0.25);
    expect(share(0, 100)).toBe(0);
  });

  it("never divides by zero", () => {
    expect(share(5, 0)).toBe(0);
    expect(share(0, 0)).toBe(0);
  });
});

describe("sliceColor", () => {
  it("uses theme tokens, not made-up colours", () => {
    for (let i = 0; i < 5; i++) expect(sliceColor(i)).toMatch(/^var\(--/);
  });

  it("gives different colours to the first few slices", () => {
    const first = [0, 1, 2, 3, 4].map(sliceColor);
    expect(new Set(first).size).toBe(5);
  });

  it("recycles the palette faded rather than running out", () => {
    expect(sliceColor(5)).not.toBe(sliceColor(0));
    expect(sliceColor(5)).toContain("color-mix");
    // 아무리 많아도 완전히 투명해지지는 않는다
    expect(sliceColor(50)).toMatch(/3[0-9]%|[4-9][0-9]%|100%/);
  });
});

describe("extras list", () => {
  it("normalises case, spacing and .exe", () => {
    expect(parseExtras(" Code.exe , CHROME ,, discord ")).toEqual(["code", "chrome", "discord"]);
    expect(parseExtras("")).toEqual([]);
  });

  it("accepts human-readable names", () => {
    expect(parseExtras("firefox, visual studio code, ")).toEqual(["firefox", "code"]);
    expect(parseExtras("VS Code")).toEqual(["code"]);
  });

  it("adds without duplicating", () => {
    expect(addExtra("", "Code.exe")).toBe("code");
    expect(addExtra("code", "chrome")).toBe("code, chrome");
    // 이미 있으면 목록을 건드리지 않는다
    expect(addExtra("code, chrome", "CODE.EXE")).toBe("code, chrome");
    expect(addExtra("code", "  ")).toBe("code");
  });

  it("removes by normalised name", () => {
    expect(removeExtra("code, chrome", "Chrome.exe")).toBe("code");
    expect(removeExtra("code", "missing")).toBe("code");
    expect(removeExtra("code", "code")).toBe("");
  });
});

describe("pickTracked", () => {
  const rows = [
    { exe: "code", category: "work", seconds: 4000 },
    { exe: "eldenring", category: "game", seconds: 3000 },
    { exe: "chrome", category: "other", seconds: 2000 },
    { exe: "roblox", category: "game", seconds: 1000 },
  ] as const;

  it("always includes games", () => {
    const got = pickTracked([...rows], []);
    expect(got.map((r) => r.exe)).toEqual(["eldenring", "roblox"]);
  });

  it("adds only the programs the user registered", () => {
    const got = pickTracked([...rows], ["code"]);
    expect(got.map((r) => r.exe)).toEqual(["code", "eldenring", "roblox"]);
    expect(got.map((r) => r.exe)).not.toContain("chrome");
  });

  it("orders by time spent", () => {
    const got = pickTracked([...rows], ["code", "chrome"]);
    expect(got.map((r) => r.seconds)).toEqual([4000, 3000, 2000, 1000]);
  });

  it("is empty when nothing qualifies", () => {
    expect(pickTracked([{ exe: "x", category: "other", seconds: 5 }], [])).toEqual([]);
  });

  it("candidates exclude games and already-registered programs", () => {
    const c = pickCandidates([...rows], ["code"]);
    expect(c.map((r) => r.exe)).toEqual(["chrome"]);
  });

  it("candidates are capped", () => {
    const many = Array.from({ length: 20 }, (_, i) => ({ exe: `p${i}`, category: "other" as const, seconds: 100 - i }));
    expect(pickCandidates(many, [], 5)).toHaveLength(5);
  });
});
