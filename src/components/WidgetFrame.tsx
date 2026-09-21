import { useEffect, useRef, useState, type CSSProperties, type PointerEvent as RPointerEvent, type ReactNode } from "react";
import { contentScale, useSettings, type WidgetInstance } from "../core/settings";
import { groupMove, resizeBox, type Box, type ResizeDir } from "../core/layout";
import { widgetById } from "../widgets/registry";
import "./WidgetFrame.css";

/**
 * 둥근 카드 프레임. 편집 모드에서 헤더 드래그로 이동, **테두리 어디로든** 크기 조절.
 *
 * 크기 조절 손잡이는 네 변 + 네 모서리 여덟 개다. 위/왼쪽을 끌면 반대쪽 변을 고정한 채
 * `x`/`y` 까지 함께 바뀐다 (`core/layout::resizeBox`).
 *
 * 헤더 드래그는 **선택된 위젯 전부**를 함께 옮긴다 (Canvas 의 마키나 Ctrl/Shift 클릭으로 고른다).
 * 선택에 없는 위젯을 그냥 끌면 그 하나만 선택되고 움직인다 — 한 개짜리 묶음일 뿐이라 경로가 하나다.
 *
 * 내용 배율(CSS zoom) = 비율 배율 × 넘침 보정.
 * - 비율 배율: 기본 크기 대비 (autoScale 이 꺼져 있으면 1)
 * - 넘침 보정: 렌더 후 body 의 scroll/client 크기를 재서 내용이 넘치면 그만큼 줄인다 → 잘리지 않음 보장
 * children 은 `(innerSize) => ReactNode` — 배율을 뺀 실제 레이아웃 크기를 받는다.
 */
/** 변이 먼저, 모서리가 나중 — 나중에 그린 것이 위에 온다. */
const RESIZE_DIRS: ResizeDir[] = ["n", "s", "e", "w", "nw", "ne", "sw", "se"];

export function WidgetFrame({ inst, children }: { inst: WidgetInstance; children: (inner: { w: number; h: number }) => ReactNode }) {
  const def = widgetById(inst.widgetId)!;
  const locked = useSettings((s) => s.locked);
  const snap = useSettings((s) => s.gridSnap);
  const autoScale = useSettings((s) => s.autoScale);
  const moveResize = useSettings((s) => s.moveResize);
  const moveMany = useSettings((s) => s.moveMany);
  const selection = useSettings((s) => s.selection);
  const setSelection = useSettings((s) => s.setSelection);
  const toggleSelected = useSettings((s) => s.toggleSelected);
  const openSettings = useSettings((s) => s.openSettings);
  const removeWidget = useSettings((s) => s.removeWidget);
  const drag = useRef<{ mode: "move" | ResizeDir; sx: number; sy: number; box: Box; boxes: Box[] } | null>(null);
  const selected = selection.includes(inst.id);

  const ratio = contentScale(inst, autoScale);
  const [fit, setFit] = useState(1);
  const zoom = Math.max(0.4, Math.round(ratio * fit * 100) / 100);

  // 내용 여백은 카드 크기를 따라간다 — 56px 짜리 아이콘 위젯에 12px 를 양쪽으로 주면
  // 쓸 수 있는 자리가 절반 이하로 줄어든다. 작은 카드는 얇게, 큰 카드는 넉넉하게.
  //
  // 배율로 나누는 이유: 여백은 body **안**에 있어 `zoom` 과 함께 커진다. 눈에 보이는 여백을
  // 일정하게 두려면 배율을 미리 빼 둬야 한다 — 안 그러면 크게 키운 위젯만 테두리가 뚱뚱해진다.
  const padVisual = Math.max(6, Math.min(14, Math.round(Math.min(inst.w, inst.h) * 0.07)));
  const pad = Math.max(4, Math.round(padVisual / zoom));

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
        const w = el.clientWidth - pad * 2, h = el.clientHeight - pad * 2;
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
  }, [ratio, zoom, pad]);
  // 크기/설정이 바뀌면 보정을 초기화하고 다시 잰다
  useEffect(() => { setFit(1); }, [inst.w, inst.h, inst.settings]);

  const onDown = (mode: "move" | ResizeDir) => (e: RPointerEvent) => {
    if (locked || e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    e.stopPropagation();

    let ids = [inst.id];
    if (mode === "move") {
      // Ctrl/Shift 클릭은 묶음에 넣고 빼기만 한다 — 그 자리에서 끌기까지 시작하면
      // 방금 뺀 위젯이 따라 움직여 무엇을 골랐는지 알 수 없게 된다.
      if (e.shiftKey || e.ctrlKey || e.metaKey) { toggleSelected(inst.id); return; }
      if (!selected) setSelection([inst.id]);
      else if (selection.length > 1) ids = selection;
    }
    // 원위치는 **시작할 때** 찍어 둔다. 매 프레임 현재 좌표에 더하면 격자 맞춤의
    // 반올림 오차가 쌓여 묶음이 서서히 어긋난다.
    const boxes = useSettings.getState().instances
      .filter((i) => ids.includes(i.id))
      .map((i) => ({ id: i.id, x: i.x, y: i.y, w: i.w, h: i.h }));

    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { mode, sx: e.clientX, sy: e.clientY, box: { id: inst.id, x: inst.x, y: inst.y, w: inst.w, h: inst.h }, boxes };
  };
  const onMove = (e: RPointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = e.clientX - d.sx, dy = e.clientY - d.sy;
    const bounds = { w: window.innerWidth, h: window.innerHeight };
    if (d.mode === "move") moveMany(groupMove(d.boxes, inst.id, dx, dy, snap, bounds));
    else moveResize(inst.id, resizeBox(d.box, d.mode, dx, dy, def.minSize, snap, bounds));
  };
  const onUp = () => { drag.current = null; };

  const bodyStyle: CSSProperties & { zoom?: number } = zoom !== 1 ? { zoom } : {};

  return (
    <div
      className={`widget ${locked ? "" : "editing"}${!locked && selected ? " selected" : ""}`}
      style={{
        left: inst.x, top: inst.y, width: inst.w, height: inst.h,
        "--wpad": `${pad}px`,
        // 카드 뒤 배경화면을 정렬하는 데 쓴다 (WidgetFrame.css 의 .widget::before)
        "--wx": `${inst.x}px`, "--wy": `${inst.y}px`,
        ...(inst.accent ? { "--accent": inst.accent } : {}),
      } as CSSProperties}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {/* 제목줄은 없다. 편집 모드에서만 이동 핸들 + 설정/제거를 내용 위에 겹쳐 그린다. 평소 설정은 설정 위젯에서. */}
      {!locked && (
        <div className="widget-header" title={def.title} onPointerDown={onDown("move")} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
          <span className="widget-grip">⠿</span>
          {selected && selection.length > 1 && <span className="widget-group" title="함께 움직이는 위젯 수">⧉ {selection.length}</span>}
          <span className="widget-actions">
            {zoom !== 1 && <span className="widget-scale" title="내용 배율">{Math.round(zoom * 100)}%</span>}
            <button title="설정" onClick={() => openSettings(inst.id)}>⚙</button>
            {!def.singleton && <button title="제거" onClick={() => removeWidget(inst.id)}>✕</button>}
          </span>
        </div>
      )}
      <div className="widget-body" style={bodyStyle} ref={bodyRef}>{children(inner)}</div>
      {/* 크기 조절: 네 변 + 네 모서리. 모서리가 변보다 위에 와야 겹치는 자리에서 대각선이 이긴다. */}
      {!locked && RESIZE_DIRS.map((d) => (
        <div key={d} className={`widget-resize r-${d}`} title="크기 조절"
          onPointerDown={onDown(d)} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp} />
      ))}
    </div>
  );
}
