//! 일정 저장소. `CalendarSource` 트레이트 뒤에 SQLite 구현을 둔다 (추후 Google Calendar 등 추가 가능).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarEvent {
    /// 비어 있으면 upsert 시 새 id 발급
    #[serde(default)]
    pub id: String,
    pub title: String,
    /// ISO-8601 로컬 시각 문자열 (`2026-09-14T09:00`). 종일 일정은 `2026-09-14`.
    pub starts_at: String,
    pub ends_at: String,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
}

pub trait CalendarSource: Send {
    /// `from <= starts_at < to` (문자열 비교; ISO 형식이라 사전순 = 시간순)
    fn list(&self, from: &str, to: &str) -> Result<Vec<CalendarEvent>, CalendarError>;
    fn upsert(&mut self, ev: CalendarEvent) -> Result<CalendarEvent, CalendarError>;
    fn delete(&mut self, id: &str) -> Result<bool, CalendarError>;
}

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, CalendarError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Self, CalendarError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, CalendarError> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS events (
               id TEXT PRIMARY KEY,
               title TEXT NOT NULL,
               starts_at TEXT NOT NULL,
               ends_at TEXT NOT NULL,
               all_day INTEGER NOT NULL DEFAULT 0,
               color TEXT NOT NULL DEFAULT '',
               note TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
               updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             );
             CREATE INDEX IF NOT EXISTS idx_events_starts ON events(starts_at);",
        )?;
        Ok(Self { conn })
    }
}

fn validate(ev: &CalendarEvent) -> Result<(), CalendarError> {
    if ev.title.trim().is_empty() {
        return Err(CalendarError::Invalid("제목이 비어 있습니다".into()));
    }
    if ev.starts_at.is_empty() || ev.ends_at.is_empty() {
        return Err(CalendarError::Invalid("시작/종료 시각이 필요합니다".into()));
    }
    if ev.ends_at < ev.starts_at {
        return Err(CalendarError::Invalid("종료가 시작보다 빠릅니다".into()));
    }
    Ok(())
}

impl CalendarSource for SqliteStore {
    fn list(&self, from: &str, to: &str) -> Result<Vec<CalendarEvent>, CalendarError> {
        let mut st = self.conn.prepare(
            "SELECT id, title, starts_at, ends_at, all_day, color, note FROM events
             WHERE starts_at >= ?1 AND starts_at < ?2 ORDER BY all_day DESC, starts_at",
        )?;
        let rows = st.query_map(params![from, to], |r| {
            Ok(CalendarEvent {
                id: r.get(0)?, title: r.get(1)?, starts_at: r.get(2)?, ends_at: r.get(3)?,
                all_day: r.get::<_, i64>(4)? != 0, color: r.get(5)?, note: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn upsert(&mut self, mut ev: CalendarEvent) -> Result<CalendarEvent, CalendarError> {
        validate(&ev)?;
        if ev.id.is_empty() {
            ev.id = uuid::Uuid::new_v4().to_string();
        }
        self.conn.execute(
            "INSERT INTO events (id, title, starts_at, ends_at, all_day, color, note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, starts_at=excluded.starts_at,
               ends_at=excluded.ends_at, all_day=excluded.all_day, color=excluded.color, note=excluded.note,
               updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![ev.id, ev.title.trim(), ev.starts_at, ev.ends_at, ev.all_day as i64, ev.color, ev.note],
        )?;
        ev.title = ev.title.trim().to_string();
        Ok(ev)
    }

    fn delete(&mut self, id: &str) -> Result<bool, CalendarError> {
        let existed: Option<String> = self
            .conn
            .query_row("SELECT id FROM events WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        self.conn.execute("DELETE FROM events WHERE id = ?1", params![id])?;
        Ok(existed.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(title: &str, start: &str) -> CalendarEvent {
        CalendarEvent { id: String::new(), title: title.into(), starts_at: start.into(), ends_at: start.into(), all_day: false, color: "".into(), note: "".into() }
    }

    #[test]
    fn crud_roundtrip() {
        let mut s = SqliteStore::in_memory().unwrap();
        let a = s.upsert(ev("회의", "2026-09-14T10:00")).unwrap();
        s.upsert(ev("점심", "2026-09-15T12:00")).unwrap();
        assert!(!a.id.is_empty());

        let day = s.list("2026-09-14", "2026-09-15").unwrap();
        assert_eq!(day.len(), 1);
        assert_eq!(day[0].title, "회의");

        let mut edited = a.clone();
        edited.title = "회의(변경)".into();
        s.upsert(edited).unwrap();
        assert_eq!(s.list("2026-09-14", "2026-09-15").unwrap()[0].title, "회의(변경)");

        assert!(s.delete(&a.id).unwrap());
        assert!(!s.delete(&a.id).unwrap());
        assert_eq!(s.list("2026-09-01", "2026-10-01").unwrap().len(), 1);
    }

    #[test]
    fn rejects_invalid() {
        let mut s = SqliteStore::in_memory().unwrap();
        assert!(s.upsert(ev("  ", "2026-09-14T10:00")).is_err());
        let mut bad = ev("x", "2026-09-14T10:00");
        bad.ends_at = "2026-09-14T09:00".into();
        assert!(s.upsert(bad).is_err());
    }
}
