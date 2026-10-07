import { useMemo, useRef, useState, type CSSProperties, type PointerEvent as RPointerEvent } from "react";
import type { Item, MergeContent, MergeView } from "./types";
import { Clicker } from "./Clicker";
import { dropTargetAt } from "./drag";
import { Icon, ItemView, isGenerator, itemName } from "./Icon";
import { CLICKER_COL_W, CLICKER_ROW_H, COLS, INFO_H, ROWS, STORAGE_H, orderMarks, sellRefund, slotSize, type Layout } from "./layout";

interface Props {
  view: MergeView;
  content: MergeContent | null;
  layout: Layout;
  editing: boolean;
  act: (cmd: string, args?: Record<string, unknown>) => Promise<boolean>;
  reload: () => Promise<void>;
  /** G-4: 에너지 부족 → 클리커 강조 */
  hint: boolean;
}

interface Press { from: number; draggable: boolean; sx: number; sy: number; active: boolean; pid: number }
type Confirm = { kind: "sell" | "box"; cell: number };

const DRAG_PX = 4;

export function Board({ view, content, layout, editing, act, reload, hint }: Props) {
  const { cell, wide } = layout;
  const rootRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const invRef = useRef<HTMLDivElement>(null);
  const press = useRef<Press | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [ghost, setGhost] = useState<{ x: number; y: number; item: Item } | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);

  const marks = useMemo(() => orderMarks(view), [view]);

  const itemAt = (i: number): Item | null => {
    if (i >= 100) return view.inv[i - 100] ?? null;
    const c = view.board[i];
    return c && c.state !== "empty" ? c.item : null;
  };
  const stateAt = (i: number) => (i >= 100 ? (view.inv[i - 100] ? "free" : "empty") : (view.board[i]?.state ?? "empty"));

  const select = (i: number | null) => { setSelected(i); setConfirm(null); };

  const targetAt = (cx: number, cy: number) => {
    const g = gridRef.current;
    if (!g) return null;
    const inv = invRef.current;
    return dropTargetAt(g.getBoundingClientRect(), COLS, ROWS, inv ? inv.getBoundingClientRect() : null, view.invSlots, cx, cy);
  };

  /** 포인터 → 레이아웃 px (merge-play 기준). CSS zoom 아래서도 사각형과의 비율로 맞는다. */
  const localPos = (cx: number, cy: number) => {
    const el = rootRef.current!;
    const r = el.getBoundingClientRect();
    return { x: ((cx - r.left) * el.clientWidth) / Math.max(1, r.width), y: ((cy - r.top) * el.clientHeight) / Math.max(1, r.height) };
  };

  const onDown = (e: RPointerEvent) => {
    if (editing || e.button !== 0) return;
    const t = targetAt(e.clientX, e.clientY);
    if (t === null) return;
    const st = stateAt(t);
    if (st === "empty") { select(null); return; }
    const draggable = st === "free";
    press.current = { from: t, draggable, sx: e.clientX, sy: e.clientY, active: false, pid: e.pointerId };
    if (draggable) e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onMove = (e: RPointerEvent) => {
    const p = press.current;
    if (!p || !p.draggable || e.pointerId !== p.pid) return;
    if (!p.active) {
      if (Math.hypot(e.clientX - p.sx, e.clientY - p.sy) <= DRAG_PX) return;
      p.active = true;
      setDragFrom(p.from);
    }
    const item = itemAt(p.from);
    if (item) setGhost({ ...localPos(e.clientX, e.clientY), item });
  };

  const end = () => { press.current = null; setGhost(null); setDragFrom(null); };

  const onUp = (e: RPointerEvent) => {
    const p = press.current;
    if (!p || e.pointerId !== p.pid) return;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
    end();
    if (p.active) {
      const to = targetAt(e.clientX, e.clientY);
      if (to !== null && to !== p.from) act("merge_move", { from: p.from, to }).then((ok) => { if (ok) select(to); });
      return; // 놓은 자리가 밖이면 제자리로 (아무것도 하지 않는다)
    }
    tap(p.from);
  };

  const tap = (i: number) => {
    const item = itemAt(i);
    select(i);
    if (!item) return;
    const st = stateAt(i);
    if (st === "box") { setConfirm({ kind: "box", cell: i }); return; }
    // 🎁 는 보관함에서도 꺼낼 수 있다(백엔드 pop_gift 가 가운데 근처 빈 칸에 놓는다). 생산기는 보드 위에서만.
    if (st === "free" && (item.kind === "gift" || (i < 100 && isGenerator(content, item)))) act("merge_tap", { cell: i });
  };

  const sel = selected !== null ? itemAt(selected) : null;
  const selState = selected !== null ? stateAt(selected) : "empty";
  const activeConfirm = confirm && confirm.cell === selected && sel ? confirm : null;

  // --- 칸 ---
  const cellStyle = { "--cell": `${cell}px` } as CSSProperties;
  const body = (i: number, item: Item | null, st: string) => {
    if (!item) return null;
    if (st === "box") return <Icon icon={content?.skins.box ?? { emoji: "📦" }} />;
    return (
      <>
        <ItemView content={content} item={item} />
        {st === "web" && <span className="merge-web"><Icon icon={content?.skins.web ?? { emoji: "🕸" }} /></span>}
        {marks.has(i) && <span className="merge-check">✔</span>}
      </>
    );
  };
  const cls = (i: number, st: string) =>
    `merge-cell st-${st}${selected === i ? " sel" : ""}${dragFrom === i ? " drag-src" : ""}`;

  const slot = slotSize(layout.areaW - layout.boardLeft, view.invSlots, view.invNextCost !== null);

  // --- 정보 줄 ---
  const info = () => {
    if (activeConfirm && sel) {
      if (activeConfirm.kind === "box") {
        return (
          <>
            <span className="merge-info-text">상자를 열까요? −{view.eco.boxOpenCost}⚡</span>
            <button className="merge-btn ok" onClick={() => { setConfirm(null); act("merge_open_box", { cell: activeConfirm.cell }); }}>열기</button>
            <button className="merge-btn" onClick={() => setConfirm(null)}>취소</button>
          </>
        );
      }
      return (
        <>
          <span className="merge-info-text">{itemName(content, sel)} 을(를) 팔까요?</span>
          <button className="merge-btn ok" onClick={() => { setConfirm(null); act("merge_sell", { cell: activeConfirm.cell }); }}>확인</button>
          <button className="merge-btn" onClick={() => setConfirm(null)}>취소</button>
        </>
      );
    }
    if (!sel || selected === null) return null;
    if (selState === "box") return <span className="merge-info-text">상자 — 눌러서 열기</span>;
    if (sel.kind === "gift") return <span className="merge-info-text">선물 — 눌러서 하나씩 꺼내기</span>;
    const ch = content?.chains[sel.chain];
    const next = ch && sel.level < ch.maxLevel ? ch.levels[sel.level]?.name : null;
    const sellable = selState === "free" && !isGenerator(content, sel);
    const refund = sellRefund(sel.level, view.eco.sellRefund);
    const askFirst = sel.level >= view.eco.sellConfirmFromLevel;
    return (
      <>
        <span className="merge-info-text">
          {itemName(content, sel)} Lv{sel.level} · {next ? `합치면 → ${next}` : "최고 레벨"}
        </span>
        {sellable && (
          <button
            className="merge-btn"
            onClick={() => (askFirst ? setConfirm({ kind: "sell", cell: selected }) : act("merge_sell", { cell: selected }))}
          >
            {refund > 0 ? `팔기 +${refund}⚡` : "버리기"}
          </button>
        )}
      </>
    );
  };

  const clicker = (
    <Clicker view={view} content={content} reload={reload} hint={hint} />
  );

  return (
    <div
      ref={rootRef}
      className={`merge-play ${wide ? "wide" : "narrow"}${layout.compact ? " compact" : ""}${editing ? " editing" : ""}`}
      style={cellStyle}
      tabIndex={0}
      data-capture-keys
      {...(editing ? {} : { onPointerDown: onDown, onPointerMove: onMove, onPointerUp: onUp, onPointerCancel: end })}
    >
      <div className="merge-col" style={{ width: layout.areaW }}>
        <div
          ref={gridRef}
          className="merge-grid"
          style={{ marginLeft: layout.boardLeft, width: layout.gridW, height: layout.gridH, gridTemplateColumns: `repeat(${COLS}, ${cell}px)` }}
        >
          {view.board.map((c, i) => {
            const item = c.state === "empty" ? null : c.item;
            return (
              <div key={i} data-idx={i} className={cls(i, c.state)}>
                <div className="merge-cell-in">{body(i, item, c.state)}</div>
              </div>
            );
          })}
        </div>

        <div className="merge-info" style={{ height: INFO_H, paddingLeft: layout.boardLeft }}>
          {info()}
          {view.pendingGifts > 0 && (
            <button className="merge-btn gift" onClick={() => act("merge_claim_gift")}>🎁 받기 ×{view.pendingGifts}</button>
          )}
        </div>

        <div className="merge-storage" style={{ height: STORAGE_H, paddingLeft: layout.boardLeft }}>
          <div ref={invRef} className="merge-slots" style={{ width: slot * view.invSlots }}>
            {Array.from({ length: view.invSlots }, (_, s) => {
              const i = 100 + s;
              const item = view.inv[s] ?? null;
              return (
                <div key={s} data-idx={i} className={cls(i, item ? "free" : "empty")} style={{ width: slot, height: STORAGE_H }}>
                  <div className="merge-cell-in">{body(i, item, "free")}</div>
                </div>
              );
            })}
          </div>
          {view.invNextCost !== null && (
            <button className="merge-btn" title="보관함 칸 늘리기" onClick={() => act("merge_inv_expand")}>+{view.invNextCost}⚡</button>
          )}
        </div>

        {!wide && <div className="merge-clicker-row" style={{ height: CLICKER_ROW_H }}>{clicker}</div>}
      </div>

      {wide && <div className="merge-clicker-col" style={{ width: CLICKER_COL_W, height: layout.gridH }}>{clicker}</div>}

      {ghost && (
        <div className="merge-ghost" style={{ width: cell, height: cell, transform: `translate(${ghost.x - cell / 2}px, ${ghost.y - cell / 2}px)` }}>
          <ItemView content={content} item={ghost.item} />
        </div>
      )}
    </div>
  );
}
