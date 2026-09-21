/**
 * 편집 모드의 배치 계산 — 선택 사각형(마키)과 여러 위젯 동시 이동.
 *
 * DOM 이 필요 없는 순수 함수라 여기서 테스트한다. WidgetFrame/Canvas 는 포인터 이벤트만 다룬다.
 */
import { overlaps, type Rect } from "./settings";

export interface Box extends Rect {
  id: string;
}

/** 드래그 시작점과 현재점으로 정규화된 사각형 (어느 방향으로 끌어도 양수 크기). */
export function marqueeRect(sx: number, sy: number, cx: number, cy: number): Rect {
  return { x: Math.min(sx, cx), y: Math.min(sy, cy), w: Math.abs(cx - sx), h: Math.abs(cy - sy) };
}

/**
 * 저장된 배치를 현재 작업영역 안으로 접는다.
 *
 * 위젯 좌표는 **그 모니터 작업영역의 픽셀**이다. 그래서 해상도가 다른 PC 로 설정을 옮기거나,
 * 모니터를 바꾸거나, 디스플레이 배율을 올리면 화면 밖에 남는 위젯이 생긴다. 편집 모드의 경계
 * 보정은 끌고 있는 동안에만 도는데 **보이지 않는 위젯은 끌 수가 없다** — 특히 설정 톱니(설정
 * 패널의 유일한 위젯 진입점, 삭제 불가)가 밖으로 나가면 스스로 되돌릴 방법이 없다.
 * 그래서 불러올 때와 창 크기가 바뀔 때 한 번씩 접는다.
 *
 * 바뀌지 않은 항목은 **같은 객체 그대로** 돌려준다 — 호출한 쪽이 "달라졌나"를 참조로 물을 수 있다.
 */
export function clampToBounds<T extends Box>(boxes: T[], bounds: { w: number; h: number }): T[] {
  if (!(bounds.w > 0) || !(bounds.h > 0)) return boxes; // 창 크기를 아직 모른다 — 건드리지 않는다
  return boxes.map((b) => {
    const w = Math.max(1, Math.min(b.w, bounds.w));
    const h = Math.max(1, Math.min(b.h, bounds.h));
    const x = Math.max(0, Math.min(b.x, bounds.w - w));
    const y = Math.max(0, Math.min(b.y, bounds.h - h));
    return b.x === x && b.y === y && b.w === w && b.h === h ? b : { ...b, x, y, w, h };
  });
}

/** 사각형에 **닿기만 해도** 선택 — 완전히 감싸야 하면 큰 위젯을 고를 수 없다. */
export function hitsInRect(boxes: Box[], rect: Rect): string[] {
  return boxes.filter((b) => overlaps(b, rect)).map((b) => b.id);
}

/**
 * 여러 위젯을 같은 만큼 옮긴다.
 *
 * - 격자 맞춤은 **끄는 위젯(anchor) 하나에만** 적용하고 그 차이를 전부에 똑같이 더한다.
 *   각자를 따로 맞추면 서로의 간격이 어긋나 배치가 무너진다.
 * - 경계 보정도 **묶음 전체**로 본다. 하나가 벽에 닿으면 전부 멈춘다 — 따로 잘라내면
 *   벽에 닿은 것만 뒤처져 역시 간격이 무너진다.
 */
export function groupMove(
  boxes: Box[],
  anchorId: string,
  dx: number,
  dy: number,
  snap: number,
  bounds: { w: number; h: number },
): { id: string; x: number; y: number }[] {
  const anchor = boxes.find((b) => b.id === anchorId);
  if (!anchor || boxes.length === 0) return [];
  const step = Math.max(1, snap);
  const snapped = (origin: number, d: number) => Math.round((origin + d) / step) * step - origin;

  const clampDelta = (d: number, axis: "x" | "y") => {
    const size = axis === "x" ? "w" : "h";
    const limit = axis === "x" ? bounds.w : bounds.h;
    const lo = -Math.min(...boxes.map((b) => b[axis]));                        // 더 왼쪽/위로는 못 간다
    const hi = Math.min(...boxes.map((b) => Math.max(0, limit - b[axis] - b[size]))); // 오른쪽/아래 여유
    return Math.min(hi, Math.max(lo, d));
  };

  const mx = clampDelta(snapped(anchor.x, dx), "x");
  const my = clampDelta(snapped(anchor.y, dy), "y");
  return boxes.map((b) => ({ id: b.id, x: b.x + mx, y: b.y + my }));
}

/** 크기 조절 손잡이의 방향. 네 변 + 네 모서리. */
export type ResizeDir = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

/**
 * 한 방향으로 끌어 사각형의 크기를 바꾼다.
 *
 * **움직이는 변만 옮긴다** — 반대쪽 변은 시작할 때의 자리에 그대로 둔다. 그래서 왼쪽/위를
 * 끌면 `x`/`y` 가 함께 줄고, 오른쪽/아래를 끌면 좌표는 그대로다.
 *
 * 격자 맞춤은 **끄는 변의 화면 좌표**에 건다. 크기(`w`)에 걸면 반대쪽 변이 격자에서 떨어져
 * 이웃 위젯과 줄이 맞지 않는다.
 *
 * 최소 크기(`min`)와 작업영역(`bounds`)은 변마다 잘라낸다 — 벽에 닿으면 그 변만 멈춘다.
 */
export function resizeBox(
  start: Rect,
  dir: ResizeDir,
  dx: number,
  dy: number,
  min: { w: number; h: number },
  snap: number,
  bounds: { w: number; h: number },
): Rect {
  const step = Math.max(1, snap);
  const fit = (v: number) => Math.round(v / step) * step;

  let { x, y, w, h } = start;
  const right = start.x + start.w, bottom = start.y + start.h;

  if (dir.includes("e")) {
    const r = Math.min(bounds.w, Math.max(start.x + min.w, fit(right + dx)));
    w = r - start.x;
  }
  if (dir.includes("w")) {
    const l = Math.max(0, Math.min(right - min.w, fit(start.x + dx)));
    x = l; w = right - l;
  }
  if (dir.includes("s")) {
    const b = Math.min(bounds.h, Math.max(start.y + min.h, fit(bottom + dy)));
    h = b - start.y;
  }
  if (dir.includes("n")) {
    const t = Math.max(0, Math.min(bottom - min.h, fit(start.y + dy)));
    y = t; h = bottom - t;
  }
  return { x, y, w, h };
}
