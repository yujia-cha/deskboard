//! 주민과 주문 — 명세 §9. 주문은 `Game.orders` 에 주민 id + 요구 아이템으로만 저장한다.

use super::board::{self, Cell, ChainItem, Game, Item, Pos, RuleError};
use super::content::Content;
use super::economy::Economy;
use super::energy::{self, Fx};
use super::progress;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub resident: String,
    pub wants: Vec<ChainItem>,
}

/// 주문에 쓸 수 있는 `free` 아이템을 **보관함 → 보드(위→아래, 왼쪽→오른쪽)** 순으로 (O-5).
/// 거미줄·상자에 든 것과 🎁 는 세지 않는다 (O-4).
fn candidates(game: &Game) -> impl Iterator<Item = (Pos, &ChainItem)> {
    let storage = game.inv.iter().enumerate().filter_map(|(i, x)| match x {
        Some(Item::Chain(c)) => Some((Pos::Storage(i), c)),
        _ => None,
    });
    let board = game.board.iter().enumerate().filter_map(|(i, x)| match x {
        Cell::Free { item: Item::Chain(c) } => Some((Pos::Board(i), c)),
        _ => None,
    });
    storage.chain(board)
}

/// 이 아이템 하나가 지금 `free` 로 있는가 (칩의 ✔).
pub fn has(game: &Game, want: &ChainItem) -> bool {
    candidates(game).any(|(_, c)| c == want)
}

/// 요구 아이템을 하나씩 서로 다른 칸에서 찾는다. 하나라도 없으면 `None`.
pub fn locate(game: &Game, wants: &[ChainItem]) -> Option<Vec<Pos>> {
    let mut used: Vec<Pos> = Vec::with_capacity(wants.len());
    for w in wants {
        let (p, _) = candidates(game).find(|(p, c)| *c == w && !used.contains(p))?;
        used.push(p);
    }
    Some(used)
}

pub fn fulfillable(game: &Game, order: &Order) -> bool {
    locate(game, &order.wants).is_some()
}

/// 주문할 수 있는 주민: 입주했고 숨기지 않았고, 좋아하는 계열 중 쓸 수 있는 것이 있다.
pub fn pool(content: &Content, level: u32) -> Vec<String> {
    content
        .active_residents(level)
        .into_iter()
        .filter(|r| !content.liked_chains(r, level).is_empty())
        .map(str::to_string)
        .collect()
}

/// O-3: `최고 = min(계열 최고, base + ⌊L/2⌋)`, `최저 = max(base, 최고 - (span-1))`, 그 사이에서 고르게.
fn want_level(content: &Content, eco: &Economy, chain: &str, player_level: u32, rng: &mut impl Rng) -> u8 {
    let cmax = content.max_level(chain).unwrap_or(1);
    let top = (eco.order.level_base as u32 + player_level / 2).min(cmax as u32) as u8;
    let bottom = top.saturating_sub(eco.order.level_span.saturating_sub(1)).max(eco.order.level_base).min(top);
    rng.random_range(bottom..=top)
}

/// 이 주민의 새 주문. 쓸 계열이 없으면 `None`.
pub fn new_order_for(
    content: &Content,
    eco: &Economy,
    player_level: u32,
    resident: &str,
    rng: &mut impl Rng,
) -> Option<Order> {
    let chains = content.liked_chains(resident, player_level);
    if chains.is_empty() {
        return None;
    }
    // likes 가 빈 주민(곰)은 늘 2종
    let always_two = content.residents.get(resident)?.likes.is_empty();
    let two = always_two
        || (player_level >= eco.order.two_item_from_level && rng.random_bool(eco.order.two_item_chance));
    let target = if two { 2 } else { 1 };
    let mut wants: Vec<ChainItem> = Vec::new();
    for _ in 0..32 {
        if wants.len() == target {
            break;
        }
        let chain = chains[rng.random_range(0..chains.len())];
        let w = ChainItem::new(chain, want_level(content, eco, chain, player_level, rng));
        if !wants.contains(&w) {
            wants.push(w);
        }
    }
    (!wants.is_empty()).then(|| Order { resident: resident.to_string(), wants })
}

/// 라운드로빈으로 다음 주민의 새 주문 (O-1). 원할 게 없는 주민은 풀에서 빠져 저절로 건너뛴다.
pub fn new_order(game: &mut Game, content: &Content, eco: &Economy, rng: &mut impl Rng) -> Option<Order> {
    let pool = pool(content, game.level);
    if pool.is_empty() {
        return None;
    }
    let who = pool[game.order_rr % pool.len()].clone();
    game.order_rr = game.order_rr.wrapping_add(1);
    new_order_for(content, eco, game.level, &who, rng)
}

/// 주문 칸을 채운다 (3개, Lv5 부터 4개).
pub fn refill_slots(game: &mut Game, content: &Content, eco: &Economy, rng: &mut impl Rng) {
    let target = progress::order_slots(eco, game.level);
    while game.orders.len() < target {
        match new_order(game, content, eco, rng) {
            Some(o) => game.orders.push(o),
            None => break,
        }
    }
}

/// 전달 (O-5~O-9). 보상 ⭐ 는 `fx.stars` 에 쌓고, 돌려주는 값도 같다.
pub fn deliver(
    game: &mut Game,
    content: &Content,
    eco: &Economy,
    slot: usize,
    rng: &mut impl Rng,
    fx: &mut Fx,
) -> Result<i64, RuleError> {
    let order = game.orders.get(slot).cloned().ok_or(RuleError::BadCell)?;
    let positions = locate(game, &order.wants).ok_or(RuleError::NoItems)?;
    let near = positions.iter().find_map(|p| match p {
        Pos::Board(i) => Some(*i),
        Pos::Storage(_) => None,
    });
    for p in &positions {
        match *p {
            Pos::Board(i) => game.board[i] = Cell::Empty,
            Pos::Storage(i) => game.inv[i] = None,
        }
    }

    let value: i64 = order.wants.iter().map(ChainItem::value).sum();
    let stars = ((value as f64 * eco.order.star_per_value).round() as i64).max(1);
    game.stars += stars;
    fx.stars += stars;
    if eco.order.energy_per_value > 0.0 {
        let gain = (value as f64 * eco.order.energy_per_value).round() as i64;
        energy::credit(game, gain, "order", Some(order.resident.clone()), fx);
    }

    // 친밀도 — 5·15·30·50번째에 좋아하는 계열의 🎁
    let n = {
        let c = game.affection.entry(order.resident.clone()).or_insert(0);
        *c += 1;
        *c
    };
    if let Some(i) = eco.resident.heart_milestones.iter().position(|&m| m == n) {
        let liked = content.liked_chains(&order.resident, game.level);
        if !liked.is_empty() {
            let chain = liked[rng.random_range(0..liked.len())];
            let level = eco.resident.heart_gift_levels[i].min(content.max_level(chain).unwrap_or(1)).max(1);
            let gift = Item::Gift { items: vec![ChainItem::new(chain, level)] };
            board::place_or_queue(game, gift, near);
        }
    }

    // O-7: 그 칸에 곧바로 새 주문
    match new_order(game, content, eco, rng) {
        Some(o) => game.orders[slot] = o,
        None => {
            game.orders.remove(slot);
        }
    }
    Ok(stars)
}

/// 같은 주민의 다른 주문으로 교체 (O-8). 다른 주문을 만들 수 없으면 과금 없이 그대로 둔다.
pub fn reroll(
    game: &mut Game,
    content: &Content,
    eco: &Economy,
    slot: usize,
    rng: &mut impl Rng,
    fx: &mut Fx,
) -> Result<(), RuleError> {
    let old = game.orders.get(slot).cloned().ok_or(RuleError::BadCell)?;
    let fresh = (0..16)
        .filter_map(|_| new_order_for(content, eco, game.level, &old.resident, rng))
        .find(|o| o.wants != old.wants);
    let Some(fresh) = fresh else { return Ok(()) };
    energy::spend(game, eco.order.reroll_cost, "spend:reroll", fx)?;
    game.orders[slot] = fresh;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::merge::board::tests::{at, free, rng, setup};

    fn game_at(level: u32) -> Game {
        Game { level, ..Game::default() }
    }

    fn order(resident: &str, wants: &[(&str, u8)]) -> Order {
        Order { resident: resident.into(), wants: wants.iter().map(|(c, l)| ChainItem::new(c, *l)).collect() }
    }

    #[test]
    fn first_orders_come_from_the_start_board_then_round_robin() {
        let (_, c, e) = setup();
        let g = board::new_game(&e, &c, 0, &mut rng());
        assert_eq!(g.orders[0], order("parrot", &[("clean", 2)]));
        assert_eq!(g.orders[1], order("rabbit", &[("garden", 2)]));
        assert_eq!(g.orders[2].resident, "parrot", "세 번째 칸은 규칙으로 — 라운드로빈이 이어진다");
        assert_eq!(g.orders[2].wants.len(), 1);
        assert_eq!(g.orders[2].wants[0].level, 2, "L1 은 Lv2 만");
    }

    #[test]
    fn round_robin_cycles_through_the_pool() {
        let (_, c, e) = setup();
        let mut g = game_at(5);
        let mut r = rng();
        let who: Vec<String> = (0..8).map(|_| new_order(&mut g, &c, &e, &mut r).unwrap().resident).collect();
        assert_eq!(who, ["parrot", "rabbit", "cat", "dog", "parrot", "rabbit", "cat", "dog"]);
    }

    #[test]
    fn want_level_ranges() {
        let (_, c, e) = setup();
        let mut r = rng();
        let range = |lvl: u32, r: &mut rand::rngs::StdRng| {
            let (mut lo, mut hi) = (u8::MAX, 0u8);
            for _ in 0..300 {
                let o = new_order_for(&c, &e, lvl, "parrot", r).unwrap();
                for w in &o.wants {
                    lo = lo.min(w.level);
                    hi = hi.max(w.level);
                }
            }
            (lo, hi)
        };
        assert_eq!(range(1, &mut r), (2, 2));
        assert_eq!(range(4, &mut r), (2, 4));
        assert_eq!(range(10, &mut r), (5, 7));
        assert_eq!(range(30, &mut r), (6, 8), "계열 최고(8) 에서 멈춘다");
    }

    #[test]
    fn two_item_orders_start_at_level_3_with_about_thirty_percent() {
        let (_, c, e) = setup();
        let mut r = rng();
        for _ in 0..200 {
            assert_eq!(new_order_for(&c, &e, 2, "parrot", &mut r).unwrap().wants.len(), 1);
        }
        let twos = (0..2000).filter(|_| new_order_for(&c, &e, 3, "parrot", &mut r).unwrap().wants.len() == 2).count();
        assert!((450..750).contains(&twos), "30% 근처여야 한다: {twos}/2000");
        // 2종은 서로 다른 아이템이다
        for _ in 0..200 {
            let o = new_order_for(&c, &e, 6, "parrot", &mut r).unwrap();
            if o.wants.len() == 2 {
                assert_ne!(o.wants[0], o.wants[1]);
            }
        }
    }

    #[test]
    fn bear_always_wants_two() {
        let (_, c, e) = setup();
        let mut r = rng();
        for _ in 0..200 {
            let o = new_order_for(&c, &e, 9, "bear", &mut r).unwrap();
            assert_eq!(o.wants.len(), 2);
            assert_ne!(o.wants[0], o.wants[1]);
        }
    }

    #[test]
    fn bear_respects_locked_and_hidden_chains() {
        let (_, mut c, e) = setup();
        let mut r = rng();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..300 {
            for w in new_order_for(&c, &e, 4, "bear", &mut r).unwrap().wants {
                seen.insert(w.chain);
            }
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), ["clean", "cook", "garden"], "tools(5)·sew(7) 는 잠김");
        c.chains.get_mut("cook").unwrap().hidden = true;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..300 {
            for w in new_order_for(&c, &e, 4, "bear", &mut r).unwrap().wants {
                seen.insert(w.chain);
            }
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), ["clean", "garden"]);
    }

    #[test]
    fn residents_with_nothing_to_want_are_skipped() {
        let (_, mut c, e) = setup();
        let mut r = rng();
        let mut g = game_at(5);
        assert_eq!(pool(&c, 5), ["parrot", "rabbit", "cat", "dog"]);
        c.chains.get_mut("cook").unwrap().hidden = true; // 고양이가 원할 게 없다
        assert_eq!(pool(&c, 5), ["parrot", "rabbit", "dog"]);
        let who: Vec<String> = (0..6).map(|_| new_order(&mut g, &c, &e, &mut r).unwrap().resident).collect();
        assert_eq!(who, ["parrot", "rabbit", "dog", "parrot", "rabbit", "dog"]);
        c.residents.get_mut("dog").unwrap().hidden = true;
        assert_eq!(pool(&c, 5), ["parrot", "rabbit"]);
        // 아무도 없으면 None
        c.residents.get_mut("parrot").unwrap().hidden = true;
        c.residents.get_mut("rabbit").unwrap().hidden = true;
        assert!(new_order(&mut g, &c, &e, &mut r).is_none());
    }

    #[test]
    fn fulfillable_counts_free_items_only() {
        let mut g = game_at(1);
        let o = order("parrot", &[("clean", 2)]);
        g.board[at(2, 2)] = Cell::Web { item: Item::chain("clean", 2) };
        assert!(!fulfillable(&g, &o), "거미줄에 걸린 것은 세지 않는다");
        g.board[at(3, 2)] = Cell::Box { item: Item::chain("clean", 2) };
        assert!(!fulfillable(&g, &o));
        g.board[at(4, 2)] = Cell::Free { item: Item::Gift { items: vec![ChainItem::new("clean", 2)] } };
        assert!(!fulfillable(&g, &o), "🎁 안은 아니다");
        g.inv[3] = Some(Item::chain("clean", 2));
        assert!(fulfillable(&g, &o), "보관함도 센다");
        let two = order("parrot", &[("clean", 2), ("garden", 2)]);
        assert!(!fulfillable(&g, &two));
        assert!(has(&g, &ChainItem::new("clean", 2)) && !has(&g, &ChainItem::new("garden", 2)));
        g.board[at(0, 0)] = free("garden", 2);
        assert!(fulfillable(&g, &two));
    }

    #[test]
    fn deliver_takes_storage_first_then_board_row_major() {
        let (_, c, e) = setup();
        let mut g = game_at(1);
        g.orders = vec![order("parrot", &[("clean", 2)])];
        g.board[at(5, 1)] = free("clean", 2);
        g.board[at(1, 3)] = free("clean", 2);
        g.inv[2] = Some(Item::chain("clean", 2));
        let mut fx = Fx::default();
        deliver(&mut g, &c, &e, 0, &mut rng(), &mut fx).unwrap();
        assert_eq!(g.inv[2], None, "보관함 먼저");
        assert_eq!(g.board[at(5, 1)], free("clean", 2));
        g.orders = vec![order("parrot", &[("clean", 2)])];
        deliver(&mut g, &c, &e, 0, &mut rng(), &mut fx).unwrap();
        assert_eq!(g.board[at(5, 1)], Cell::Empty, "위쪽 줄이 먼저 (행 우선)");
        assert_eq!(g.board[at(1, 3)], free("clean", 2));
    }

    #[test]
    fn deliver_without_items_errors_and_changes_nothing() {
        let (_, c, e) = setup();
        let mut g = game_at(1);
        g.orders = vec![order("parrot", &[("clean", 2)])];
        let before = g.clone();
        let mut fx = Fx::default();
        assert_eq!(deliver(&mut g, &c, &e, 0, &mut rng(), &mut fx), Err(RuleError::NoItems));
        assert_eq!(deliver(&mut g, &c, &e, 5, &mut rng(), &mut fx), Err(RuleError::BadCell));
        assert_eq!(g, before);
        assert_eq!(fx.stars, 0);
    }

    #[test]
    fn star_reward_formula() {
        let (_, c, e) = setup();
        let stars_for = |wants: &[(&str, u8)]| {
            let mut g = game_at(1);
            g.orders = vec![order("parrot", wants)];
            for (i, (ch, l)) in wants.iter().enumerate() {
                g.board[i] = free(ch, *l);
            }
            let mut fx = Fx::default();
            let s = deliver(&mut g, &c, &e, 0, &mut rng(), &mut fx).unwrap();
            assert_eq!((g.stars, fx.stars), (s, s));
            s
        };
        assert_eq!(stars_for(&[("clean", 3)]), 2, "Lv3 V=4");
        assert_eq!(stars_for(&[("clean", 6), ("garden", 5)]), 24, "V=48");
        assert_eq!(stars_for(&[("clean", 2)]), 1, "V=2");
        assert_eq!(stars_for(&[("clean", 1)]), 1, "최소 1");
    }

    #[test]
    fn deliver_replaces_the_slot_and_leaves_energy_alone_by_default() {
        let (_, c, e) = setup();
        let mut g = board::new_game(&e, &c, 0, &mut rng());
        g.board[at(0, 0)] = free("clean", 2);
        let mut fx = Fx::default();
        let energy = g.energy;
        deliver(&mut g, &c, &e, 0, &mut rng(), &mut fx).unwrap();
        assert_eq!(g.orders.len(), 3);
        assert_eq!(g.energy, energy);
        assert!(fx.deltas.is_empty(), "energyPerValue 0 이면 장부 행이 없다");
        assert_eq!(g.affection["parrot"], 1);
        // 켜면 order 행
        let mut e2 = e.clone();
        e2.order.energy_per_value = 1.0;
        g.orders[1] = order("rabbit", &[("garden", 3)]);
        g.board[at(0, 1)] = free("garden", 3);
        deliver(&mut g, &c, &e2, 1, &mut rng(), &mut fx).unwrap();
        assert_eq!(g.energy, energy + 4);
        assert_eq!((fx.deltas[0].reason, fx.deltas[0].delta), ("order", 4));
    }

    #[test]
    fn affection_milestones_give_a_liked_gift_of_rising_level() {
        let (_, c, e) = setup();
        for (before, level) in [(4u32, 3u8), (14, 4), (29, 5), (49, 6), (9, 0)] {
            let mut g = game_at(1);
            g.orders = vec![order("parrot", &[("clean", 2)])];
            g.board[at(0, 0)] = free("clean", 2);
            g.affection.insert("parrot".into(), before);
            deliver(&mut g, &c, &e, 0, &mut rng(), &mut Fx::default()).unwrap();
            let gift = g.board.iter().find_map(|x| match x {
                Cell::Free { item: Item::Gift { items } } => Some(items.clone()),
                _ => None,
            });
            if level == 0 {
                assert!(gift.is_none(), "10번째는 선물 없음");
            } else {
                // 곰이 아닌 앵무의 선물은 clean 계열이어야 하고, 계열 최고(8) 를 넘지 않는다
                assert_eq!(gift, Some(vec![ChainItem::new("clean", level)]), "{before}");
                // 전달한 칸 근처에 놓인다
                assert!(matches!(g.board[at(0, 0)], Cell::Free { item: Item::Gift { .. } }));
            }
        }
    }

    #[test]
    fn affection_gift_goes_to_the_freed_cell_or_storage_when_board_is_full() {
        let (_, c, e) = setup();
        let mut g = game_at(1);
        g.orders = vec![order("parrot", &[("clean", 2)])];
        g.board = vec![free("garden", 1); 63];
        g.board[at(0, 0)] = free("clean", 2);
        g.affection.insert("parrot".into(), 4);
        // 전달하면 칸이 하나 비므로 거기에 놓인다
        deliver(&mut g, &c, &e, 0, &mut rng(), &mut Fx::default()).unwrap();
        assert!(matches!(g.board[at(0, 0)], Cell::Free { item: Item::Gift { .. } }));
        // 보관함에서 꺼내 전달 + 보드 가득 → 비는 보관함 칸으로
        let mut g = game_at(1);
        g.orders = vec![order("parrot", &[("clean", 2)])];
        g.board = vec![free("garden", 1); 63];
        g.inv = vec![Some(Item::chain("clean", 2)), Some(Item::chain("clean", 1)), Some(Item::chain("clean", 1)), Some(Item::chain("clean", 1))];
        g.affection.insert("parrot".into(), 4);
        deliver(&mut g, &c, &e, 0, &mut rng(), &mut Fx::default()).unwrap();
        assert!(matches!(g.inv[0], Some(Item::Gift { .. })), "빈 보관함 칸");
    }

    #[test]
    fn reroll_changes_the_order_for_two_energy() {
        let (_, c, e) = setup();
        let mut g = game_at(4);
        g.energy = 10;
        g.orders = vec![order("parrot", &[("clean", 3)])];
        let mut fx = Fx::default();
        let mut r = rng();
        reroll(&mut g, &c, &e, 0, &mut r, &mut fx).unwrap();
        assert_eq!(g.orders[0].resident, "parrot");
        assert_ne!(g.orders[0].wants, vec![ChainItem::new("clean", 3)]);
        assert_eq!(g.energy, 8);
        assert_eq!((fx.deltas[0].reason, fx.deltas[0].delta), ("spend:reroll", -2));
        g.energy = 1;
        let before = g.clone();
        assert_eq!(reroll(&mut g, &c, &e, 0, &mut r, &mut fx), Err(RuleError::NoEnergy));
        assert_eq!(g, before);
        assert_eq!(reroll(&mut g, &c, &e, 9, &mut r, &mut fx), Err(RuleError::BadCell));
    }

    #[test]
    fn reroll_is_free_when_no_different_order_exists() {
        let (_, c, e) = setup();
        let mut g = game_at(1); // L1 앵무는 clean Lv2 하나뿐
        g.energy = 10;
        g.orders = vec![order("parrot", &[("clean", 2)])];
        reroll(&mut g, &c, &e, 0, &mut rng(), &mut Fx::default()).unwrap();
        assert_eq!(g.energy, 10);
    }

    #[test]
    fn refill_makes_three_slots_and_four_from_level_5() {
        let (_, c, e) = setup();
        let mut g = game_at(1);
        refill_slots(&mut g, &c, &e, &mut rng());
        assert_eq!(g.orders.len(), 3);
        g.level = 5;
        refill_slots(&mut g, &c, &e, &mut rng());
        assert_eq!(g.orders.len(), 4);
    }
}
