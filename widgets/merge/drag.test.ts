import { describe, expect, it } from "vitest";
import { cellAt, dropTargetAt, slotAt } from "./drag";

const COLS = 7, ROWS = 9;
/** 레이아웃 px 의 사각형을 배율 z 로 줄이거나 키운 화면 사각형 */
const rectAt = (z: number, left = 100, top = 50, w = 350, h = 450) => ({ left, top, width: w * z, height: h * z });
const centerOf = (r: { left: number; top: number; width: number; height: number }, row: number, col: number, cols = COLS, rows = ROWS) => ({
  x: r.left + ((col + 0.5) / cols) * r.width,
  y: r.top + ((row + 0.5) / rows) * r.height,
});

describe("cellAt — CSS zoom 에 상관없이 같은 칸", () => {
  for (const z of [0.6, 1.0, 1.4]) {
    it(`zoom ${z}: 모든 칸의 중심`, () => {
      const r = rectAt(z);
      for (let row = 0; row < ROWS; row++)
        for (let col = 0; col < COLS; col++) {
          const { x, y } = centerOf(r, row, col);
          expect(cellAt(r, COLS, ROWS, x, y)).toBe(row * COLS + col);
        }
    });
  }

  it("모서리: 왼쪽 위는 0, 오른쪽 아래 끝 픽셀 직전은 62", () => {
    const r = rectAt(1);
    expect(cellAt(r, COLS, ROWS, r.left, r.top)).toBe(0);
    expect(cellAt(r, COLS, ROWS, r.left + r.width - 0.01, r.top + r.height - 0.01)).toBe(62);
    expect(cellAt(r, COLS, ROWS, r.left + r.width - 0.01, r.top)).toBe(6);
    expect(cellAt(r, COLS, ROWS, r.left, r.top + r.height - 0.01)).toBe(56);
  });

  it("바깥(오른쪽·아래 경계 포함)은 null", () => {
    const r = rectAt(1);
    expect(cellAt(r, COLS, ROWS, r.left - 0.01, r.top + 10)).toBeNull();
    expect(cellAt(r, COLS, ROWS, r.left + 10, r.top - 0.01)).toBeNull();
    expect(cellAt(r, COLS, ROWS, r.left + r.width, r.top + 10)).toBeNull();
    expect(cellAt(r, COLS, ROWS, r.left + 10, r.top + r.height)).toBeNull();
  });

  it("칸 경계는 다음 칸에 속한다", () => {
    const r = rectAt(1);
    expect(cellAt(r, COLS, ROWS, r.left + r.width / COLS, r.top + 1)).toBe(1);
  });

  it("크기가 0 인 사각형은 null", () => {
    expect(cellAt({ left: 0, top: 0, width: 0, height: 0 }, COLS, ROWS, 0, 0)).toBeNull();
  });
});

describe("slotAt · dropTargetAt", () => {
  const inv = (z: number) => ({ left: 100, top: 700 * z, width: 160 * z, height: 40 * z });

  it("보관함 4칸 — 비율로 슬롯을 고른다 (zoom 0.6/1/1.4)", () => {
    for (const z of [0.6, 1.0, 1.4]) {
      const r = inv(z);
      for (let i = 0; i < 4; i++) {
        const { x, y } = centerOf(r, 0, i, 4, 1);
        expect(slotAt(r, 4, x, y)).toBe(i);
      }
    }
  });

  it("보드는 칸 번호, 보관함은 100+슬롯, 그 밖은 null", () => {
    for (const z of [0.6, 1.0, 1.4]) {
      const board = rectAt(z);
      const iv = inv(z);
      const b = centerOf(board, 2, 3);
      expect(dropTargetAt(board, COLS, ROWS, iv, 4, b.x, b.y)).toBe(2 * COLS + 3);
      const s = centerOf(iv, 0, 2, 4, 1);
      expect(dropTargetAt(board, COLS, ROWS, iv, 4, s.x, s.y)).toBe(102);
      expect(dropTargetAt(board, COLS, ROWS, iv, 4, 5, 5)).toBeNull();
      // 보관함 오른쪽 끝(슬롯 밖)
      expect(dropTargetAt(board, COLS, ROWS, iv, 4, iv.left + iv.width + 1, iv.top + 2)).toBeNull();
    }
  });

  it("보관함이 없으면 보드 밖은 null", () => {
    const board = rectAt(1);
    expect(dropTargetAt(board, COLS, ROWS, null, 0, board.left + 5, board.top + board.height + 10)).toBeNull();
  });

  it("슬롯이 12개일 때 마지막 칸 = 111", () => {
    const iv = { left: 0, top: 0, width: 120, height: 10 };
    expect(dropTargetAt(rectAt(1, 500, 500), COLS, ROWS, iv, 12, 119.9, 5)).toBe(111);
  });
});
