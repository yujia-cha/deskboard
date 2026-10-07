//! 머지 게임 저장소. `MergeStore` 트레이트 뒤에 SQLite(`merge.sqlite`) 구현을 둔다.
//!
//! `game` 은 한 행 JSON(작아서 조작마다 통째로 다시 쓴다), 에너지는 `ledger` 의 합이 진실이고
//! `game.energy` 는 캐시다 — 열 때마다 장부 합으로 교정한다.

use super::board::{self, Game, CURRENT_VERSION};
use super::content::Content;
use super::economy::Economy;
use super::energy::{self, LedgerDelta, DAILY};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("저장된 게임이 더 새 버전(v{found})이라 열 수 없습니다 (이 앱은 v{supported})")]
    TooNew { found: i64, supported: u32 },
}

pub trait MergeStore: Send {
    fn load_game(&self) -> Result<Game, StoreError>;
    fn content(&self) -> Result<Content, StoreError>;
    /// 게임 JSON + 장부 + ⭐ 를 **한 트랜잭션**으로 쓴다.
    fn save_tx(
        &mut self,
        game: &Game,
        deltas: &[LedgerDelta],
        stars: i64,
        now_ms: i64,
        day_key: &str,
        week_key: &str,
    ) -> Result<(), StoreError>;
    #[cfg(test)]
    fn ledger_sum(&self) -> Result<i64, StoreError>;
}

pub struct SqliteStore {
    conn: Connection,
}

fn daily_list() -> String {
    DAILY.iter().map(|r| format!("'{r}'")).collect::<Vec<_>>().join(",")
}

fn schema() -> String {
    format!(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS game (
           id INTEGER PRIMARY KEY CHECK (id = 1),
           version INTEGER NOT NULL,
           json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS ledger (
           id INTEGER PRIMARY KEY,
           at INTEGER NOT NULL,
           delta INTEGER NOT NULL,
           reason TEXT NOT NULL,
           ref TEXT,
           day_key TEXT NOT NULL
         );
         CREATE UNIQUE INDEX IF NOT EXISTS ledger_daily ON ledger(reason, day_key) WHERE reason IN ({daily});
         CREATE TABLE IF NOT EXISTS chains (
           id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, max_level INTEGER NOT NULL,
           unlock_level INTEGER NOT NULL, emits TEXT, hidden INTEGER NOT NULL DEFAULT 0,
           builtin INTEGER NOT NULL DEFAULT 0, ord INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS chain_levels (
           chain_id TEXT NOT NULL, level INTEGER NOT NULL, name TEXT NOT NULL, icon TEXT NOT NULL,
           PRIMARY KEY (chain_id, level)
         );
         CREATE TABLE IF NOT EXISTS residents (
           id TEXT PRIMARY KEY, name TEXT NOT NULL, icon TEXT NOT NULL, likes TEXT NOT NULL,
           join_level INTEGER NOT NULL, hidden INTEGER NOT NULL DEFAULT 0,
           builtin INTEGER NOT NULL DEFAULT 0, ord INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS skins (key TEXT PRIMARY KEY, icon TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS stars (
           id INTEGER PRIMARY KEY, at INTEGER NOT NULL, delta INTEGER NOT NULL, week_key TEXT NOT NULL
         );",
        daily = daily_list()
    )
}

impl SqliteStore {
    pub fn open(path: &Path, eco: &Economy) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::init(Connection::open(path)?, eco)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::in_memory_with(&Economy::builtin())
    }

    pub fn in_memory_with(eco: &Economy) -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?, eco)
    }

    fn init(mut conn: Connection, eco: &Economy) -> Result<Self, StoreError> {
        conn.execute_batch(&schema())?;

        let stored: Option<(i64, String)> = conn
            .query_row("SELECT version, json FROM game WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        // 다운그레이드 금지 — 새 버전이 쓴 파일은 열지도, 덮어쓰지도 않는다
        if let Some((v, _)) = &stored {
            if *v > CURRENT_VERSION as i64 {
                return Err(StoreError::TooNew { found: *v, supported: CURRENT_VERSION });
            }
        }

        Content::seed(&conn)?;

        let now = chrono::Local::now();
        match stored {
            None => {
                let content = Content::load(&conn)?;
                let game = board::new_game(eco, &content, now.timestamp_millis(), &mut rand::rng());
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO game (id, version, json) VALUES (1, ?1, ?2)",
                    params![CURRENT_VERSION, serde_json::to_string(&game)?],
                )?;
                tx.execute(
                    "INSERT INTO ledger (at, delta, reason, ref, day_key) VALUES (?1, ?2, 'start', NULL, ?3)",
                    params![now.timestamp_millis(), eco.energy.start, energy::day_key(now.naive_local(), 0)],
                )?;
                tx.commit()?;
            }
            Some((_, json)) => {
                let mut game: Game = serde_json::from_str(&json)?;
                game.normalize();
                let sum: i64 =
                    conn.query_row("SELECT COALESCE(SUM(delta), 0) FROM ledger", [], |r| r.get(0))?;
                if game.energy != sum {
                    game.energy = sum;
                    conn.execute("UPDATE game SET json = ?1 WHERE id = 1", params![serde_json::to_string(&game)?])?;
                }
            }
        }
        Ok(Self { conn })
    }
}

impl MergeStore for SqliteStore {
    fn load_game(&self) -> Result<Game, StoreError> {
        let json: String = self.conn.query_row("SELECT json FROM game WHERE id = 1", [], |r| r.get(0))?;
        let mut g: Game = serde_json::from_str(&json)?;
        g.normalize();
        Ok(g)
    }

    fn content(&self) -> Result<Content, StoreError> {
        Ok(Content::load(&self.conn)?)
    }

    fn save_tx(
        &mut self,
        game: &Game,
        deltas: &[LedgerDelta],
        stars: i64,
        now_ms: i64,
        day_key: &str,
        week_key: &str,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(game)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO game (id, version, json) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET version = excluded.version, json = excluded.json",
            params![game.v, json],
        )?;
        // 부분 UNIQUE 인덱스의 ON CONFLICT 대상에는 인덱스와 같은 WHERE 를 되풀이해야 한다
        let upsert = format!(
            "INSERT INTO ledger (at, delta, reason, ref, day_key) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(reason, day_key) WHERE reason IN ({}) DO UPDATE SET delta = delta + excluded.delta, at = excluded.at",
            daily_list()
        );
        for d in deltas.iter().filter(|d| d.delta != 0) {
            if energy::is_daily(d.reason) {
                tx.execute(&upsert, params![now_ms, d.delta, d.reason, Option::<String>::None, day_key])?;
            } else {
                tx.execute(
                    "INSERT INTO ledger (at, delta, reason, ref, day_key) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![now_ms, d.delta, d.reason, d.ref_id, day_key],
                )?;
            }
        }
        if stars != 0 {
            tx.execute(
                "INSERT INTO stars (at, delta, week_key) VALUES (?1, ?2, ?3)",
                params![now_ms, stars, week_key],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    fn ledger_sum(&self) -> Result<i64, StoreError> {
        Ok(self.conn.query_row("SELECT COALESCE(SUM(delta), 0) FROM ledger", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(s: &SqliteStore, sql: &str) -> i64 {
        s.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn d(reason: &'static str, delta: i64) -> LedgerDelta {
        LedgerDelta { reason, delta, ref_id: None }
    }

    #[test]
    fn fresh_db_has_schema_seed_start_ledger_and_game() {
        let s = SqliteStore::in_memory().unwrap();
        assert_eq!(count(&s, "SELECT COUNT(*) FROM chains"), 8);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM chain_levels WHERE chain_id='clean'"), 8);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM chain_levels WHERE chain_id='gen_box'"), 5);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM residents"), 6);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM skins"), 4);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM chains WHERE builtin = 1"), 8);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger"), 1);
        assert_eq!(s.ledger_sum().unwrap(), 100);
        let reason: String = s.conn.query_row("SELECT reason FROM ledger", [], |r| r.get(0)).unwrap();
        assert_eq!(reason, "start");
        let g = s.load_game().unwrap();
        assert_eq!(g.energy, 100);
        assert_eq!(g.orders.len(), 3);
        assert_eq!(g.v, 1);
        let version: i64 = s.conn.query_row("SELECT version FROM game", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 1);
        // DB 에서 읽은 콘텐츠는 내장값과 같다
        let c = s.content().unwrap();
        let b = Content::builtin();
        assert_eq!(c.chains, b.chains);
        assert_eq!(c.residents, b.residents);
        assert_eq!(c.skins, b.skins);
        assert!(!c.level_rewards.is_empty());
    }

    #[test]
    fn daily_reasons_upsert_per_day() {
        let mut s = SqliteStore::in_memory().unwrap();
        let g = s.load_game().unwrap();
        for _ in 0..3 {
            s.save_tx(&g, &[d("spend:spawn", -1), d("regen", 2), d("clicker", 1)], 0, 1000, "2026-10-07", "2026-W41").unwrap();
        }
        fn row(s: &SqliteStore, reason: &str, day: &str) -> Option<i64> {
            s.conn
                .query_row("SELECT delta FROM ledger WHERE reason=?1 AND day_key=?2", params![reason, day], |r| r.get(0))
                .optional()
                .unwrap()
        }
        assert_eq!(row(&s, "spend:spawn", "2026-10-07"), Some(-3));
        assert_eq!(row(&s, "regen", "2026-10-07"), Some(6));
        assert_eq!(row(&s, "clicker", "2026-10-07"), Some(3));
        s.save_tx(&g, &[d("spend:spawn", -1)], 0, 2000, "2026-10-08", "2026-W41").unwrap();
        assert_eq!(row(&s, "spend:spawn", "2026-10-08"), Some(-1), "다음 날은 새 줄");
        assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger WHERE reason='spend:spawn'"), 2);
        assert_eq!(s.ledger_sum().unwrap(), 100 - 3 + 6 + 3 - 1);
    }

    #[test]
    fn non_daily_reasons_insert_one_row_each_with_ref() {
        let mut s = SqliteStore::in_memory().unwrap();
        let g = s.load_game().unwrap();
        let sell = LedgerDelta { reason: "sell", delta: 4, ref_id: Some("x".into()) };
        s.save_tx(&g, &[sell.clone(), sell, d("spend:box", -5), d("spend:box", -5), d("sell", 0)], 0, 1, "2026-10-07", "w").unwrap();
        assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger WHERE reason='sell'"), 2, "0 은 쓰지 않는다");
        assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger WHERE reason='spend:box'"), 2);
        let r: Option<String> = s.conn.query_row("SELECT ref FROM ledger WHERE reason='sell' LIMIT 1", [], |r| r.get(0)).unwrap();
        assert_eq!(r.as_deref(), Some("x"));
    }

    #[test]
    fn clicker_rows_never_carry_a_ref() {
        let mut s = SqliteStore::in_memory().unwrap();
        let g = s.load_game().unwrap();
        let with_ref = LedgerDelta { reason: "clicker", delta: 1, ref_id: Some("whatever".into()) };
        s.save_tx(&g, &[with_ref], 0, 1, "2026-10-07", "w").unwrap();
        assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger WHERE reason='clicker' AND ref IS NULL"), 1);
    }

    #[test]
    fn stars_rows_carry_the_week_key() {
        let mut s = SqliteStore::in_memory().unwrap();
        let g = s.load_game().unwrap();
        s.save_tx(&g, &[], 3, 5, "2026-10-07", "2026-W41").unwrap();
        s.save_tx(&g, &[], 0, 6, "2026-10-07", "2026-W41").unwrap();
        assert_eq!(count(&s, "SELECT COUNT(*) FROM stars"), 1);
        let (delta, week): (i64, String) =
            s.conn.query_row("SELECT delta, week_key FROM stars", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((delta, week.as_str()), (3, "2026-W41"));
    }

    #[test]
    fn game_json_roundtrips_through_save_and_load() {
        let mut s = SqliteStore::in_memory().unwrap();
        let mut g = s.load_game().unwrap();
        g.stars = 12;
        g.level = 2;
        g.clicker_rem = 7.5;
        g.key_clicker = true;
        g.affection.insert("parrot".into(), 3);
        g.pending_gifts.push(board::Item::chain("clean", 2));
        s.save_tx(&g, &[], 0, 1, "d", "w").unwrap();
        assert_eq!(s.load_game().unwrap(), g);
    }

    #[test]
    fn reopening_corrects_the_energy_cache_from_the_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("merge.sqlite");
        let eco = Economy::builtin();
        {
            let mut s = SqliteStore::open(&p, &eco).unwrap();
            let mut g = s.load_game().unwrap();
            g.energy = 9999; // 캐시가 어긋났다
            s.save_tx(&g, &[d("sell", 5)], 0, 1, "d", "w").unwrap();
            assert_eq!(s.load_game().unwrap().energy, 9999);
        }
        let s = SqliteStore::open(&p, &eco).unwrap();
        assert_eq!(s.load_game().unwrap().energy, 105, "SUM(delta)");
        assert_eq!(s.ledger_sum().unwrap(), 105);
        assert_eq!(count(&s, "SELECT COUNT(*) FROM ledger WHERE reason='start'"), 1, "다시 열어도 시작 ⚡ 는 한 번");
    }

    #[test]
    fn seed_adds_only_missing_ids_and_keeps_user_edits() {
        let s = SqliteStore::in_memory().unwrap();
        s.conn.execute("UPDATE chains SET name = '내 청소', hidden = 1 WHERE id = 'clean'", []).unwrap();
        s.conn.execute("UPDATE chain_levels SET name = '내 거품' WHERE chain_id = 'clean' AND level = 1", []).unwrap();
        s.conn.execute("DELETE FROM chains WHERE id = 'cook'", []).unwrap();
        s.conn.execute("DELETE FROM chain_levels WHERE chain_id = 'cook'", []).unwrap();
        s.conn.execute("UPDATE residents SET name = '내 앵무' WHERE id = 'parrot'", []).unwrap();
        Content::seed(&s.conn).unwrap();
        let c = s.content().unwrap();
        assert_eq!(c.chains["clean"].name, "내 청소");
        assert!(c.chains["clean"].hidden);
        assert_eq!(c.chains["clean"].levels[0].name, "내 거품");
        assert_eq!(c.chains["cook"].levels.len(), 7, "지운 id 는 되살아난다");
        assert_eq!(c.residents["parrot"].name, "내 앵무");
        assert_eq!(count(&s, "SELECT COUNT(*) FROM chains"), 8);
    }

    #[test]
    fn newer_stored_version_is_refused_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("merge.sqlite");
        let eco = Economy::builtin();
        drop(SqliteStore::open(&p, &eco).unwrap());
        {
            let conn = Connection::open(&p).unwrap();
            conn.execute("UPDATE game SET version = 99, json = '{\"v\":99,\"future\":true}'", []).unwrap();
        }
        match SqliteStore::open(&p, &eco) {
            Err(StoreError::TooNew { found: 99, .. }) => {}
            Err(e) => panic!("{e}"),
            Ok(_) => panic!("열리면 안 된다"),
        }
        let conn = Connection::open(&p).unwrap();
        let (v, json): (i64, String) =
            conn.query_row("SELECT version, json FROM game", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(v, 99);
        assert_eq!(json, "{\"v\":99,\"future\":true}");
    }

    #[test]
    fn corrupt_game_json_is_an_error_not_a_reset() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("merge.sqlite");
        let eco = Economy::builtin();
        drop(SqliteStore::open(&p, &eco).unwrap());
        Connection::open(&p).unwrap().execute("UPDATE game SET json = 'oops'", []).unwrap();
        assert!(matches!(SqliteStore::open(&p, &eco), Err(StoreError::Json(_))));
    }
}
