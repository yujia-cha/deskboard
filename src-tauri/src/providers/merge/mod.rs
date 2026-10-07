//! 🧩 Merge-2 보드 게임. **Rust 가 게임 상태의 주인**이다 — 조작 하나 = 커맨드 하나 = SQLite 트랜잭션 하나.
//! 프론트는 받은 `MergeView` 를 그리기만 하고, 변경 커맨드는 `merge://changed`(`()`) 를 함께 emit 한다.
//!
//! 설계: docs/MERGE_GAME.md · 규칙: docs/MERGE_GAME_SPEC.md (숫자가 다르면 SPEC 이 이긴다).

pub mod board;
pub mod content;
pub mod economy;
pub mod energy;
pub mod keycount;
pub mod orders;
pub mod progress;
pub mod store;

use super::Provider;
use board::{Cell, Game, Item};
use chrono::{DateTime, Local};
use content::Content;
use economy::Economy;
use energy::Fx;
use keycount::KeyClicker;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use store::{MergeStore, SqliteStore};
use tauri::{AppHandle, Emitter, Manager, State};

// --- 응답 -------------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WantView {
    pub chain: String,
    pub level: u8,
    pub have: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderView {
    pub resident: String,
    pub wants: Vec<WantView>,
    pub ready: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcoView {
    pub regen_interval_sec: u64,
    pub regen_cap: i64,
    pub clicks_per_energy: u32,
    pub sell_refund: f64,
    pub sell_confirm_from_level: u8,
    pub box_open_cost: i64,
    pub reroll_cost: i64,
    pub gen_cost: i64,
}

/// `merge_state` 의 응답이자 모든 변경 커맨드의 반환값.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeView {
    pub board: Vec<Cell>,
    pub inv: Vec<Option<Item>>,
    pub inv_slots: u8,
    pub inv_next_cost: Option<i64>,
    pub pending_gifts: usize,
    pub orders: Vec<OrderView>,
    pub stars: i64,
    pub level: u32,
    pub next_level_at: Option<i64>,
    pub energy: i64,
    /// unix ms — 프론트가 "다음 +1 까지" 를 센다
    pub regen_anchor: i64,
    pub clicker_rem: f64,
    pub key_clicker: bool,
    pub key_notice_seen: bool,
    pub tutorial_step: u8,
    pub eco: EcoView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickerView {
    pub energy: i64,
    pub clicker_rem: f64,
}

// --- 상태 -------------------------------------------------------------------------------------

pub struct Inner {
    pub store: Box<dyn MergeStore>,
    pub eco: Economy,
    pub content: Content,
    pub game: Game,
    /// ⌨ 를 세도 되는지 가르는 "지금 떠 있는 merge 위젯 인스턴스" (명세 K-9)
    pub present: HashSet<String>,
    pub keys: Option<KeyClicker>,
    rng: StdRng,
}

pub struct MergeState(pub Mutex<Inner>);

impl Inner {
    pub fn new(store: Box<dyn MergeStore>, eco: Economy) -> Result<Self, String> {
        let game = store.load_game().map_err(|e| e.to_string())?;
        let content = store.content().map_err(|e| e.to_string())?;
        Ok(Self {
            store,
            eco,
            content,
            game,
            present: HashSet::new(),
            keys: None,
            rng: StdRng::from_os_rng(),
        })
    }

    pub fn in_memory(eco: Economy) -> Result<Self, String> {
        let store = SqliteStore::in_memory_with(&eco).map_err(|e| e.to_string())?;
        Self::new(Box::new(store), eco)
    }

    /// 조작 하나의 트랜잭션 안무: 회복 정산 → 규칙 적용(오류면 아무것도 저장하지 않음) → 레벨업 → 한 번에 저장.
    pub fn mutate_at<T>(
        &mut self,
        now: DateTime<Local>,
        f: impl FnOnce(&mut Game, &Content, &Economy, &mut StdRng, &mut Fx) -> Result<T, board::RuleError>,
    ) -> Result<T, String> {
        let now_ms = now.timestamp_millis();
        let mut g = self.game.clone();
        let mut fx = Fx::default();
        energy::settle_regen(&mut g, &self.eco, now_ms, &mut fx);
        let out = f(&mut g, &self.content, &self.eco, &mut self.rng, &mut fx).map_err(|e| e.to_string())?;
        progress::apply_level_ups(&mut g, &self.content, &self.eco, &mut self.rng);
        let naive = now.naive_local();
        let day = energy::day_key(naive, g.day_start_hour);
        let week = energy::week_key(naive, g.day_start_hour);
        self.store.save_tx(&g, &fx.deltas, fx.stars, now_ms, &day, &week).map_err(|e| e.to_string())?;
        self.game = g;
        Ok(out)
    }

    pub fn mutate<T>(
        &mut self,
        f: impl FnOnce(&mut Game, &Content, &Economy, &mut StdRng, &mut Fx) -> Result<T, board::RuleError>,
    ) -> Result<T, String> {
        self.mutate_at(Local::now(), f)
    }

    /// 읽기 전용 커맨드용 — 시간 회복만 정산한다. 얻은 ⚡ 가 있을 때만 저장하고 `true` 를 돌려준다
    /// (읽을 때마다 쓰고 emit 하면 `merge://changed` → 다시 읽기 → emit 의 고리가 된다).
    pub fn refresh_at(&mut self, now: DateTime<Local>) -> Result<bool, String> {
        let now_ms = now.timestamp_millis();
        let mut g = self.game.clone();
        let mut fx = Fx::default();
        energy::settle_regen(&mut g, &self.eco, now_ms, &mut fx);
        if fx.deltas.is_empty() {
            // 기준 시각만 당겨졌을 수 있다 — 메모리에만 반영해도 충분하다 (다음 정산이 같은 결과)
            self.game = g;
            return Ok(false);
        }
        let naive = now.naive_local();
        let day = energy::day_key(naive, g.day_start_hour);
        let week = energy::week_key(naive, g.day_start_hour);
        self.store.save_tx(&g, &fx.deltas, 0, now_ms, &day, &week).map_err(|e| e.to_string())?;
        self.game = g;
        Ok(true)
    }

    pub fn refresh(&mut self) -> Result<bool, String> {
        self.refresh_at(Local::now())
    }

    pub fn view(&self) -> MergeView {
        let g = &self.game;
        let eco = &self.eco;
        let orders = g
            .orders
            .iter()
            .map(|o| OrderView {
                resident: o.resident.clone(),
                wants: o
                    .wants
                    .iter()
                    .map(|w| WantView { chain: w.chain.clone(), level: w.level, have: orders::has(g, w) })
                    .collect(),
                ready: orders::fulfillable(g, o),
            })
            .collect();
        MergeView {
            board: g.board.clone(),
            inv: g.inv.clone(),
            inv_slots: g.inv_slots,
            inv_next_cost: board::inv_next_cost(g, eco),
            pending_gifts: g.pending_gifts.len(),
            orders,
            stars: g.stars,
            level: g.level,
            next_level_at: Some(progress::next_level_at(eco, g.level)),
            energy: g.energy,
            regen_anchor: g.regen_anchor,
            clicker_rem: g.clicker_rem,
            key_clicker: g.key_clicker,
            key_notice_seen: g.key_notice_seen,
            tutorial_step: g.tutorial_step,
            eco: EcoView {
                regen_interval_sec: eco.regen.interval_sec,
                regen_cap: eco.regen.cap,
                clicks_per_energy: eco.clicker.clicks_per_energy,
                sell_refund: eco.sell.refund,
                sell_confirm_from_level: eco.sell.confirm_from_level,
                box_open_cost: eco.chest.open_cost,
                reroll_cost: eco.order.reroll_cost,
                gen_cost: eco.generator.cost,
            },
        }
    }

    // --- 조작 (커맨드 하나 = 메서드 하나) -----------------------------------------------------------

    pub fn tap(&mut self, cell: usize) -> Result<(), String> {
        self.mutate(|g, c, e, r, fx| board::tap(g, c, e, cell, r, fx))
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), String> {
        self.mutate(|g, c, _, _, _| board::move_item(g, c, from, to))
    }

    pub fn sell(&mut self, cell: usize) -> Result<(), String> {
        self.mutate(|g, c, e, _, fx| board::sell(g, c, e, cell, fx).map(|_| ()))
    }

    pub fn deliver(&mut self, slot: usize) -> Result<(), String> {
        self.mutate(|g, c, e, r, fx| orders::deliver(g, c, e, slot, r, fx).map(|_| ()))
    }

    pub fn reroll(&mut self, slot: usize) -> Result<(), String> {
        self.mutate(|g, c, e, r, fx| orders::reroll(g, c, e, slot, r, fx))
    }

    pub fn open_box(&mut self, cell: usize) -> Result<(), String> {
        self.mutate(|g, _, e, _, fx| board::open_box(g, e, cell, fx))
    }

    pub fn inv_expand(&mut self) -> Result<(), String> {
        self.mutate(|g, _, e, _, fx| board::inv_expand(g, e, fx))
    }

    pub fn claim_gift(&mut self) -> Result<(), String> {
        self.mutate(|g, _, _, _, _| board::claim_gift(g))
    }

    /// `"key_notice"` 는 ⌨ 안내를 봤다는 표시, 숫자는 튜토리얼 단계.
    pub fn tutorial(&mut self, step: &str) -> Result<(), String> {
        enum Step {
            KeyNotice,
            At(u8),
        }
        let parsed = match step {
            "key_notice" => Step::KeyNotice,
            s => Step::At(s.parse().map_err(|_| board::RuleError::BadStep.to_string())?),
        };
        self.mutate(|g, _, _, _, _| {
            match parsed {
                Step::KeyNotice => g.key_notice_seen = true,
                Step::At(n) => g.tutorial_step = n,
            }
            Ok(())
        })
    }

    /// 클릭 `clicks` 번(소수 가능)을 모은다. 마우스 클릭(1 씩)과 ⌨(`clicksPerKey` 배)가 같은 길을 쓴다.
    pub fn clicker_clicks(&mut self, clicks: f64) -> Result<ClickerView, String> {
        self.mutate(|g, _, e, _, fx| {
            energy::clicker_add(g, e, clicks, fx);
            Ok(())
        })?;
        Ok(ClickerView { energy: self.game.energy, clicker_rem: self.game.clicker_rem })
    }

    pub fn set_key_clicker(&mut self, on: bool) -> Result<(), String> {
        self.mutate(|g, _, _, _, _| {
            g.key_clicker = on;
            Ok(())
        })
    }

    /// ⌨ 를 세야 하는가: 켜져 있고 merge 위젯이 하나라도 떠 있을 때만 (K-9).
    pub fn key_wanted(&self) -> bool {
        self.game.key_clicker && !self.present.is_empty()
    }

    /// `key_wanted` 에 맞춰 키 세기를 시작/정지한다. 멈출 때는 떼어 낸 `KeyClicker` 를 **돌려준다** —
    /// drop 이 남은 횟수를 정산하려고 `apply_key_clicks`(= 이 락)를 부를 수 있으므로,
    /// **호출자는 락을 놓은 뒤에** 돌려받은 값을 버려야 한다 (락을 쥔 채 버리면 자기 자신을 기다린다).
    #[must_use = "락을 놓은 뒤에 drop 한다"]
    pub fn sync_key_source(&mut self, app: &AppHandle) -> Option<KeyClicker> {
        if self.key_wanted() {
            if self.keys.is_none() {
                let app = app.clone();
                self.keys = KeyClicker::start(Box::new(move |n| apply_key_clicks(app.clone(), n)));
            }
            None
        } else {
            self.keys.take()
        }
    }
}

/// `KeyClicker` 의 정산기가 부른다 — 누른 횟수 `n` 만 받는다 (어떤 키였는지는 어디에도 없다).
pub fn apply_key_clicks(app: AppHandle, n: u64) {
    if n == 0 {
        return;
    }
    let Some(state) = app.try_state::<MergeState>() else { return };
    let result = {
        let Ok(mut inner) = state.0.lock() else { return };
        let clicks = n as f64 * inner.eco.clicker.clicks_per_key;
        inner.clicker_clicks(clicks)
    };
    match result {
        Ok(_) => {
            let _ = app.emit("merge://changed", ());
        }
        Err(e) => log::warn!("merge: key clicks not applied: {e}"),
    }
}

// --- Provider -----------------------------------------------------------------------------------

pub struct MergeProvider;

impl Provider for MergeProvider {
    fn id(&self) -> &'static str {
        "merge"
    }

    fn start(&self, app: AppHandle) {
        let dir = super::data_dir(&app);
        let eco = Economy::load_with_override(&dir.join("merge-economy.json"));
        let path = dir.join("merge.sqlite");
        let opened = SqliteStore::open(&path, &eco)
            .map_err(|e| e.to_string())
            .and_then(|s| Inner::new(Box::new(s), eco.clone()));
        let inner = match opened {
            Ok(i) => i,
            Err(e) => {
                log::error!("merge: cannot open {} ({e}); falling back to in-memory", path.display());
                Inner::in_memory(eco).expect("in-memory sqlite")
            }
        };
        app.manage(MergeState(Mutex::new(inner)));
    }
}

fn lock<'a>(state: &'a State<'_, MergeState>) -> Result<std::sync::MutexGuard<'a, Inner>, String> {
    state.0.lock().map_err(|_| "게임 저장소를 열 수 없습니다".to_string())
}

/// 변경 커맨드의 공통 꼴: 락 → 조작 → 키 세기 동기화 → 뷰 → (락을 놓고) 정지한 `KeyClicker` 정리 → emit.
fn run(
    app: &AppHandle,
    state: &State<'_, MergeState>,
    f: impl FnOnce(&mut Inner) -> Result<(), String>,
) -> Result<MergeView, String> {
    // `_stale` 은 함수 끝에서(= 락이 풀린 뒤) drop 된다
    let (view, _stale) = {
        let mut inner = lock(state)?;
        f(&mut inner)?;
        let stale = inner.sync_key_source(app);
        (inner.view(), stale)
    };
    let _ = app.emit("merge://changed", ());
    Ok(view)
}

#[tauri::command]
pub fn merge_state(app: AppHandle, state: State<'_, MergeState>) -> Result<MergeView, String> {
    let (view, changed) = {
        let mut inner = lock(&state)?;
        let changed = inner.refresh()?;
        (inner.view(), changed)
    };
    if changed {
        let _ = app.emit("merge://changed", ());
    }
    Ok(view)
}

#[tauri::command]
pub fn merge_content(state: State<'_, MergeState>) -> Result<Content, String> {
    Ok(lock(&state)?.content.clone())
}

#[tauri::command]
pub fn merge_tap(app: AppHandle, state: State<'_, MergeState>, cell: usize) -> Result<MergeView, String> {
    run(&app, &state, |i| i.tap(cell))
}

#[tauri::command]
pub fn merge_move(
    app: AppHandle,
    state: State<'_, MergeState>,
    from: usize,
    to: usize,
) -> Result<MergeView, String> {
    run(&app, &state, |i| i.move_item(from, to))
}

#[tauri::command]
pub fn merge_sell(app: AppHandle, state: State<'_, MergeState>, cell: usize) -> Result<MergeView, String> {
    run(&app, &state, |i| i.sell(cell))
}

#[tauri::command]
pub fn merge_deliver(app: AppHandle, state: State<'_, MergeState>, slot: usize) -> Result<MergeView, String> {
    run(&app, &state, |i| i.deliver(slot))
}

#[tauri::command]
pub fn merge_reroll(app: AppHandle, state: State<'_, MergeState>, slot: usize) -> Result<MergeView, String> {
    run(&app, &state, |i| i.reroll(slot))
}

#[tauri::command]
pub fn merge_open_box(app: AppHandle, state: State<'_, MergeState>, cell: usize) -> Result<MergeView, String> {
    run(&app, &state, |i| i.open_box(cell))
}

#[tauri::command]
pub fn merge_inv_expand(app: AppHandle, state: State<'_, MergeState>) -> Result<MergeView, String> {
    run(&app, &state, |i| i.inv_expand())
}

#[tauri::command]
pub fn merge_claim_gift(app: AppHandle, state: State<'_, MergeState>) -> Result<MergeView, String> {
    run(&app, &state, |i| i.claim_gift())
}

#[tauri::command]
pub fn merge_tutorial(app: AppHandle, state: State<'_, MergeState>, step: String) -> Result<MergeView, String> {
    run(&app, &state, |i| i.tutorial(&step))
}

#[tauri::command]
pub fn merge_set_key_clicker(app: AppHandle, state: State<'_, MergeState>, on: bool) -> Result<MergeView, String> {
    run(&app, &state, |i| i.set_key_clicker(on))
}

/// 프론트가 모아 보내는 마우스 클릭 `n` 번. `{energy, clickerRem}` 만 돌려준다.
#[tauri::command]
pub fn merge_clicker_add(app: AppHandle, state: State<'_, MergeState>, n: u32) -> Result<ClickerView, String> {
    let view = lock(&state)?.clicker_clicks(n as f64)?;
    let _ = app.emit("merge://changed", ());
    Ok(view)
}

/// merge 위젯 인스턴스가 떴다/사라졌다. 게임 상태는 바뀌지 않으므로 emit 하지 않는다.
#[tauri::command]
pub fn merge_widget_present(
    app: AppHandle,
    state: State<'_, MergeState>,
    instance_id: String,
    present: bool,
) -> Result<MergeView, String> {
    let (view, _stale) = {
        let mut inner = lock(&state)?;
        if present {
            inner.present.insert(instance_id);
        } else {
            inner.present.remove(&instance_id);
        }
        let stale = inner.sync_key_source(&app);
        (inner.view(), stale)
    };
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use board::{ChainItem, Cell, Item};
    use chrono::Duration;

    fn inner() -> Inner {
        let mut i = Inner::in_memory(Economy::builtin()).unwrap();
        i.rng = StdRng::seed_from_u64(42);
        i.game.regen_anchor = Local::now().timestamp_millis();
        i
    }

    fn at(x: usize, y: usize) -> usize {
        y * board::W + x
    }

    fn free(chain: &str, level: u8) -> Cell {
        Cell::Free { item: Item::chain(chain, level) }
    }

    fn persisted(i: &Inner) -> Game {
        i.store.load_game().unwrap()
    }

    #[test]
    fn tapping_a_generator_spends_one_energy_and_spawns() {
        let mut i = inner();
        let gen = at(2, 4);
        assert_eq!(i.game.board[gen], free("gen_box", 1));
        i.tap(gen).unwrap();
        assert_eq!(i.game.energy, 99);
        assert_eq!(i.store.ledger_sum().unwrap(), 99);
        assert_eq!(i.game.board[at(2, 3)], free("clean", 1), "가장 가까운 빈 칸 중 위쪽");
        assert_eq!(persisted(&i), i.game, "저장된 것과 메모리가 같다");
    }

    #[test]
    fn not_enough_energy_leaves_state_untouched() {
        let mut i = inner();
        i.game.energy = 0;
        let before = i.game.clone();
        let ledger = i.store.ledger_sum().unwrap();
        assert_eq!(i.tap(at(2, 4)).unwrap_err(), "에너지가 부족합니다");
        assert_eq!(i.game, before);
        assert_eq!(i.store.ledger_sum().unwrap(), ledger);
    }

    #[test]
    fn rule_errors_are_atomic() {
        let mut i = inner();
        let before = i.game.clone();
        let ledger = i.store.ledger_sum().unwrap();
        assert_eq!(i.move_item(at(0, 0), at(1, 1)).unwrap_err(), "그 칸은 옮길 수 없습니다");
        assert_eq!(i.tap(at(0, 0)).unwrap_err(), "그 칸은 옮길 수 없습니다", "상자를 누르면 오류 — 프론트가 확인 뒤 open_box");
        assert_eq!(i.sell(at(2, 4)).unwrap_err(), "생산기는 팔 수 없습니다");
        assert_eq!(i.deliver(0).unwrap_err(), "주문을 채울 아이템이 없습니다");
        assert_eq!(i.deliver(9).unwrap_err(), "잘못된 칸 번호입니다");
        assert_eq!(i.move_item(500, 1).unwrap_err(), "잘못된 칸 번호입니다");
        assert_eq!(i.game, before);
        assert_eq!(i.store.ledger_sum().unwrap(), ledger);
        assert_eq!(persisted(&i).board, before.board);
    }

    #[test]
    fn state_read_settles_regen_and_persists_only_when_it_changed() {
        let mut i = inner();
        i.game.energy = 50;
        let now = Local::now();
        i.game.regen_anchor = (now - Duration::seconds(125 * 5)).timestamp_millis();
        assert!(i.refresh_at(now).unwrap());
        assert_eq!(i.game.energy, 55);
        assert_eq!(i.view().energy, 55);
        assert_eq!(i.store.ledger_sum().unwrap(), 105, "회복은 장부에 한 줄");
        assert!(!i.refresh_at(now).unwrap(), "다시 읽어도 변화 없음 — emit 고리를 만들지 않는다");
        assert_eq!(i.store.ledger_sum().unwrap(), 105);
    }

    #[test]
    fn commands_settle_regen_first() {
        let mut i = inner();
        i.game.energy = 10;
        let now = Local::now();
        i.game.regen_anchor = (now - Duration::seconds(125 * 3)).timestamp_millis();
        i.mutate_at(now, |g, c, e, r, fx| board::tap(g, c, e, at(2, 4), r, fx)).unwrap();
        assert_eq!(i.game.energy, 10 + 3 - 1);
    }

    #[test]
    fn deliver_gives_stars_and_levels_up_with_rewards() {
        let mut i = inner();
        i.game.stars = 9;
        i.game.board[at(0, 8)] = free("clean", 2);
        i.deliver(0).unwrap();
        assert_eq!(i.game.stars, 10);
        assert_eq!(i.game.level, 2);
        assert!(i.game.board.iter().any(|c| matches!(c, Cell::Free { item: Item::Gift { items } } if items.len() == 4)));
        assert_eq!(i.game.orders.len(), 3);
        let stars: i64 = i.store.load_game().unwrap().stars;
        assert_eq!(stars, 10);
    }

    #[test]
    fn view_matches_the_api_contract() {
        let mut i = inner();
        i.game.inv[0] = Some(Item::Gift { items: vec![ChainItem::new("clean", 2)] });
        let v = serde_json::to_value(i.view()).unwrap();
        assert_eq!(v["board"].as_array().unwrap().len(), 63);
        assert_eq!(v["board"][0]["state"], "box");
        assert_eq!(v["board"][at(3, 3)]["item"], serde_json::json!({"kind":"chain","chain":"clean","level":1}));
        assert_eq!(v["board"][at(2, 3)], serde_json::json!({"state":"empty"}));
        assert_eq!(v["inv"].as_array().unwrap().len(), 4);
        assert!(v["inv"][1].is_null());
        assert_eq!(v["inv"][0]["kind"], "gift");
        for (k, want) in [
            ("invSlots", serde_json::json!(4)),
            ("invNextCost", serde_json::json!(20)),
            ("pendingGifts", serde_json::json!(0)),
            ("stars", serde_json::json!(0)),
            ("level", serde_json::json!(1)),
            ("nextLevelAt", serde_json::json!(10)),
            ("energy", serde_json::json!(100)),
            ("clickerRem", serde_json::json!(0.0)),
            ("keyClicker", serde_json::json!(false)),
            ("keyNoticeSeen", serde_json::json!(false)),
            ("tutorialStep", serde_json::json!(0)),
        ] {
            assert_eq!(v[k], want, "{k}");
        }
        assert!(v["regenAnchor"].is_i64());
        assert_eq!(v["orders"].as_array().unwrap().len(), 3);
        assert_eq!(v["orders"][0]["resident"], "parrot");
        assert_eq!(v["orders"][0]["wants"][0], serde_json::json!({"chain":"clean","level":2,"have":false}));
        assert_eq!(v["orders"][0]["ready"], false);
        assert_eq!(
            v["eco"],
            serde_json::json!({
                "regenIntervalSec": 120, "regenCap": 100, "clicksPerEnergy": 20, "sellRefund": 0.5,
                "sellConfirmFromLevel": 5, "boxOpenCost": 5, "rerollCost": 2, "genCost": 1
            })
        );
        // 칩의 ✔ 와 [전달]
        i.game.board[at(0, 8)] = free("clean", 2);
        let v = serde_json::to_value(i.view()).unwrap();
        assert_eq!(v["orders"][0]["wants"][0]["have"], true);
        assert_eq!(v["orders"][0]["ready"], true);
        assert_eq!(v["orders"][1]["ready"], false);
    }

    #[test]
    fn content_serializes_to_the_api_contract() {
        let i = inner();
        let v = serde_json::to_value(&i.content).unwrap();
        assert!(v["chains"]["clean"]["levels"].as_array().unwrap().len() == 8);
        assert_eq!(v["residents"]["bear"]["likes"], serde_json::json!([]));
        assert_eq!(v["skins"]["box"], serde_json::json!({"emoji":"📦"}));
    }

    #[test]
    fn clicker_batches_keep_the_remainder_and_one_ledger_row() {
        let mut i = inner();
        let c = i.clicker_clicks(25.0).unwrap();
        assert_eq!((c.energy, c.clicker_rem), (101, 5.0));
        let c = i.clicker_clicks(15.0).unwrap();
        assert_eq!((c.energy, c.clicker_rem), (102, 0.0));
        assert_eq!(i.store.ledger_sum().unwrap(), 102);
        // 키 한 번 = clicksPerKey 클릭 (소수)
        i.eco.clicker.clicks_per_key = 0.5;
        let clicks = 40.0 * i.eco.clicker.clicks_per_key;
        let c = i.clicker_clicks(clicks).unwrap();
        assert_eq!((c.energy, c.clicker_rem), (103, 0.0));
    }

    #[test]
    fn no_key_identity_leaves_the_game_state() {
        // 키 세기는 횟수(u64)만 받는다 — 게임 JSON 에 키 정보가 들어갈 자리가 없다 (장부 ref 는 store.rs 테스트)
        let mut i = inner();
        i.clicker_clicks(37.0).unwrap();
        let json = serde_json::to_string(&i.game).unwrap().to_lowercase();
        for banned in ["vkey", "keycode", "scancode", "\"vk\"", "\"key\"", "\"code\""] {
            assert!(!json.contains(banned), "{banned}");
        }
    }

    #[test]
    fn key_source_is_wanted_only_when_enabled_and_a_widget_is_present() {
        let mut i = inner();
        assert!(!i.key_wanted());
        i.set_key_clicker(true).unwrap();
        assert!(!i.key_wanted(), "위젯이 없으면 세지 않는다 (K-9)");
        i.present.insert("a".into());
        assert!(i.key_wanted());
        i.present.insert("b".into());
        i.present.remove("a");
        assert!(i.key_wanted(), "하나라도 남아 있으면");
        i.present.remove("b");
        assert!(!i.key_wanted());
        i.present.insert("a".into());
        i.set_key_clicker(false).unwrap();
        assert!(!i.key_wanted());
    }

    #[test]
    fn tutorial_steps() {
        let mut i = inner();
        i.tutorial("key_notice").unwrap();
        assert!(i.game.key_notice_seen);
        i.tutorial("3").unwrap();
        assert_eq!(i.game.tutorial_step, 3);
        assert_eq!(i.tutorial("nope").unwrap_err(), "잘못된 단계입니다");
        assert_eq!(i.game.tutorial_step, 3);
        assert_eq!(persisted(&i), i.game);
    }

    #[test]
    fn box_inventory_and_sell_flow_writes_matching_ledger_rows() {
        let mut i = inner();
        i.open_box(at(1, 1)).unwrap();
        assert!(matches!(i.game.board[at(1, 1)], Cell::Web { .. }));
        assert_eq!(i.game.energy, 95);
        i.inv_expand().unwrap();
        assert_eq!((i.game.inv_slots, i.game.energy), (5, 75));
        i.game.board[at(0, 8)] = free("clean", 4);
        i.sell(at(0, 8)).unwrap();
        assert_eq!(i.game.energy, 79);
        assert_eq!(i.store.ledger_sum().unwrap(), 79);
        assert_eq!(persisted(&i), i.game);
    }

    #[test]
    fn reroll_and_claim_gift_work_through_the_command_path() {
        let mut i = inner();
        i.game.level = 4;
        i.game.orders[0] = orders::Order { resident: "parrot".into(), wants: vec![ChainItem::new("clean", 3)] };
        i.reroll(0).unwrap();
        assert_eq!(i.game.energy, 98);
        assert_ne!(i.game.orders[0].wants, vec![ChainItem::new("clean", 3)]);
        i.game.pending_gifts.push(Item::Gift { items: vec![ChainItem::new("clean", 2)] });
        assert_eq!(i.view().pending_gifts, 1);
        i.claim_gift().unwrap();
        assert_eq!(i.view().pending_gifts, 0);
    }
}
