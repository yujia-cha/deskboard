import type { MergeView } from "./types";

export const COLS = 7;
export const ROWS = 9;
/** 예약 높이(px) — 합이 모자라면 칸이 줄고, 18 아래로는 줄이지 않는다. */
export const HEADER_H = 24;
export const ORDERS_H = 44;
export const INFO_H = 22;
export const STORAGE_H = 40;
/** 좁은 배치: 보드 아래 가로 줄 높이 / 넓은 배치: 보드 오른쪽 세로 줄 폭 */
export const CLICKER_ROW_H = 72;
export const CLICKER_COL_W = 64;
export const MIN_CELL = 18;

export interface Layout {
  wide: boolean;
  cell: number;
  /** 보드 격자 크기 (cell*7, cell*9) */
  gridW: number;
  gridH: number;
  /** 보드·정보·보관함이 쓰는 왼쪽 열의 폭 */
  areaW: number;
  /** 위젯 왼쪽 위 기준 격자 좌표 */
  boardLeft: number;
  boardTop: number;
  /** 짧은 변 < 280 이면 정보 줄 글자를 숨긴다 */
  compact: boolean;
}

/** K-5: 폭 ≥ 높이×0.75 이면 클리커는 보드 오른쪽 세로 줄. */
export function isWide(size: { w: number; h: number }): boolean {
  return size.w >= size.h * 0.75;
}

export function computeLayout(size: { w: number; h: number }, wide: boolean = isWide(size)): Layout {
  const areaW = wide ? size.w - CLICKER_COL_W : size.w;
  const reserved = HEADER_H + ORDERS_H + INFO_H + STORAGE_H + (wide ? 0 : CLICKER_ROW_H);
  const boardH = size.h - reserved;
  const cell = Math.max(MIN_CELL, Math.floor(Math.min(areaW / COLS, boardH / ROWS)));
  const gridW = cell * COLS;
  return {
    wide,
    cell,
    gridW,
    gridH: cell * ROWS,
    areaW,
    boardLeft: Math.max(0, Math.floor((areaW - gridW) / 2)),
    boardTop: HEADER_H + ORDERS_H,
    compact: Math.min(size.w, size.h) < 280,
  };
}

/** 보관함 슬롯 한 칸의 폭 (칸들은 맞붙는다). 확장 버튼 자리를 빼고 나눈다. */
export function slotSize(areaW: number, invSlots: number, hasExpand: boolean): number {
  const room = areaW - (hasExpand ? 64 : 8);
  return Math.max(20, Math.min(STORAGE_H, Math.floor(room / Math.max(1, invSlots))));
}

/** M-3: 환급 = floor(2^(level-1) × refund) */
export function sellRefund(level: number, ratio: number): number {
  return Math.floor(Math.pow(2, Math.max(0, level - 1)) * ratio);
}

/**
 * O-4: 주문이 요구한 아이템이 free 로 있으면 그 칸에 ✔. 백엔드와 같은 순서(보관함 먼저, 그다음 보드 행우선)로
 * 첫 번째 free 칸을 고른다. `wants[].have` 가 참인 것만 찾고, 한 주문 안에서는 같은 칸을 두 번 쓰지 않는다.
 * 칸 번호: 보드 0..62, 보관함 100+i.
 */
export function orderMarks(view: Pick<MergeView, "board" | "inv" | "orders">): Set<number> {
  const marks = new Set<number>();
  const candidates: { idx: number; chain: string; level: number }[] = [];
  view.inv.forEach((it, i) => {
    if (it && it.kind === "chain") candidates.push({ idx: 100 + i, chain: it.chain, level: it.level });
  });
  view.board.forEach((c, i) => {
    if (c.state === "free" && c.item.kind === "chain") candidates.push({ idx: i, chain: c.item.chain, level: c.item.level });
  });
  for (const o of view.orders) {
    const used = new Set<number>();
    for (const w of o.wants) {
      if (!w.have) continue;
      const hit = candidates.find((c) => !used.has(c.idx) && c.chain === w.chain && c.level === w.level);
      if (hit) { used.add(hit.idx); marks.add(hit.idx); }
    }
  }
  return marks;
}
