import { useLayoutEffect, useRef, useState, useEffect, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { useSettings, type Rect } from "../core/settings";
import { useDismiss } from "../core/dismiss";
import "./ContextMenu.css";

export interface ContextMenuItem { label: string; onSelect: () => void }

/**
 * 커서 위치에 뜨는 우클릭 메뉴. body 로 portal 해서 위젯 overflow 에 갇히지 않고,
 * 창 안으로 클램프한 사각형을 히트 영역으로 등록해 클릭이 바탕화면으로 새지 않게 한다.
 * 메뉴 밖을 누르거나(바탕화면 포함) Esc 면 닫힌다.
 */
export function ContextMenu({ id, parentScope, at, items, onClose, accent }: {
  id: string;
  /** 메뉴를 연 팝업의 dismiss scope — 메뉴 클릭이 그 팝업을 닫지 않게 한다. */
  parentScope?: string;
  at: { x: number; y: number };
  items: ContextMenuItem[];
  onClose: () => void;
  /** 연 위젯의 인스턴스별 강조색. portal 로 body 에 그려지므로 별도로 전달받아야 CSS 변수를 물려받는다. */
  accent?: string;
}) {
  const setOverlayRect = useSettings((s) => s.setOverlayRect);
  const ref = useRef<HTMLDivElement>(null);
  const [rect, setRect] = useState<Rect | null>(null);

  useDismiss(id, true, onClose);

  useLayoutEffect(() => {
    const w = ref.current?.offsetWidth || 160;
    const h = ref.current?.offsetHeight || 64;
    setRect({
      x: Math.min(at.x, Math.max(0, window.innerWidth - w)),
      y: Math.min(at.y, Math.max(0, window.innerHeight - h)),
      w, h,
    });
  }, [at.x, at.y, items.length]);

  useEffect(() => {
    if (!rect) return;
    setOverlayRect(id, rect);
    return () => setOverlayRect(id, null);
  }, [id, rect, setOverlayRect]);

  return createPortal(
    <div ref={ref} data-dismiss-scope={parentScope ? `${id} ${parentScope}` : id} className="ctx-menu"
      style={{ left: (rect ?? at).x, top: (rect ?? at).y, ...(accent ? { "--accent": accent } as CSSProperties : {}) }}
      onContextMenu={(e) => e.preventDefault()}>
      {items.map((it) => (
        <button key={it.label} onClick={() => { onClose(); it.onSelect(); }}>{it.label}</button>
      ))}
    </div>,
    document.body,
  );
}
