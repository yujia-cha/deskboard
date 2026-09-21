import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
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

  const shown = settings.showDone ? items : items.filter((n) => !n.done);
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

      <div className="notes-list">
        {shown.length === 0 && <div className="dim">할 일이 없습니다</div>}
        {shown.map((n) => (
          <div key={n.id} className={`notes-item ${n.done ? "done" : ""}`}>
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
