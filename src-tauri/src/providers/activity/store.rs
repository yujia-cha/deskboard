//! 활동 누적 저장소.
//!
//! **기록하는 것은 실행 파일 이름뿐이다.** 창 제목은 저장하지 않는다 — 문서명·탭 제목·대화
//! 상대가 그대로 남는 게 바탕화면 위젯이 할 일은 아니다.

use super::rules::Category;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UsageRow {
    pub exe: String,
    pub category: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionRow {
    pub exe: String,
    pub category: String,
    /// ISO-8601 로컬 시각
    pub started_at: String,
    pub ended_at: String,
    pub seconds: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, ActivityError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, ActivityError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, ActivityError> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS usage (
               day TEXT NOT NULL,
               exe TEXT NOT NULL,
               seconds INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (day, exe)
             );
             CREATE TABLE IF NOT EXISTS sessions (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               exe TEXT NOT NULL,
               started_at TEXT NOT NULL,
               ended_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_sessions_exe ON sessions(exe, ended_at);
             CREATE TABLE IF NOT EXISTS rules (
               exe TEXT PRIMARY KEY,
               category TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    /// 하루치 누적에 초를 더한다. `exe` 는 정규화된 이름이어야 한다.
    pub fn add_seconds(&mut self, day: &str, exe: &str, secs: i64) -> Result<(), ActivityError> {
        if secs <= 0 || exe.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO usage (day, exe, seconds) VALUES (?1, ?2, ?3)
             ON CONFLICT(day, exe) DO UPDATE SET seconds = seconds + excluded.seconds",
            params![day, exe, secs],
        )?;
        Ok(())
    }

    /// 방금 끝난 사용 구간을 기록한다.
    /// 같은 프로그램이 `gap_secs` 안에 다시 시작된 거라면 새로 넣지 않고 직전 구간을 늘린다
    /// (잠깐 창을 바꿨다고 세션이 쪼개지면 "얼마나 했나"를 읽을 수 없다).
    pub fn close_session(
        &mut self,
        exe: &str,
        started_at: &str,
        ended_at: &str,
        gap_secs: i64,
    ) -> Result<(), ActivityError> {
        if exe.is_empty() {
            return Ok(());
        }
        let last: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT id, ended_at FROM sessions WHERE exe = ?1 ORDER BY ended_at DESC LIMIT 1",
                params![exe],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        if let Some((id, prev_end)) = last {
            if super::within_gap(&prev_end, started_at, gap_secs) {
                self.conn.execute(
                    "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                    params![ended_at, id],
                )?;
                return Ok(());
            }
        }
        self.conn.execute(
            "INSERT INTO sessions (exe, started_at, ended_at) VALUES (?1, ?2, ?3)",
            params![exe, started_at, ended_at],
        )?;
        Ok(())
    }

    /// `from <= day <= to` 의 프로그램별 합계. 많이 쓴 순.
    pub fn usage(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, ActivityError> {
        let mut st = self.conn.prepare(
            "SELECT exe, SUM(seconds) FROM usage WHERE day BETWEEN ?1 AND ?2
             GROUP BY exe ORDER BY SUM(seconds) DESC",
        )?;
        let rows = st.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 하루의 사용 구간들. 최근 순.
    pub fn sessions(&self, day: &str, limit: i64) -> Result<Vec<(String, String, String)>, ActivityError> {
        let mut st = self.conn.prepare(
            "SELECT exe, started_at, ended_at FROM sessions
             WHERE substr(started_at, 1, 10) = ?1 OR substr(ended_at, 1, 10) = ?1
             ORDER BY ended_at DESC LIMIT ?2",
        )?;
        let rows = st.query_map(params![day, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn user_rules(&self) -> Result<Vec<(String, Category)>, ActivityError> {
        let mut st = self.conn.prepare("SELECT exe, category FROM rules")?;
        let rows = st.query_map([], |r| {
            let exe: String = r.get(0)?;
            let cat: String = r.get(1)?;
            Ok((exe, Category::parse(&cat)))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_rule(&mut self, exe: &str, cat: Category) -> Result<(), ActivityError> {
        self.conn.execute(
            "INSERT INTO rules (exe, category) VALUES (?1, ?2)
             ON CONFLICT(exe) DO UPDATE SET category = excluded.category",
            params![exe, cat.as_str()],
        )?;
        Ok(())
    }

    /// 오래된 기록을 지운다. 위젯이 보여주는 범위 밖은 쌓아둘 이유가 없다.
    pub fn prune(&mut self, before_day: &str) -> Result<usize, ActivityError> {
        let a = self.conn.execute("DELETE FROM usage WHERE day < ?1", params![before_day])?;
        let b = self
            .conn
            .execute("DELETE FROM sessions WHERE substr(ended_at, 1, 10) < ?1", params![before_day])?;
        Ok(a + b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::in_memory().unwrap()
    }

    #[test]
    fn seconds_accumulate_per_day_and_program() {
        let mut s = store();
        s.add_seconds("2026-09-21", "code", 300).unwrap();
        s.add_seconds("2026-09-21", "code", 120).unwrap();
        s.add_seconds("2026-09-21", "eldenring", 600).unwrap();
        s.add_seconds("2026-09-22", "code", 60).unwrap();

        let today = s.usage("2026-09-21", "2026-09-21").unwrap();
        assert_eq!(today, vec![("eldenring".into(), 600), ("code".into(), 420)]);
        // 범위를 넓히면 합쳐진다
        let both = s.usage("2026-09-21", "2026-09-22").unwrap();
        assert_eq!(both[1], ("code".to_string(), 480));
    }

    #[test]
    fn nonsense_additions_are_ignored() {
        let mut s = store();
        s.add_seconds("2026-09-21", "code", 0).unwrap();
        s.add_seconds("2026-09-21", "code", -5).unwrap();
        s.add_seconds("2026-09-21", "", 100).unwrap();
        assert_eq!(s.usage("2026-09-21", "2026-09-21").unwrap().len(), 0);
    }

    #[test]
    fn an_empty_range_is_not_an_error() {
        assert_eq!(store().usage("2026-01-01", "2026-01-02").unwrap().len(), 0);
    }

    #[test]
    fn a_short_break_extends_the_previous_session() {
        let mut s = store();
        s.close_session("eldenring", "2026-09-21T10:00:00", "2026-09-21T10:30:00", 300).unwrap();
        // 2분 뒤에 재개 → 같은 세션으로 이어붙인다
        s.close_session("eldenring", "2026-09-21T10:32:00", "2026-09-21T11:00:00", 300).unwrap();
        let got = s.sessions("2026-09-21", 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "2026-09-21T10:00:00");
        assert_eq!(got[0].2, "2026-09-21T11:00:00");
    }

    #[test]
    fn a_long_break_starts_a_new_session() {
        let mut s = store();
        s.close_session("eldenring", "2026-09-21T10:00:00", "2026-09-21T10:30:00", 300).unwrap();
        // 1시간 뒤 → 새 세션
        s.close_session("eldenring", "2026-09-21T11:30:00", "2026-09-21T12:00:00", 300).unwrap();
        assert_eq!(s.sessions("2026-09-21", 10).unwrap().len(), 2);
    }

    #[test]
    fn different_programs_never_merge() {
        let mut s = store();
        s.close_session("eldenring", "2026-09-21T10:00:00", "2026-09-21T10:30:00", 300).unwrap();
        s.close_session("code", "2026-09-21T10:31:00", "2026-09-21T10:40:00", 300).unwrap();
        assert_eq!(s.sessions("2026-09-21", 10).unwrap().len(), 2);
    }

    #[test]
    fn sessions_are_listed_most_recent_first_and_limited() {
        let mut s = store();
        for h in 10..15 {
            s.close_session("code", &format!("2026-09-21T{h}:00:00"), &format!("2026-09-21T{h}:30:00"), 0).unwrap();
        }
        let got = s.sessions("2026-09-21", 2).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, "2026-09-21T14:00:00");
    }

    #[test]
    fn sessions_of_another_day_are_not_returned() {
        let mut s = store();
        s.close_session("code", "2026-09-20T10:00:00", "2026-09-20T10:30:00", 0).unwrap();
        assert_eq!(s.sessions("2026-09-21", 10).unwrap().len(), 0);
    }

    #[test]
    fn user_rules_are_stored_and_replaced() {
        let mut s = store();
        s.set_rule("chrome", Category::Work).unwrap();
        assert_eq!(s.user_rules().unwrap(), vec![("chrome".to_string(), Category::Work)]);
        s.set_rule("chrome", Category::Other).unwrap();
        assert_eq!(s.user_rules().unwrap(), vec![("chrome".to_string(), Category::Other)]);
    }

    #[test]
    fn prune_drops_only_older_records() {
        let mut s = store();
        s.add_seconds("2026-08-01", "old", 100).unwrap();
        s.add_seconds("2026-09-21", "new", 100).unwrap();
        s.close_session("old", "2026-08-01T10:00:00", "2026-08-01T10:30:00", 0).unwrap();
        s.prune("2026-09-01").unwrap();
        assert_eq!(s.usage("2026-01-01", "2026-12-31").unwrap(), vec![("new".to_string(), 100)]);
        assert_eq!(s.sessions("2026-08-01", 10).unwrap().len(), 0);
    }
}
