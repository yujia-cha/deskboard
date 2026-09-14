import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  addDays, addMonths, format, isSameDay, isSameMonth, startOfMonth, startOfWeek, subMonths,
} from "date-fns";
import { ko } from "date-fns/locale";
import { useEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
import "./Calendar.css";

export interface CalendarSettings extends Record<string, unknown> {
  weekStartsMonday: boolean;
}

export interface CalendarEvent {
  id: string; title: string; starts_at: string; ends_at: string; all_day: boolean; color: string; note: string;
}

const COLORS = ["#7c9cff", "#58d68d", "#f5b041", "#ff6b6b", "#c39bd3", "#48c9b0"];
const dayKey = (d: Date) => format(d, "yyyy-MM-dd");

function emptyEvent(day: Date): CalendarEvent {
  return { id: "", title: "", starts_at: `${dayKey(day)}T09:00`, ends_at: `${dayKey(day)}T10:00`, all_day: false, color: COLORS[0], note: "" };
}

export function Calendar({ settings, size }: WidgetProps<CalendarSettings>) {
  const [cursor, setCursor] = useState(() => startOfMonth(new Date()));
  const [selected, setSelected] = useState(() => new Date());
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [editing, setEditing] = useState<CalendarEvent | null>(null);
  const [error, setError] = useState<string | null>(null);

  const weekStartsOn = settings.weekStartsMonday ? 1 : 0;
  const gridStart = startOfWeek(cursor, { weekStartsOn });
  const days = Array.from({ length: 42 }, (_, i) => addDays(gridStart, i));

  const reload = useCallback(async () => {
    const from = dayKey(gridStart), to = dayKey(addDays(gridStart, 42));
    try { setEvents(await invoke<CalendarEvent[]>("calendar_list", { from, to })); }
    catch (e) { setError(String(e)); }
  }, [gridStart.getTime()]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { reload(); }, [reload]);
  useEvent("calendar://changed", reload);

  const eventsOn = (d: Date) => events.filter((e) => e.starts_at.slice(0, 10) === dayKey(d));
  const selectedEvents = eventsOn(selected);

  const save = async () => {
    if (!editing) return;
    setError(null);
    try { await invoke("calendar_upsert", { event: editing }); setEditing(null); }
    catch (e) { setError(String(e)); }
  };
  const remove = async (id: string) => {
    try { await invoke("calendar_delete", { id }); if (editing?.id === id) setEditing(null); }
    catch (e) { setError(String(e)); }
  };

  const narrow = size.w < 300;
  const showList = size.h >= 300;

  return (
    <div className="cal">
      <div className="cal-nav">
        <button onClick={() => setCursor(subMonths(cursor, 1))}>‹</button>
        <button className="cal-month" onClick={() => { const t = new Date(); setCursor(startOfMonth(t)); setSelected(t); }}>
          {format(cursor, "yyyy년 M월", { locale: ko })}
        </button>
        <button onClick={() => setCursor(addMonths(cursor, 1))}>›</button>
      </div>

      <div className="cal-grid">
        {Array.from({ length: 7 }, (_, i) => addDays(gridStart, i)).map((d) => (
          <div key={i(d)} className={`cal-dow ${d.getDay() === 0 ? "sun" : d.getDay() === 6 ? "sat" : ""}`}>{format(d, "EEEEE", { locale: ko })}</div>
        ))}
        {days.map((d) => {
          const evs = eventsOn(d);
          const cls = ["cal-day",
            isSameMonth(d, cursor) ? "" : "other",
            isSameDay(d, new Date()) ? "today" : "",
            isSameDay(d, selected) ? "selected" : "",
            d.getDay() === 0 ? "sun" : d.getDay() === 6 ? "sat" : "",
          ].join(" ");
          return (
            <button key={dayKey(d)} className={cls} onClick={() => setSelected(d)}
              onDoubleClick={() => { setSelected(d); setEditing(emptyEvent(d)); }} title={evs.map((e) => e.title).join("\n")}>
              <span className="cal-num">{d.getDate()}</span>
              {evs.length > 0 && (
                <span className="cal-dots">{evs.slice(0, narrow ? 2 : 3).map((e) => <i key={e.id} style={{ background: e.color || COLORS[0] }} />)}</span>
              )}
            </button>
          );
        })}
      </div>

      {showList && (
        <div className="cal-list">
          <div className="cal-list-head">
            <span>{format(selected, "M월 d일 (EEE)", { locale: ko })}</span>
            <button onClick={() => setEditing(emptyEvent(selected))}>＋ 일정</button>
          </div>
          {selectedEvents.length === 0 && !editing && <div className="dim">일정 없음 — 날짜를 더블클릭해 추가</div>}
          {selectedEvents.map((e) => (
            <div key={e.id} className="cal-ev" onClick={() => setEditing(e)}>
              <i style={{ background: e.color || COLORS[0] }} />
              <span className="cal-ev-time">{e.all_day ? "종일" : e.starts_at.slice(11, 16)}</span>
              <span className="cal-ev-title">{e.title}</span>
              <button title="삭제" onClick={(ev) => { ev.stopPropagation(); remove(e.id); }}>✕</button>
            </div>
          ))}
        </div>
      )}

      {editing && (
        <div className="cal-editor" onKeyDown={(e) => { if (e.key === "Escape") setEditing(null); }}>
          <input autoFocus placeholder="제목" value={editing.title}
            onChange={(e) => setEditing({ ...editing, title: e.target.value })}
            onKeyDown={(e) => { if (e.key === "Enter") save(); }} />
          <label className="cal-row"><input type="checkbox" checked={editing.all_day}
            onChange={(e) => setEditing({ ...editing, all_day: e.target.checked,
              starts_at: e.target.checked ? editing.starts_at.slice(0, 10) : `${editing.starts_at.slice(0, 10)}T09:00`,
              ends_at: e.target.checked ? editing.ends_at.slice(0, 10) : `${editing.ends_at.slice(0, 10)}T10:00` })} /> 종일</label>
          <div className="cal-row">
            <input type={editing.all_day ? "date" : "datetime-local"} value={editing.starts_at}
              onChange={(e) => setEditing({ ...editing, starts_at: e.target.value, ends_at: e.target.value > editing.ends_at ? e.target.value : editing.ends_at })} />
            <span>~</span>
            <input type={editing.all_day ? "date" : "datetime-local"} value={editing.ends_at}
              onChange={(e) => setEditing({ ...editing, ends_at: e.target.value })} />
          </div>
          <div className="cal-row cal-colors">
            {COLORS.map((c) => <button key={c} className={editing.color === c ? "on" : ""} style={{ background: c }} onClick={() => setEditing({ ...editing, color: c })} />)}
          </div>
          <textarea placeholder="메모" rows={2} value={editing.note} onChange={(e) => setEditing({ ...editing, note: e.target.value })} />
          {error && <div className="cal-error">{error}</div>}
          <div className="cal-row cal-actions">
            {editing.id && <button className="danger" onClick={() => remove(editing.id)}>삭제</button>}
            <span style={{ flex: 1 }} />
            <button onClick={() => setEditing(null)}>취소</button>
            <button className="primary" onClick={save}>저장</button>
          </div>
        </div>
      )}
    </div>
  );
}

const i = (d: Date) => d.getDay();
