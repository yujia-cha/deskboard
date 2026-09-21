import { useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { useSettings } from "../core/settings";
import { hitsInRect, marqueeRect, type Box } from "../core/layout";
import { widgetById } from "../widgets/registry";
import { WidgetFrame } from "./WidgetFrame";

/** 클릭과 선택 드래그를 가르는 거리(px). 이보다 짧으면 "빈 곳 클릭" = 선택 해제. */
const DRAG_SLOP = 4;

/**
 * 모든 위젯 인스턴스를 절대 좌표(작업영역 px)로 배치. 창이 작업영역 전체를 덮는다.
 *
 * 편집 모드에서는 빈 곳을 끌어 **선택 사각형(마키)** 을 그린다 — 닿은 위젯이 모두 선택되고,
 * 그중 하나를 끌면 함께 움직인다 (`WidgetFrame` + `core/layout::groupMove`).
 * 잠금 상태에서는 이 창이 애초에 빈 영역의 클릭을 바탕화면으로 흘려보내므로 아무 일도 없다.
 */
export function Canvas() {
  const instances = useSettings((s) => s.instances);
  const locked = useSettings((s) => s.locked);
  const selection = useSettings((s) => s.selection);
  const setSelection = useSettings((s) => s.setSelection);
  const clearSelection = useSettings((s) => s.clearSelection);

  const [band, setBand] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  // 드래그 중 계속 읽되 리렌더를 일으키지 않아야 하는 값들
  const drag = useRef<{ sx: number; sy: number; base: string[]; moved: boolean } | null>(null);

  const boxes = (): Box[] => instances.map((i) => ({ id: i.id, x: i.x, y: i.y, w: i.w, h: i.h }));

  const onPointerDown = (e: RPointerEvent) => {
    if (locked || e.button !== 0) return;
    // 위젯 위에서 시작한 드래그는 WidgetFrame 이 이미 가로챈다 (stopPropagation).
    const additive = e.shiftKey || e.ctrlKey || e.metaKey;
    drag.current = { sx: e.clientX, sy: e.clientY, base: additive ? selection : [], moved: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: RPointerEvent) => {
    const d = drag.current;
    if (!d) return;
    if (!d.moved && Math.abs(e.clientX - d.sx) < DRAG_SLOP && Math.abs(e.clientY - d.sy) < DRAG_SLOP) return;
    d.moved = true;
    const rect = marqueeRect(d.sx, d.sy, e.clientX, e.clientY);
    setBand(rect);
    // 끄는 동안 바로 보여준다 — 손을 떼고 나서야 무엇이 잡혔는지 알면 다시 끌게 된다.
    const next = [...new Set([...d.base, ...hitsInRect(boxes(), rect)])];
    // 같은 목록이면 상태를 건드리지 않는다 — 매 포인터 이벤트마다 모든 위젯을 다시 그리게 된다.
    const cur = useSettings.getState().selection;
    if (next.length !== cur.length || next.some((id, i) => id !== cur[i])) setSelection(next);
  };

  const onPointerUp = () => {
    const d = drag.current;
    drag.current = null;
    setBand(null);
    if (d && !d.moved) clearSelection(); // 빈 곳 클릭 = 선택 해제
  };

  return (
    <div
      className={`canvas ${locked ? "" : "editing"}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    >
      {instances.map((inst) => {
        const def = widgetById(inst.widgetId);
        if (!def) return null;
        const C = def.component;
        return (
          <WidgetFrame key={inst.id} inst={inst}>
            {(inner) => <C instanceId={inst.id} settings={inst.settings} size={inner} editing={!locked} />}
          </WidgetFrame>
        );
      })}
      {band && <div className="marquee" style={{ left: band.x, top: band.y, width: band.w, height: band.h }} />}
    </div>
  );
}
