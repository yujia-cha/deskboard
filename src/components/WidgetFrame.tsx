import { useEffect, useRef, useState, type CSSProperties, type PointerEvent as RPointerEvent, type ReactNode } from "react";
import { contentScale, useSettings, type WidgetInstance } from "../core/settings";
import { widgetById } from "../widgets/registry";
import "./WidgetFrame.css";

/**
 * 둥근 카드 프레임. 편집 모드에서 헤더 드래그로 이동, 우하단 핸들로 크기 조절.
 *
 * 내용 배율(CSS zoom) = 비율 배율 × 넘침 보정.
 * - 비율 배율: 기본 크기 대비 (autoScale 이 꺼져 있으면 1)
 * - 넘침 보정: 렌더 후 body 의 scroll/client 크기를 재서 내용이 넘치면 그만큼 줄인다 → 잘리지 않음 보장
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

  const ratio = contentScale(inst, autoScale);
  const [fit, setFit] = useState(1);
  const zoom = Math.max(0.4, Math.round(ratio * fit * 100) / 100);

  // body 실측: inner 크기(배율 제외 레이아웃 px) + 넘침 보정
  const bodyRef = useRef<HTMLDivElement>(null);
  const [inner, setInner] = useState({ w: inst.w / zoom, h: inst.h / zoom });
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    let raf = 0;
    const measure = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        // 매초 바뀌는 위젯(시계 등)의 DOM 변경마다 호출되므로 값이 같으면 상태를 건드리지 않는다
        const w = el.clientWidth - 24, h = el.clientHeight - 24;
        setInner((prev) => (prev.w === w && prev.h === h ? prev : { w, h }));
        const over = Math.max(el.scrollWidth / Math.max(1, el.clientWidth), el.scrollHeight / Math.max(1, el.clientHeight));
        setFit((f) => {
          if (over > 1.01) return Math.max(0.5 / ratio, Math.round((f / over) * 1000) / 1000); // 넘침 → 축소
          if (over < 0.92 && f < 1) return Math.min(1, Math.round((f / over) * 1000) / 1000);  // 여유 → 회복
          return f;
        });
      });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    const mo = new MutationObserver(measure);
    mo.observe(el, { childList: true, subtree: true, characterData: true });
    return () => { cancelAnimationFrame(raf); ro.disconnect(); mo.disconnect(); };
  }, [ratio, zoom]);
  // 크기/설정이 바뀌면 보정을 초기화하고 다시 잰다
  useEffect(() => { setFit(1); }, [inst.w, inst.h, inst.settings]);

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

  const bodyStyle: CSSProperties & { zoom?: number } = zoom !== 1 ? { zoom } : {};

  return (
    <div
      className={`widget ${locked ? "" : "editing"}`}
      style={{ left: inst.x, top: inst.y, width: inst.w, height: inst.h, ...(inst.accent ? { "--accent": inst.accent } as CSSProperties : {}) }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {/* 제목줄은 없다. 편집 모드에서만 이동 핸들 + 설정/제거를 내용 위에 겹쳐 그린다. 평소 설정은 설정 위젯에서. */}
      {!locked && (
        <div className="widget-header" title={def.title} onPointerDown={onDown("move")} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
          <span className="widget-grip">⠿</span>
          <span className="widget-actions">
            {zoom !== 1 && <span className="widget-scale" title="내용 배율">{Math.round(zoom * 100)}%</span>}
            <button title="설정" onClick={() => openSettings(inst.id)}>⚙</button>
            {!def.singleton && <button title="제거" onClick={() => removeWidget(inst.id)}>✕</button>}
          </span>
        </div>
      )}
      <div className="widget-body" style={bodyStyle} ref={bodyRef}>{children(inner)}</div>
      {!locked && <div className="widget-resize" onPointerDown={onDown("resize")} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp} />}
    </div>
  );
}
