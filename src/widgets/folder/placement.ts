export interface Rect { x: number; y: number; w: number; h: number }

/**
 * 앵커(폴더 아이콘) 기준 펼침 팝업 위치를 계산하는 순수 함수.
 * 아래 공간이 부족하고 위쪽이 더 넓으면 위로 펼치고, 결과는 항상 뷰포트 안으로 클램프한다.
 */
export function computePopupRect(
  anchor: Rect,
  content: { w: number; h: number },
  viewport: { w: number; h: number },
  gap = 8,
): Rect {
  const spaceBelow = viewport.h - (anchor.y + anchor.h) - gap;
  const spaceAbove = anchor.y - gap;
  const openUp = spaceBelow < content.h && spaceAbove > spaceBelow;

  const y = openUp ? anchor.y - gap - content.h : anchor.y + anchor.h + gap;
  const x = anchor.x;

  const maxX = Math.max(gap, viewport.w - content.w - gap);
  const maxY = Math.max(gap, viewport.h - content.h - gap);
  return {
    x: Math.min(Math.max(gap, x), maxX),
    y: Math.min(Math.max(gap, y), maxY),
    w: content.w,
    h: content.h,
  };
}
