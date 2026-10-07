//! ⭐ → 레벨 → 보상 (명세 §10). 레벨은 내려가지 않는다 (L-3).

use super::board::{self, ChainItem, Game, Item};
use super::content::Content;
use super::economy::Economy;
use super::orders;
use rand::Rng;

/// 이 레벨부터 주문 칸이 `order.slotsAt5` 개, 보관함 +1 (명세 L-1 의 5레벨 행)
const SLOTS_LEVEL: u32 = 5;

/// 레벨 `level` 에 닿는 누적 ⭐. 표 밖은 마지막 값에 `step` 씩 더한다.
pub fn threshold(eco: &Economy, level: u32) -> i64 {
    let th = &eco.level.thresholds;
    let l = level as usize;
    if l <= 1 {
        0
    } else if l <= th.len() {
        th[l - 1]
    } else {
        th[th.len() - 1] + eco.level.step * (l - th.len()) as i64
    }
}

/// 누적 ⭐ 로 정해지는 레벨 (1 이상)
pub fn level_for(eco: &Economy, stars: i64) -> u32 {
    let mut l = 1;
    while threshold(eco, l + 1) <= stars {
        l += 1;
    }
    l
}

/// 다음 레벨에 닿는 누적 ⭐ (표 밖에서도 `step` 으로 이어지므로 늘 있다)
pub fn next_level_at(eco: &Economy, level: u32) -> i64 {
    threshold(eco, level + 1)
}

pub fn order_slots(eco: &Economy, level: u32) -> usize {
    if level >= SLOTS_LEVEL {
        eco.order.slots_at5
    } else {
        eco.order.slots
    }
}

/// `"clean:2"` 또는 `"any:4"` (해금된 아이템 계열 중 최고 레벨이 4 이상인 것 하나)
fn resolve(spec: &str, game: &Game, content: &Content, rng: &mut impl Rng) -> Option<ChainItem> {
    let Some(n) = spec.strip_prefix("any:") else { return ChainItem::parse(spec) };
    let n: u8 = n.parse().ok()?;
    let ok: Vec<&str> = content
        .active_item_chains(game.level)
        .into_iter()
        .filter(|id| content.max_level(id).is_some_and(|m| m >= n))
        .collect();
    (!ok.is_empty()).then(|| ChainItem::new(ok[rng.random_range(0..ok.len())], n))
}

fn give_rewards(game: &mut Game, content: &Content, level: u32, rng: &mut impl Rng) {
    let key = level.to_string();
    let rewards = content
        .level_rewards
        .get(&key)
        .or_else(|| (level > 10 && level.is_multiple_of(2)).then(|| content.level_rewards.get("evenAfter10")).flatten())
        .cloned()
        .unwrap_or_default();
    for r in rewards {
        if let Some(specs) = &r.gift {
            let items: Vec<ChainItem> = specs.iter().filter_map(|s| resolve(s, game, content, rng)).collect();
            if !items.is_empty() {
                board::place_or_queue(game, Item::Gift { items }, None);
            }
        }
        if let Some(spec) = &r.item {
            if let Some(it) = resolve(spec, game, content, rng) {
                board::place_or_queue(game, Item::Chain(it), None);
            }
        }
    }
}

/// ⭐ 가 늘어 올라야 할 레벨을 올리고 보상을 **한 번씩** 준다. 주문 칸도 새 레벨에 맞춰 채운다.
pub fn apply_level_ups(game: &mut Game, content: &Content, eco: &Economy, rng: &mut impl Rng) {
    let target = level_for(eco, game.stars);
    while game.level < target {
        game.level += 1;
        if game.level == SLOTS_LEVEL {
            game.inv_slots = (game.inv_slots + 1).min(eco.inv.max);
            game.inv.resize(game.inv_slots as usize, None);
        }
        give_rewards(game, content, game.level, rng);
    }
    orders::refill_slots(game, content, eco, rng);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::merge::board::tests::{free, rng, setup};
    use crate::providers::merge::board::Cell;

    #[test]
    fn thresholds_follow_the_table_then_step_80() {
        let (_, _, e) = setup();
        let t: Vec<i64> = (1..=12).map(|l| threshold(&e, l)).collect();
        assert_eq!(t, vec![0, 10, 25, 45, 70, 100, 140, 190, 250, 320, 400, 480]);
        assert_eq!(threshold(&e, 0), 0);
    }

    #[test]
    fn level_for_uses_inclusive_thresholds() {
        let (_, _, e) = setup();
        for (stars, level) in [(0, 1), (9, 1), (10, 2), (24, 2), (25, 3), (319, 9), (320, 10), (399, 10), (400, 11), (480, 12)] {
            assert_eq!(level_for(&e, stars), level, "{stars}");
        }
        assert_eq!(next_level_at(&e, 1), 10);
        assert_eq!(next_level_at(&e, 10), 400);
    }

    #[test]
    fn level_2_reward_is_a_gift_with_four_items_given_once() {
        let (mut g, c, e) = setup();
        g.stars = 10;
        let mut r = rng();
        apply_level_ups(&mut g, &c, &e, &mut r);
        assert_eq!(g.level, 2);
        let gifts: Vec<_> = g
            .board
            .iter()
            .filter_map(|x| match x {
                Cell::Free { item: Item::Gift { items } } => Some(items.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(gifts.len(), 1);
        assert_eq!(gifts[0].len(), 4);
        assert_eq!(gifts[0][0], ChainItem::new("clean", 2));
        // 다시 적용해도 두 번 주지 않는다
        apply_level_ups(&mut g, &c, &e, &mut r);
        assert_eq!(g.board.iter().filter(|x| matches!(x, Cell::Free { .. })).count(), 1);
    }

    #[test]
    fn jumping_several_levels_gives_every_reward_and_level_5_perks() {
        let (mut g, c, e) = setup();
        g.stars = 100; // Lv6
        apply_level_ups(&mut g, &c, &e, &mut rng());
        assert_eq!(g.level, 6);
        let free_chain = |id: &str| g.board.iter().any(|x| matches!(x, Cell::Free { item: Item::Chain(i) } if i.chain == id));
        assert!(free_chain("gen_box"), "Lv4 도구함");
        assert!(free_chain("gen_cart"), "Lv6 장바구니");
        assert_eq!(g.inv_slots, 5, "Lv5 보관함 +1");
        assert_eq!(g.inv.len(), 5);
        assert_eq!(g.orders.len(), 4, "Lv5 주문 칸 4개");
        assert!(g.pending_gifts.is_empty());
    }

    #[test]
    fn rewards_queue_when_board_and_storage_are_full() {
        let (mut g, c, e) = setup();
        g.board = vec![free("garden", 1); 63];
        g.inv = vec![Some(Item::chain("clean", 1)); 4];
        g.stars = 25; // Lv3 — Lv2 선물이 대기열로
        apply_level_ups(&mut g, &c, &e, &mut rng());
        assert_eq!(g.level, 3);
        assert_eq!(g.pending_gifts.len(), 1, "잃지 않는다");
    }

    #[test]
    fn any_n_rewards_pick_unlocked_chains_with_enough_levels() {
        let (mut g, c, e) = setup();
        g.level = 7;
        g.stars = 190; // Lv8 → 아무 계열 Lv4 ×2
        apply_level_ups(&mut g, &c, &e, &mut rng());
        assert_eq!(g.level, 8);
        let gift = g
            .board
            .iter()
            .find_map(|x| match x {
                Cell::Free { item: Item::Gift { items } } => Some(items.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(gift.len(), 2);
        for it in gift {
            assert_eq!(it.level, 4);
            assert!(c.chains[&it.chain].kind == crate::providers::merge::content::ChainKind::Item);
        }
    }

    #[test]
    fn even_levels_after_ten_give_a_lv5_gift() {
        let (mut g, c, e) = setup();
        g.level = 10;
        g.stars = 400; // 11: 보상 없음
        apply_level_ups(&mut g, &c, &e, &mut rng());
        assert_eq!(g.level, 11);
        assert!(g.board.iter().all(|x| matches!(x, Cell::Empty)));
        g.stars = 480; // 12: 짝수
        apply_level_ups(&mut g, &c, &e, &mut rng());
        assert_eq!(g.level, 12);
        assert_eq!(g.board.iter().filter(|x| matches!(x, Cell::Free { item: Item::Gift { .. } })).count(), 1);
    }

    #[test]
    fn level_never_decreases() {
        let (mut g, c, e) = setup();
        g.level = 5;
        g.stars = 0;
        apply_level_ups(&mut g, &c, &e, &mut rng());
        assert_eq!(g.level, 5);
        assert_eq!(g.orders.len(), 4);
    }
}
