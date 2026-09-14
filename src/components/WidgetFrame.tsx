import { useEffect, useRef, useState, type CSSProperties, type PointerEvent as RPointerEvent, type ReactNode } from "react";
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
  // body 의 실제 내용 영역을 실측한다 (CSS zoom 하에서 clientWidth 는 배율을 뺀 레이아웃 px).
  const bodyRef = useRef<HTMLDivElement>(null);
  const [inner, setInner] = useState({ w: inst.w / scale, h: inst.h / scale });
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const measure = () => setInner({ w: el.clientWidth - 24, h: el.clientHeight - 24 });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [scale]);

  const snapTo = (v: number) => Math.max(0, Math.round(v / snap) * snap);
  const clampX = (x: number, w: number) => Math.min(x, Math.max(0, window.innerWidth - w));
  const clampY = (y: number, h: number) => Math.min(y, Math.max(0, window.innerHeight - h));

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
    if (d.mode === "move") moveResize(inst.id, { x: clampX(snapTo(d.ox + dx), inst.w), y: clampY(snapTo(d.oy + dy), inst.h) });
    else moveResize(inst.id, {
      w: Math.min(Math.max(def.minSize.w, snapTo(d.ow + dx)), window.innerWidth - inst.x),
      h: Math.min(Math.max(def.minSize.h, snapTo(d.oh + dy)), window.innerHeight - inst.y),
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
      <div className="widget-body" style={bodyStyle} ref={bodyRef}>{children(inner)}</div>
      {!locked && <div className="widget-resize" onPointerDown={onDown("resize")} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp} />}
    </div>
  );
}
