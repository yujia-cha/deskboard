import { useCallback, useEffect, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
import { dropTarget, moveBefore } from "./reorder";
import "./Notes.css";

export interface NotesSettings extends Record<string, unknown> {
  title: string;
  showDone: boolean;
  strikeDone: boolean;
}

export interface Note {
  id: string;
  instance_id: string;
  text: string;
  done: boolean;
  ord: number;
}

export function Notes({ instanceId, settings, size }: WidgetProps<NotesSettings>) {
  const [items, setItems] = useState<Note[]>([]);
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  // 끄는 중인 항목과 그동안 보여 줄 순서. 손을 뗄 때만 서버에 보낸다 —
  // 매 포인터 이동마다 쓰면 SQL 과 새로고침이 초당 수십 번 돈다.
  const [dragId, setDragId] = useState<string | null>(null);
  const preview = useRef<string[] | null>(null);
  const [, bump] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const reload = useCallback(() => {
    invoke<Note[]>("notes_list", { instanceId })
      .then((n) => { setItems(n); setError(null); })
      .catch((e) => setError(String(e)));
  }, [instanceId]);

  useEffect(reload, [reload]);
  // 같은 목록을 여러 위젯이 보고 있을 수 있으니 변경 이벤트로 다시 읽는다.
  useEvent("notes://changed", reload);

  const run = (p: Promise<unknown>) =>
    p.then(() => { setError(null); reload(); }).catch((e) => setError(String(e)));

  const add = () => {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    run(invoke("notes_upsert", { note: { id: "", instance_id: instanceId, text, done: false, ord: 0 } }));
  };
  const toggle = (n: Note) => run(invoke("notes_upsert", { note: { ...n, done: !n.done } }));
  const rename = (n: Note, text: string) => {
    setEditing(null);
    if (text.trim() && text.trim() !== n.text) run(invoke("notes_upsert", { note: { ...n, text } }));
  };
  const remove = (n: Note) => run(invoke("notes_delete", { id: n.id }));
  const clearDone = () => run(invoke("notes_clear_done", { instanceId }));

  // 끄는 동안에는 미리보기 순서로 보여 준다. 저장된 순서는 손을 뗄 때 갱신된다.
  const order = preview.current;
  const sorted = order
    ? [...items].sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id))
    : items;
  const shown = settings.showDone ? sorted : sorted.filter((n) => !n.done);

  // --- 순서 바꾸기 ---
  //
  // 화면에 보이는 것만으로 전체 순서를 정할 수 있게 "어느 항목 **앞**에 둘지"로 계산한다
  // (`reorder.ts`). 완료 항목을 숨겨 둔 목록에서도 숨은 것들이 제자리를 지킨다.
  const rowsOf = () => {
    const el = listRef.current;
    if (!el) return [];
    return [...el.querySelectorAll<HTMLElement>("[data-note-id]")].map((n) => {
      const r = n.getBoundingClientRect();
      return { id: n.dataset.noteId!, top: r.top, height: r.height };
    });
  };

  const dragStart = (n: Note) => (e: RPointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    preview.current = items.map((i) => i.id);
    setDragId(n.id);
  };
  const dragMove = (n: Note) => (e: RPointerEvent) => {
    if (dragId !== n.id || !preview.current) return;
    const rows = rowsOf().filter((r) => r.id !== n.id);
    const next = moveBefore(preview.current, n.id, dropTarget(rows, e.clientY));
    if (next !== preview.current) { preview.current = next; bump((v) => v + 1); }
  };
  const dragEnd = () => {
    const next = preview.current;
    preview.current = null;
    setDragId(null);
    if (!next) return;
    // 원래 순서 그대로면 보내지 않는다.
    if (next.every((id, i) => id === items[i]?.id)) return;
    run(invoke("notes_reorder", { ids: next }));
  };
  const doneCount = items.filter((n) => n.done).length;
  const compact = size.h < 140;

  return (
    <div className="notes">
      {settings.title && !compact && (
        <div className="notes-head">
          <span className="notes-title">{settings.title}</span>
          {doneCount > 0 && (
            <button className="link" onClick={clearDone} title="완료한 항목 비우기">
              완료 {doneCount} 지우기
            </button>
          )}
        </div>
      )}

      {error && <div className="notes-error">{error}</div>}

      <div className="notes-list" ref={listRef}>
        {shown.length === 0 && <div className="dim">할 일이 없습니다</div>}
        {shown.map((n) => (
          <div key={n.id} data-note-id={n.id} className={`notes-item ${n.done ? "done" : ""} ${dragId === n.id ? "dragging" : ""}`}>
            <span
              className="notes-grip"
              title="끌어서 순서 바꾸기"
              onPointerDown={dragStart(n)}
              onPointerMove={dragMove(n)}
              onPointerUp={dragEnd}
              onPointerCancel={dragEnd}
            >⠿</span>
            <button
              className="notes-check"
              onClick={() => toggle(n)}
              title={n.done ? "되돌리기" : "완료"}
              aria-pressed={n.done}
            >
              {n.done ? "☑" : "☐"}
            </button>
            {editing === n.id ? (
              <input
                className="notes-edit"
                defaultValue={n.text}
                autoFocus
                onBlur={(e) => rename(n, e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                  if (e.key === "Escape") setEditing(null);
                }}
              />
            ) : (
              <span
                className={`notes-text ${settings.strikeDone && n.done ? "strike" : ""}`}
                onDoubleClick={() => setEditing(n.id)}
                title="더블클릭해서 수정"
              >
                {n.text}
              </span>
            )}
            <button className="notes-del" onClick={() => remove(n)} title="삭제">✕</button>
          </div>
        ))}
      </div>

      <div className="notes-add">
        <input
          ref={inputRef}
          type="text"
          value={draft}
          placeholder="할 일 추가…"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") add(); }}
        />
        <button onClick={add} disabled={!draft.trim()} title="추가">＋</button>
      </div>
    </div>
  );
}
