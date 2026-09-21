import { describe, expect, it } from "vitest";
import { dropTarget, moveBefore } from "./reorder";

const ids = ["a", "b", "c", "d"];

describe("moveBefore", () => {
  it("항목을 다른 항목 앞으로 옮긴다", () => {
    expect(moveBefore(ids, "d", "b")).toEqual(["a", "d", "b", "c"]);
    expect(moveBefore(ids, "a", "c")).toEqual(["b", "a", "c", "d"]);
  });

  it("null 이면 맨 뒤로", () => {
    expect(moveBefore(ids, "a", null)).toEqual(["b", "c", "d", "a"]);
  });

  it("숨은 항목은 자기 자리를 지킨다 — 보이는 것만으로 계산해도 된다", () => {
    // b 가 완료라 화면에 없다. a 를 c 앞으로 옮겨도 b 는 원래 자리에 남는다.
    expect(moveBefore(ids, "a", "c")).toEqual(["b", "a", "c", "d"]);
  });

  it("바뀌는 게 없으면 **같은 배열**을 돌려준다 (매 포인터 이동마다 부른다)", () => {
    expect(moveBefore(ids, "b", "b")).toBe(ids);   // 자기 앞
    expect(moveBefore(ids, "b", "c")).toBe(ids);   // 이미 c 바로 앞
    expect(moveBefore(ids, "d", null)).toBe(ids);  // 이미 맨 뒤
  });

  it("모르는 id 는 목록을 건드리지 않는다", () => {
    expect(moveBefore(ids, "zzz", "b")).toBe(ids);
    expect(moveBefore(ids, "a", "zzz")).toBe(ids);
  });
});

describe("dropTarget", () => {
  const rows = [
    { id: "a", top: 0, height: 20 },
    { id: "b", top: 20, height: 20 },
    { id: "c", top: 40, height: 20 },
  ];

  it("항목의 위쪽 절반이면 그 항목 앞", () => {
    expect(dropTarget(rows, 5)).toBe("a");
    expect(dropTarget(rows, 25)).toBe("b");
  });

  it("아래쪽 절반이면 다음 항목 앞", () => {
    expect(dropTarget(rows, 15)).toBe("b");
    expect(dropTarget(rows, 35)).toBe("c");
  });

  it("마지막 항목의 아래쪽이면 맨 뒤", () => {
    expect(dropTarget(rows, 55)).toBe(null);
    expect(dropTarget(rows, 999)).toBe(null);
  });

  it("빈 목록은 맨 뒤", () => {
    expect(dropTarget([], 10)).toBe(null);
  });
});
