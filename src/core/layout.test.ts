import { describe, expect, it } from "vitest";
import { clampToBounds, groupMove, hitsInRect, marqueeRect, resizeBox, type Box } from "./layout";

const box = (id: string, x: number, y: number, w = 100, h = 100): Box => ({ id, x, y, w, h });

describe("marqueeRect", () => {
  it("normalizes a drag in any direction", () => {
    expect(marqueeRect(10, 10, 40, 30)).toEqual({ x: 10, y: 10, w: 30, h: 20 });
    // 왼쪽 위로 끌어도 같은 사각형이어야 한다
    expect(marqueeRect(40, 30, 10, 10)).toEqual({ x: 10, y: 10, w: 30, h: 20 });
  });
});

describe("hitsInRect", () => {
  const boxes = [box("a", 0, 0), box("b", 200, 0), box("c", 0, 200)];

  it("selects everything the rectangle touches", () => {
    expect(hitsInRect(boxes, { x: 0, y: 0, w: 250, h: 50 })).toEqual(["a", "b"]);
  });

  it("does not require fully enclosing a widget", () => {
    // 위젯보다 작은 사각형 — 감싸기를 요구하면 큰 위젯은 영영 못 고른다
    expect(hitsInRect(boxes, { x: 10, y: 10, w: 5, h: 5 })).toEqual(["a"]);
  });

  it("selects nothing on empty space", () => {
    expect(hitsInRect(boxes, { x: 150, y: 150, w: 10, h: 10 })).toEqual([]);
  });
});

describe("clampToBounds", () => {
  const bounds = { w: 1000, h: 800 };
  // 격자 4px — 아래 기대값들이 모두 격자 위에 떨어지도록 (격자 자체는 마지막 테스트에서 본다)

  it("leaves a widget that already fits alone (same object)", () => {
    const b = [box("a", 10, 10)];
    expect(clampToBounds(b, bounds)[0]).toBe(b[0]);
  });

  it("pulls a widget back from beyond the right/bottom edge", () => {
    // 더 넓은 화면에서 저장했다가 작은 화면에서 열었을 때
    expect(clampToBounds([box("a", 1800, 900)], bounds)[0]).toMatchObject({ x: 900, y: 700 });
  });

  it("shrinks a widget that is larger than the screen", () => {
    expect(clampToBounds([box("a", 0, 0, 2000, 1200)], bounds)[0]).toMatchObject({ x: 0, y: 0, w: 1000, h: 800 });
  });

  it("does nothing when the window size is not known yet", () => {
    const b = [box("a", 5000, 5000)];
    expect(clampToBounds(b, { w: 0, h: 0 })[0]).toBe(b[0]);
  });
});

describe("groupMove", () => {
  const bounds = { w: 1000, h: 1000 };

  it("moves every box by the same amount", () => {
    const boxes = [box("a", 100, 100), box("b", 300, 140)];
    expect(groupMove(boxes, "a", 50, 20, 1, bounds)).toEqual([
      { id: "a", x: 150, y: 120 },
      { id: "b", x: 350, y: 160 },
    ]);
  });

  it("snaps the dragged box to the grid and keeps the gaps", () => {
    const boxes = [box("a", 100, 100), box("b", 303, 100)];
    const moved = groupMove(boxes, "a", 19, 0, 8, bounds);
    expect(moved[0].x).toBe(120);            // 100+19 → 격자 8
    expect(moved[1].x - moved[0].x).toBe(203); // 간격은 그대로
  });

  it("stops the whole group when one box reaches the edge", () => {
    const boxes = [box("a", 100, 100), box("b", 880, 100)];
    const moved = groupMove(boxes, "a", 500, 0, 1, bounds);
    expect(moved[1].x).toBe(900);             // b 가 오른쪽 벽에 딱
    expect(moved[0].x).toBe(120);             // a 도 같은 만큼만 (간격 유지)
  });

  it("does not let the group leave the top-left corner", () => {
    const boxes = [box("a", 40, 40), box("b", 200, 200)];
    const moved = groupMove(boxes, "b", -500, -500, 1, bounds);
    expect(moved[0]).toEqual({ id: "a", x: 0, y: 0 });
    expect(moved[1]).toEqual({ id: "b", x: 160, y: 160 });
  });

  it("ignores a missing anchor", () => {
    expect(groupMove([box("a", 0, 0)], "nope", 10, 10, 8, bounds)).toEqual([]);
  });
});

describe("resizeBox", () => {
  const box = { x: 100, y: 100, w: 200, h: 100 };
  const min = { w: 80, h: 40 };
  const bounds = { w: 1000, h: 800 };
  // 격자 4px — 아래 기대값들이 모두 격자 위에 떨어지도록 (격자 자체는 마지막 테스트에서 본다)

  it("오른쪽/아래로 끌면 좌표는 그대로고 크기만 는다", () => {
    expect(resizeBox(box, "se", 40, 20, min, 4, bounds)).toEqual({ x: 100, y: 100, w: 240, h: 120 });
  });

  it("왼쪽/위로 끌면 반대쪽 변은 제자리에 남는다", () => {
    const r = resizeBox(box, "nw", -40, -20, min, 4, bounds);
    expect(r).toEqual({ x: 60, y: 80, w: 240, h: 120 });
    expect(r.x + r.w).toBe(box.x + box.w);
    expect(r.y + r.h).toBe(box.y + box.h);
  });

  it("한 변만 끌면 다른 축은 건드리지 않는다", () => {
    expect(resizeBox(box, "e", 40, 999, min, 4, bounds)).toEqual({ ...box, w: 240 });
    expect(resizeBox(box, "n", 999, -40, min, 4, bounds)).toEqual({ x: 100, y: 60, w: 200, h: 140 });
  });

  it("최소 크기 아래로는 줄지 않고, 그때도 반대쪽 변은 움직이지 않는다", () => {
    const r = resizeBox(box, "w", 500, 0, min, 4, bounds);
    expect(r.w).toBe(min.w);
    expect(r.x + r.w).toBe(box.x + box.w);
  });

  it("작업영역 밖으로는 못 나간다", () => {
    expect(resizeBox(box, "se", 5000, 5000, min, 4, bounds)).toEqual({ x: 100, y: 100, w: 900, h: 700 });
    expect(resizeBox(box, "nw", -5000, -5000, min, 4, bounds)).toEqual({ x: 0, y: 0, w: 300, h: 200 });
  });

  it("격자는 끄는 변의 좌표에 건다 — 반대쪽 변은 격자에서 떨어져 있어도 그대로", () => {
    const odd = { x: 103, y: 100, w: 200, h: 100 };
    const r = resizeBox(odd, "e", 5, 0, min, 8, bounds);
    expect(r.x).toBe(103);          // 왼쪽 변은 건드리지 않는다
    expect((r.x + r.w) % 8).toBe(0); // 끈 쪽만 격자에 맞는다
  });
});
