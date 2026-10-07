import { describe, expect, it } from "vitest";
import { computeLayout, isWide, orderMarks, sellRefund, slotSize } from "./layout";
import type { Cell, Item } from "./types";

describe("computeLayout", () => {
  it("기본 크기 400×560 은 좁은 배치 (400 < 420)", () => {
    const l = computeLayout({ w: 400, h: 560 });
    expect(l.wide).toBe(false);
    // areaW 400 → 57, boardH 560-202 = 358 → 39
    expect(l.cell).toBe(39);
    expect(l.gridW).toBe(273);
    expect(l.gridH).toBe(351);
    expect(l.boardTop).toBe(68);
    expect(l.boardLeft).toBe(63);
    expect(l.compact).toBe(false);
  });

  it("넓은 배치 600×560 — 클리커 열 64 를 오른쪽에 뺀다", () => {
    const l = computeLayout({ w: 600, h: 560 });
    expect(l.wide).toBe(true);
    // areaW 536 → 76, boardH 560-130 = 430 → 47
    expect(l.cell).toBe(47);
    expect(l.areaW).toBe(536);
    expect(l.boardLeft).toBe(Math.floor((536 - 329) / 2));
  });

  it("K-5 경계", () => {
    expect(isWide({ w: 300, h: 400 })).toBe(true);
    expect(isWide({ w: 299, h: 400 })).toBe(false);
  });

  it("좁은 배치는 클리커 72 를 아래에 뺀다", () => {
    const l = computeLayout({ w: 240, h: 560 });
    expect(l.wide).toBe(false);
    // areaW 240 → 34, boardH 560-202=358 → 39 → 34
    expect(l.cell).toBe(34);
    expect(l.areaW).toBe(240);
    expect(l.compact).toBe(true);
  });

  it("wide 를 직접 줄 수 있다", () => {
    expect(computeLayout({ w: 400, h: 560 }, false).wide).toBe(false);
  });

  it("칸은 18 아래로 줄지 않는다", () => {
    expect(computeLayout({ w: 100, h: 100 }).cell).toBe(18);
  });

  it("높이가 모자라면 높이로 정해진다", () => {
    const l = computeLayout({ w: 600, h: 330 }, true);
    expect(l.cell).toBe(Math.floor((330 - 130) / 9));
  });
});

describe("slotSize · sellRefund", () => {
  it("슬롯은 20..40", () => {
    expect(slotSize(336, 4, true)).toBe(40);
    expect(slotSize(240, 12, true)).toBe(20);
  });
  it("환급 = floor(2^(n-1) × 0.5)", () => {
    expect(sellRefund(1, 0.5)).toBe(0);
    expect(sellRefund(2, 0.5)).toBe(1);
    expect(sellRefund(5, 0.5)).toBe(8);
  });
});

describe("orderMarks", () => {
  const chain = (c: string, l: number): Item => ({ kind: "chain", chain: c, level: l });
  const free = (c: string, l: number): Cell => ({ state: "free", item: chain(c, l) });
  const board: Cell[] = Array.from({ length: 63 }, () => ({ state: "empty" }) as Cell);

  it("보관함이 먼저, 그다음 행 우선 — web 은 제외", () => {
    const b = [...board];
    b[5] = { state: "web", item: chain("clean", 2) };
    b[9] = free("clean", 2);
    b[20] = free("clean", 2);
    const view = {
      board: b, inv: [null, chain("clean", 2)] as (Item | null)[],
      orders: [{ resident: "parrot", ready: true, wants: [{ chain: "clean", level: 2, have: true }] }],
    };
    expect([...orderMarks(view)]).toEqual([101]);
    view.inv = [null, null];
    expect([...orderMarks(view)]).toEqual([9]);
  });

  it("have 가 거짓이면 표시하지 않고, 한 주문의 두 요구는 다른 칸을 쓴다", () => {
    const b = [...board];
    b[1] = free("garden", 2);
    b[2] = free("garden", 2);
    const view = {
      board: b, inv: [] as (Item | null)[],
      orders: [{ resident: "bear", ready: false, wants: [
        { chain: "garden", level: 2, have: true }, { chain: "garden", level: 2, have: true }, { chain: "cook", level: 1, have: false },
      ] }],
    };
    expect([...orderMarks(view)].sort()).toEqual([1, 2]);
  });
});
