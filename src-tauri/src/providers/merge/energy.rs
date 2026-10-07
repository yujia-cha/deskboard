//! 에너지. 숫자 하나가 아니라 **장부(ledger)의 합**이다 — 모든 증감은 `Fx` 에 쌓였다가
//! 커맨드 끝에서 같은 트랜잭션으로 장부에 들어간다. 시간 회복은 타이머 없이 `settle_regen`.

use super::board::{Game, RuleError};
use super::economy::Economy;
use chrono::{Datelike, Duration, NaiveDateTime};

/// 하루 단위로 묶어 한 줄을 갱신하는 장부 reason (명세 E-7). 부분 UNIQUE 인덱스와 같은 목록.
pub const DAILY: [&str; 3] = ["spend:spawn", "regen", "clicker"];

pub fn is_daily(reason: &str) -> bool {
    DAILY.contains(&reason)
}

/// 장부에 넣을 증감 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerDelta {
    pub reason: &'static str,
    pub delta: i64,
    pub ref_id: Option<String>,
}

/// 한 조작이 만든 부수 효과 — 저장 때 한 트랜잭션으로 쓴다.
#[derive(Debug, Default, Clone)]
pub struct Fx {
    pub deltas: Vec<LedgerDelta>,
    /// 이번 조작으로 늘어난 ⭐ (`stars` 표에 한 줄)
    pub stars: i64,
}

pub fn credit(game: &mut Game, amount: i64, reason: &'static str, ref_id: Option<String>, fx: &mut Fx) {
    if amount == 0 {
        return;
    }
    game.energy += amount;
    fx.deltas.push(LedgerDelta { reason, delta: amount, ref_id });
}

/// ⚡ 를 쓴다. 모자라면 아무것도 바꾸지 않고 오류. (⚡ 는 음수일 수 있다 — C-3 — 그땐 쓰는 조작이 멈춘다.)
pub fn spend(game: &mut Game, cost: i64, reason: &'static str, fx: &mut Fx) -> Result<(), RuleError> {
    if cost <= 0 {
        return Ok(());
    }
    if game.energy < cost {
        return Err(RuleError::NoEnergy);
    }
    game.energy -= cost;
    fx.deltas.push(LedgerDelta { reason, delta: -cost, ref_id: None });
    Ok(())
}

/// 시간 회복 정산. 앱이 꺼져 있던 시간도 센다. ⚡ 가 `cap` 이상이면 멈추고 기준 시각만 `now` 로 당긴다.
pub fn settle_regen(game: &mut Game, eco: &Economy, now_ms: i64, fx: &mut Fx) {
    let interval = (eco.regen.interval_sec.max(1) as i64).saturating_mul(1000);
    if now_ms < game.regen_anchor {
        // 시계가 되감겼다 — 미래의 기준에 묶이면 회복이 멈춘다
        game.regen_anchor = now_ms;
        return;
    }
    if game.energy >= eco.regen.cap {
        game.regen_anchor = now_ms;
        return;
    }
    let n = (now_ms - game.regen_anchor) / interval;
    if n == 0 {
        return;
    }
    let room = eco.regen.cap - game.energy;
    let add = n.min(room);
    credit(game, add, "regen", None, fx);
    game.regen_anchor = if game.energy >= eco.regen.cap { now_ms } else { game.regen_anchor + n * interval };
}

/// 클릭을 모은다. `clicks` 는 소수일 수 있다 (`clicksPerKey` 0.1). 만든 ⚡ 를 돌려준다.
pub fn clicker_add(game: &mut Game, eco: &Economy, clicks: f64, fx: &mut Fx) -> i64 {
    if !clicks.is_finite() || clicks <= 0.0 {
        return 0;
    }
    let per = eco.clicker.clicks_per_energy as f64;
    game.clicker_rem += clicks;
    let gained = (game.clicker_rem / per).floor();
    game.clicker_rem -= gained * per;
    if game.clicker_rem < 0.0 {
        game.clicker_rem = 0.0;
    }
    let gained = gained as i64;
    credit(game, gained, "clicker", None, fx);
    gained
}

/// 하루의 시작 시각을 뺀 날짜 `YYYY-MM-DD` (4 로 두면 새벽 3시는 어제).
pub fn day_key(now_local: NaiveDateTime, day_start_hour: u8) -> String {
    (now_local - Duration::hours(day_start_hour as i64)).format("%Y-%m-%d").to_string()
}

/// ISO 주 `YYYY-Www` (월요일 시작). 하루의 시작 시각 기준은 `day_key` 와 같다.
pub fn week_key(now_local: NaiveDateTime, day_start_hour: u8) -> String {
    let iso = (now_local - Duration::hours(day_start_hour as i64)).date().iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn eco() -> Economy {
        Economy::builtin()
    }

    fn game(energy: i64, anchor: i64) -> Game {
        Game { energy, regen_anchor: anchor, ..Game::default() }
    }

    const MIN: i64 = 60_000;

    #[test]
    fn three_hours_offline_fills_to_cap_only() {
        let e = eco();
        let mut g = game(40, 0);
        let mut fx = Fx::default();
        settle_regen(&mut g, &e, 180 * MIN, &mut fx); // 90 틱이지만 100 까지는 60 만
        assert_eq!(g.energy, 100);
        assert_eq!(g.regen_anchor, 180 * MIN, "가득 차면 기준을 now 로");
        assert_eq!(fx.deltas.len(), 1);
        assert_eq!(fx.deltas[0].delta, 60);
        assert_eq!(fx.deltas[0].reason, "regen");
    }

    #[test]
    fn partial_regen_advances_anchor_by_whole_intervals() {
        let e = eco();
        let mut g = game(10, 1000);
        let mut fx = Fx::default();
        settle_regen(&mut g, &e, 1000 + 120_000 * 3 + 50_000, &mut fx);
        assert_eq!(g.energy, 13);
        assert_eq!(g.regen_anchor, 1000 + 360_000, "남은 50초는 다음 틱에 이어진다");
    }

    #[test]
    fn regen_boundary_is_exactly_interval() {
        let e = eco();
        let mut g = game(10, 0);
        let mut fx = Fx::default();
        settle_regen(&mut g, &e, 119_999, &mut fx);
        assert_eq!(g.energy, 10);
        assert!(fx.deltas.is_empty());
        settle_regen(&mut g, &e, 120_000, &mut fx);
        assert_eq!(g.energy, 11);
    }

    #[test]
    fn at_or_over_cap_stops_and_resets_anchor() {
        let e = eco();
        let mut g = game(250, 0);
        let mut fx = Fx::default();
        settle_regen(&mut g, &e, 10_000_000, &mut fx);
        assert_eq!(g.energy, 250, "초과분은 건드리지 않는다");
        assert_eq!(g.regen_anchor, 10_000_000);
        assert!(fx.deltas.is_empty());
        // 쓰고 나면 now 부터 다시 센다 — 쌓아 둔 시간으로 한꺼번에 받지 않는다
        g.energy = 99;
        settle_regen(&mut g, &e, 10_060_000, &mut fx);
        assert_eq!(g.energy, 99);
    }

    #[test]
    fn clock_rollback_resets_anchor() {
        let e = eco();
        let mut g = game(10, 5_000_000);
        let mut fx = Fx::default();
        settle_regen(&mut g, &e, 1_000_000, &mut fx);
        assert_eq!(g.regen_anchor, 1_000_000);
        assert_eq!(g.energy, 10);
        settle_regen(&mut g, &e, 1_000_000 + 120_000, &mut fx);
        assert_eq!(g.energy, 11, "되감긴 뒤에도 회복이 이어진다");
    }

    #[test]
    fn clicker_remainder_carries_over() {
        let e = eco();
        let mut g = game(0, 0);
        let mut fx = Fx::default();
        assert_eq!(clicker_add(&mut g, &e, 19.0, &mut fx), 0);
        assert!(fx.deltas.is_empty());
        assert_eq!(g.clicker_rem, 19.0);
        assert_eq!(clicker_add(&mut g, &e, 2.0, &mut fx), 1);
        assert_eq!(g.clicker_rem, 1.0);
        assert_eq!(g.energy, 1);
        assert_eq!(clicker_add(&mut g, &e, 60.0, &mut fx), 3);
        assert_eq!(g.clicker_rem, 1.0);
        assert_eq!(g.energy, 4);
        assert_eq!(fx.deltas.iter().map(|d| d.reason).collect::<Vec<_>>(), vec!["clicker", "clicker"]);
    }

    #[test]
    fn fractional_clicks_per_key_accumulate() {
        let e = eco();
        let mut g = game(0, 0);
        let mut fx = Fx::default();
        let per_key = 0.1;
        let mut total = 0;
        for _ in 0..200 {
            total += clicker_add(&mut g, &e, per_key, &mut fx);
        }
        // 200 × 0.1 = 20 클릭 = 1 ⚡ (부동소수 오차가 있어도 한 틱 이내)
        assert!(total == 1 || (total == 0 && g.clicker_rem > 19.9), "total={total} rem={}", g.clicker_rem);
    }

    #[test]
    fn spend_blocks_when_short_or_negative() {
        let mut g = game(1, 0);
        let mut fx = Fx::default();
        assert!(matches!(spend(&mut g, 2, "spend:box", &mut fx), Err(RuleError::NoEnergy)));
        assert_eq!(g.energy, 1);
        assert!(fx.deltas.is_empty());
        spend(&mut g, 1, "spend:spawn", &mut fx).unwrap();
        assert_eq!(g.energy, 0);
        g.energy = -5;
        assert!(matches!(spend(&mut g, 1, "spend:spawn", &mut fx), Err(RuleError::NoEnergy)));
        assert_eq!(g.energy, -5);
    }

    #[test]
    fn day_key_honors_day_start_hour() {
        let at = |h| NaiveDate::from_ymd_opt(2026, 10, 7).unwrap().and_hms_opt(h, 30, 0).unwrap();
        assert_eq!(day_key(at(3), 0), "2026-10-07");
        assert_eq!(day_key(at(3), 4), "2026-10-06", "새벽 3시는 어제");
        assert_eq!(day_key(at(4), 4), "2026-10-07");
        assert_eq!(day_key(at(23), 6), "2026-10-07");
    }

    #[test]
    fn week_key_is_iso() {
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap().and_hms_opt(12, 0, 0).unwrap();
        assert_eq!(week_key(d(2026, 10, 7), 0), "2026-W41");
        assert_eq!(week_key(d(2026, 10, 5), 0), "2026-W41", "월요일");
        assert_eq!(week_key(d(2026, 10, 4), 0), "2026-W40", "일요일");
        assert_eq!(week_key(d(2027, 1, 1), 0), "2026-W53", "ISO 해 경계");
        let early = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap().and_hms_opt(2, 0, 0).unwrap();
        assert_eq!(week_key(early, 4), "2026-W40", "월요일 새벽 2시는 지난 주");
    }
}
