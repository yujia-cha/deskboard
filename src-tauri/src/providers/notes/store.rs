//! 메모·할 일 저장소. `NotesSource` 트레이트 뒤에 SQLite 구현을 둔다.
//!
//! 목록은 위젯 인스턴스마다 독립이다 — 같은 위젯을 여러 개 띄워 "업무 / 장보기" 처럼 나눠 쓴다.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Note {
    /// 비어 있으면 upsert 시 새 id 발급
    #[serde(default)]
    pub id: String,
    /// 어느 위젯 인스턴스의 목록인지
    pub instance_id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
    /// 목록 안 정렬 순서 (작을수록 위)
    #[serde(default)]
    pub ord: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum NotesError {
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
}

pub trait NotesSource: Send {
    fn list(&self, instance_id: &str) -> Result<Vec<Note>, NotesError>;
    fn upsert(&mut self, note: Note) -> Result<Note, NotesError>;
    fn delete(&mut self, id: &str) -> Result<bool, NotesError>;
    /// 완료된 항목을 한 번에 비운다.
    fn clear_done(&mut self, instance_id: &str) -> Result<usize, NotesError>;
    /// `ids` 순서대로 정렬 번호를 다시 매긴다.
    fn reorder(&mut self, ids: &[String]) -> Result<(), NotesError>;
}

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, NotesError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, NotesError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, NotesError> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS notes (
               id TEXT PRIMARY KEY,
               instance_id TEXT NOT NULL,
               text TEXT NOT NULL,
               done INTEGER NOT NULL DEFAULT 0,
               ord INTEGER NOT NULL DEFAULT 0,
               created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
               updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             );
             CREATE INDEX IF NOT EXISTS idx_notes_inst ON notes(instance_id, ord);",
        )?;
        Ok(Self { conn })
    }

    /// 새 항목이 목록 맨 아래로 가도록 다음 정렬 번호를 구한다.
    fn next_ord(&self, instance_id: &str) -> Result<i64, NotesError> {
        let max: Option<i64> = self.conn.query_row(
            "SELECT MAX(ord) FROM notes WHERE instance_id = ?1",
            params![instance_id],
            |r| r.get(0),
        )?;
        Ok(max.unwrap_or(-1) + 1)
    }
}

impl NotesSource for SqliteStore {
    fn list(&self, instance_id: &str) -> Result<Vec<Note>, NotesError> {
        let mut st = self.conn.prepare(
            "SELECT id, instance_id, text, done, ord FROM notes
             WHERE instance_id = ?1 ORDER BY ord, created_at",
        )?;
        let rows = st.query_map(params![instance_id], |r| {
            Ok(Note {
                id: r.get(0)?,
                instance_id: r.get(1)?,
                text: r.get(2)?,
                done: r.get::<_, i64>(3)? != 0,
                ord: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn upsert(&mut self, mut note: Note) -> Result<Note, NotesError> {
        if note.instance_id.trim().is_empty() {
            return Err(NotesError::Invalid("어느 목록인지 알 수 없습니다".into()));
        }
        note.text = note.text.trim().to_string();
        if note.text.is_empty() {
            return Err(NotesError::Invalid("내용이 비어 있습니다".into()));
        }
        if note.id.trim().is_empty() {
            note.id = uuid::Uuid::new_v4().to_string();
            note.ord = self.next_ord(&note.instance_id)?;
        }
        self.conn.execute(
            "INSERT INTO notes (id, instance_id, text, done, ord) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               text = excluded.text,
               done = excluded.done,
               ord = excluded.ord,
               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![note.id, note.instance_id, note.text, note.done as i64, note.ord],
        )?;
        Ok(note)
    }

    fn delete(&mut self, id: &str) -> Result<bool, NotesError> {
        Ok(self.conn.execute("DELETE FROM notes WHERE id = ?1", params![id])? > 0)
    }

    fn clear_done(&mut self, instance_id: &str) -> Result<usize, NotesError> {
        Ok(self.conn.execute(
            "DELETE FROM notes WHERE instance_id = ?1 AND done != 0",
            params![instance_id],
        )?)
    }

    fn reorder(&mut self, ids: &[String]) -> Result<(), NotesError> {
        let tx = self.conn.transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute("UPDATE notes SET ord = ?1 WHERE id = ?2", params![i as i64, id])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SqliteStore {
        SqliteStore::in_memory().unwrap()
    }
    fn note(inst: &str, text: &str) -> Note {
        Note { id: String::new(), instance_id: inst.into(), text: text.into(), done: false, ord: 0 }
    }

    #[test]
    fn adds_and_lists_in_insertion_order() {
        let mut s = store();
        s.upsert(note("a", "first")).unwrap();
        s.upsert(note("a", "second")).unwrap();
        let got = s.list("a").unwrap();
        assert_eq!(got.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["first", "second"]);
    }

    #[test]
    fn lists_are_separate_per_widget_instance() {
        let mut s = store();
        s.upsert(note("a", "work")).unwrap();
        s.upsert(note("b", "groceries")).unwrap();
        assert_eq!(s.list("a").unwrap().len(), 1);
        assert_eq!(s.list("b").unwrap().len(), 1);
        assert_eq!(s.list("a").unwrap()[0].text, "work");
    }

    #[test]
    fn editing_keeps_the_id_and_position() {
        let mut s = store();
        s.upsert(note("a", "one")).unwrap();
        let mut second = s.upsert(note("a", "two")).unwrap();
        second.text = "TWO".into();
        second.done = true;
        s.upsert(second.clone()).unwrap();
        let got = s.list("a").unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].id, second.id);
        assert_eq!(got[1].text, "TWO");
        assert!(got[1].done);
    }

    #[test]
    fn blank_text_is_rejected_rather_than_stored() {
        let mut s = store();
        assert!(s.upsert(note("a", "   ")).is_err());
        assert!(s.upsert(note("a", "")).is_err());
        assert_eq!(s.list("a").unwrap().len(), 0);
    }

    #[test]
    fn text_is_trimmed() {
        let mut s = store();
        s.upsert(note("a", "  padded  ")).unwrap();
        assert_eq!(s.list("a").unwrap()[0].text, "padded");
    }

    #[test]
    fn a_note_without_a_list_is_rejected() {
        let mut s = store();
        assert!(s.upsert(note("", "orphan")).is_err());
    }

    #[test]
    fn delete_reports_whether_anything_went() {
        let mut s = store();
        let n = s.upsert(note("a", "bye")).unwrap();
        assert!(s.delete(&n.id).unwrap());
        assert!(!s.delete(&n.id).unwrap());
        assert_eq!(s.list("a").unwrap().len(), 0);
    }

    #[test]
    fn clear_done_only_removes_finished_items_of_that_list() {
        let mut s = store();
        let mut a1 = s.upsert(note("a", "done one")).unwrap();
        s.upsert(note("a", "still open")).unwrap();
        let mut b1 = s.upsert(note("b", "other list")).unwrap();
        a1.done = true;
        b1.done = true;
        s.upsert(a1).unwrap();
        s.upsert(b1).unwrap();

        assert_eq!(s.clear_done("a").unwrap(), 1);
        assert_eq!(s.list("a").unwrap().len(), 1);
        assert_eq!(s.list("a").unwrap()[0].text, "still open");
        // 다른 목록은 건드리지 않는다
        assert_eq!(s.list("b").unwrap().len(), 1);
    }

    #[test]
    fn reorder_puts_the_list_in_the_given_order() {
        let mut s = store();
        let a = s.upsert(note("a", "1")).unwrap();
        let b = s.upsert(note("a", "2")).unwrap();
        let c = s.upsert(note("a", "3")).unwrap();
        s.reorder(&[c.id.clone(), a.id.clone(), b.id.clone()]).unwrap();
        let got = s.list("a").unwrap();
        assert_eq!(got.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["3", "1", "2"]);
    }

    #[test]
    fn reordering_an_unknown_id_is_harmless() {
        let mut s = store();
        s.upsert(note("a", "only")).unwrap();
        s.reorder(&["nope".to_string()]).unwrap();
        assert_eq!(s.list("a").unwrap().len(), 1);
    }

    #[test]
    fn listing_an_empty_list_is_not_an_error() {
        assert_eq!(store().list("nothing-here").unwrap().len(), 0);
    }
}
