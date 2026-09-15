export interface Rect { x: number; y: number; w: number; h: number }

export interface DropCandidate {
  instanceId: string;
  /** 잠금 아이콘 사각형 */
  rect: Rect;
  /** 펼쳐진 팝업이 있으면 그 사각형 (없으면 null) */
  popupRect?: Rect | null;
}

const inside = (p: { x: number; y: number }, r: Rect) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;

/**
 * 드롭 좌표(창 logical px)로 대상 폴더 인스턴스를 찾는다.
 * 펼쳐진 팝업이 있으면 팝업 영역을 아이콘보다 먼저 검사한다.
 */
export function findDropTarget(point: { x: number; y: number }, candidates: DropCandidate[]): string | null {
  for (const c of candidates) {
    if (c.popupRect && inside(point, c.popupRect)) return c.instanceId;
  }
  for (const c of candidates) {
    if (inside(point, c.rect)) return c.instanceId;
  }
  return null;
}

/** Tauri 드래그&드롭 이벤트의 PhysicalPosition → 창 logical px. */
export function toLogical(pos: { x: number; y: number }, scaleFactor: number): { x: number; y: number } {
  return { x: pos.x / scaleFactor, y: pos.y / scaleFactor };
}
