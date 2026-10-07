import { describe, expect, it } from "vitest";
import { ago, parseDomains, safeUrl, sourceOf } from "./format";

describe("safeUrl", () => {
  it("http/https 만 연다", () => {
    expect(safeUrl("https://example.com/a")).toBe("https://example.com/a");
    expect(safeUrl("javascript:alert(1)")).toBeNull();
    expect(safeUrl("file:///C:/x")).toBeNull();
    expect(safeUrl("not a url")).toBeNull();
  });
});

describe("sourceOf", () => {
  it("비어 있으면 호스트 이름", () => {
    expect(sourceOf({ title: "", url: "https://www.zdnet.co.kr/x", source: "", published: "", summary: "" })).toBe("zdnet.co.kr");
    expect(sourceOf({ title: "", url: "https://a.com", source: " 지디넷 ", published: "", summary: "" })).toBe("지디넷");
  });
});

describe("ago", () => {
  const now = 1_000_000_000_000;
  it("단위를 고른다", () => {
    expect(ago(now - 10_000, now)).toBe("방금");
    expect(ago(now - 12 * 60_000, now)).toBe("12분 전");
    expect(ago(now - 3 * 3_600_000, now)).toBe("3시간 전");
    expect(ago(now - 2 * 86_400_000, now)).toBe("2일 전");
  });
});

describe("parseDomains", () => {
  it("쉼표·공백 구분, 주소에서 호스트만, 중복·잘못된 것 제거", () => {
    expect(parseDomains("news.ycombinator.com, https://www.zdnet.co.kr/news/x , zdnet.co.kr, localhost, ")).toEqual([
      "news.ycombinator.com", "zdnet.co.kr",
    ]);
  });
});
