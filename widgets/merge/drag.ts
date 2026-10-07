/**
 * 포인터 좌표 → 칸. DOM 없이 테스트한다.
 *
 * WidgetFrame 은 내용을 CSS `zoom` 으로 줄인다. 화면 좌표(`clientX`)와 `getBoundingClientRect()` 는
 * 같은 좌표계라서, 칸 크기를 px 로 가정하지 않고 **사각형에 대한 비율**로 계산하면 배율이 상쇄된다.
 */
export interface RectLike { left: number; top: number; width: number; height: number }

/** 격자 안의 칸 번호(idx = row*cols + col). 밖이면 null. */
export function cellAt(rect: RectLike, cols: number, rows: number, clientX: number, clientY: number): number | null {
  if (rect.width <= 0 || rect.height <= 0) return null;
  const fx = (clientX - rect.left) / rect.width;
  const fy = (clientY - rect.top) / rect.height;
  if (fx < 0 || fx >= 1 || fy < 0 || fy >= 1) return null;
  return Math.floor(fy * rows) * cols + Math.floor(fx * cols);
}

/** 가로 한 줄 보관함의 칸 (0..slots-1). 밖이면 null. */
export function slotAt(rect: RectLike, slots: number, clientX: number, clientY: number): number | null {
  if (slots <= 0) return null;
  return cellAt(rect, slots, 1, clientX, clientY);
}

/** 보드 칸 번호, 보관함이면 100+슬롯, 어느 쪽도 아니면 null. */
export function dropTargetAt(
  boardRect: RectLike, cols: number, rows: number,
  invRect: RectLike | null, invSlots: number,
  clientX: number, clientY: number,
): number | null {
  const cell = cellAt(boardRect, cols, rows, clientX, clientY);
  if (cell !== null) return cell;
  if (!invRect) return null;
  const slot = slotAt(invRect, invSlots, clientX, clientY);
  return slot === null ? null : 100 + slot;
}
