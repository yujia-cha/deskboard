import { useRef, type CSSProperties, type PointerEvent as RPointerEvent, type ReactNode } from "react";
import { contentScale, useSettings, type WidgetInstance } from "../core/settings";
import { widgetById } from "../widgets/registry";
import "./WidgetFrame.css";

/**
 * 둥근 카드 프레임. 편집 모드에서 헤더 드래그로 이동, 우하단 핸들로 크기 조절.
 * autoScale 이 켜져 있으면 내용(body)을 기본 크기 대비 배율로 확대/축소한다 (CSS zoom).
 * children 은 `(innerSize) => ReactNode` — 배율을 뺀 실제 레이아웃 크기를 받는다.
 */
export function WidgetFrame({ inst, children }: { inst: WidgetInstance; children: (inner: { w: number; h: number }) => ReactNode }) {
  const def = widgetById(inst.widgetId)!;
  const locked = useSettings((s) => s.locked);
  const snap = useSettings((s) => s.gridSnap);
  const autoScale = useSettings((s) => s.autoScale);
  const moveResize = useSettings((s) => s.moveResize);
  const openSettings = useSettings((s) => s.openSettings);
  const removeWidget = useSettings((s) => s.removeWidget);
  const drag = useRef<{ mode: "move" | "resize"; sx: number; sy: number; ox: number; oy: number; ow: number; oh: number } | null>(null);

  const scale = contentScale(inst, autoScale);
  const HEADER = def.chromeless ? 0 : 29; // 헤더 높이(px)
  const PAD = 12;
  const inner = { w: (inst.w - PAD * 2) / scale, h: (inst.h - HEADER - PAD * 2) / scale };

  const snapTo = (v: number) => Math.max(0, Math.round(v / snap) * snap);

  const onDown = (mode: "move" | "resize") => (e: RPointerEvent) => {
    if (locked || e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { mode, sx: e.clientX, sy: e.clientY, ox: inst.x, oy: inst.y, ow: inst.w, oh: inst.h };
  };
  const onMove = (e: RPointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = e.clientX - d.sx, dy = e.clientY - d.sy;
    if (d.mode === "move") moveResize(inst.id, { x: snapTo(d.ox + dx), y: snapTo(d.oy + dy) });
    else moveResize(inst.id, {
      w: Math.max(def.minSize.w, snapTo(d.ow + dx)),
      h: Math.max(def.minSize.h, snapTo(d.oh + dy)),
    });
  };
  const onUp = () => { drag.current = null; };

  const bodyStyle: CSSProperties & { zoom?: number } = scale !== 1 ? { zoom: scale } : {};

  return (
    <div
      className={`widget ${locked ? "" : "editing"} ${def.chromeless ? "chromeless" : ""}`}
      style={{ left: inst.x, top: inst.y, width: inst.w, height: inst.h }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <div className="widget-header" onPointerDown={onDown("move")} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
        <span className="widget-title">{def.icon ? `${def.icon} ` : ""}{def.title}</span>
        <span className="widget-actions">
          {scale !== 1 && <span className="widget-scale" title="내용 배율">{Math.round(scale * 100)}%</span>}
          <button title="설정" onClick={() => openSettings(inst.id)}>⚙</button>
          <button title="제거" onClick={() => removeWidget(inst.id)}>✕</button>
        </span>
      </div>
      <div className="widget-body" style={bodyStyle}>{children(inner)}</div>
      {!locked && <div className="widget-resize" onPointerDown={onDown("resize")} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp} />}
    </div>
  );
}
