//! 조정값(명세 §16). 기본값은 바이너리에 포함된 `resources/merge-economy.json`,
//! 앱 데이터 폴더의 같은 이름 파일이 있으면 키별로 깊게 덮어쓴다 (배열은 통째로 교체).

use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

const DEFAULT_JSON: &str = include_str!("../../../resources/merge-economy.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Economy {
    pub board: BoardEco,
    #[serde(rename = "gen")]
    pub generator: GenEco,
    #[serde(rename = "box")]
    pub chest: BoxEco,
    pub sell: SellEco,
    pub inv: InvEco,
    pub order: OrderEco,
    pub resident: ResidentEco,
    pub level: LevelEco,
    pub energy: EnergyEco,
    pub regen: RegenEco,
    pub clicker: ClickerEco,
    pub custom: CustomEco,
    #[allow(dead_code)]
    pub hint: HintEco,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoardEco {
    pub w: usize,
    pub h: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenEco {
    pub cost: i64,
    /// `drop_table[생산기 레벨 - 1] = [Lv1 %, Lv2 %, Lv3 %]`
    pub drop_table: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxEco {
    pub open_cost: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellEco {
    pub refund: f64,
    pub confirm_from_level: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvEco {
    pub start: u8,
    pub max: u8,
    pub costs: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderEco {
    pub slots: usize,
    pub slots_at5: usize,
    pub two_item_chance: f64,
    pub two_item_from_level: u32,
    pub level_base: u8,
    pub level_span: u8,
    pub star_per_value: f64,
    pub energy_per_value: f64,
    pub reroll_cost: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentEco {
    pub heart_milestones: Vec<u32>,
    pub heart_gift_levels: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelEco {
    pub thresholds: Vec<i64>,
    pub step: i64,
}

/// `daily` 이하는 P1(할 일)에서 쓴다 — 지금은 `start` 만 읽는다.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnergyEco {
    pub start: i64,
    pub daily: Vec<i64>,
    pub weekly: Vec<i64>,
    pub all_clear_daily: i64,
    pub all_clear_weekly: i64,
    pub streak_per_day: i64,
    pub streak_max: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegenEco {
    pub interval_sec: u64,
    pub cap: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickerEco {
    pub clicks_per_energy: u32,
    pub clicks_per_key: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEco {
    pub min_levels: u8,
    pub max_levels: u8,
}

/// H-2 유휴 힌트(프론트 `hints`)에서 쓴다.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HintEco {
    pub idle_sec: u32,
}

/// `over` 의 객체는 키별로 재귀 병합하고, 그 밖의 값(배열 포함)은 통째로 교체한다.
pub fn deep_merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => deep_merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

impl Economy {
    pub fn builtin() -> Self {
        let e: Economy = serde_json::from_str(DEFAULT_JSON).expect("bundled merge-economy.json is valid");
        e.validate().expect("bundled merge-economy.json passes validation");
        e
    }

    /// 내장값 위에 사용자 파일을 키별로 덮어쓴다. 파일이 없으면 내장값 그대로,
    /// 깨졌거나 검증에 실패하면 경고 후 **내장값 전체**를 쓴다.
    pub fn load_with_override(user_file: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(user_file) else {
            return Self::builtin();
        };
        match Self::merged_from(&text) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("merge: ignoring invalid {}: {e}", user_file.display());
                Self::builtin()
            }
        }
    }

    fn merged_from(override_text: &str) -> Result<Self, String> {
        let over: Value = serde_json::from_str(override_text).map_err(|e| e.to_string())?;
        let mut base: Value = serde_json::from_str(DEFAULT_JSON).map_err(|e| e.to_string())?;
        deep_merge(&mut base, over);
        let eco: Economy = serde_json::from_value(base).map_err(|e| e.to_string())?;
        eco.validate()?;
        Ok(eco)
    }

    pub fn validate(&self) -> Result<(), String> {
        let bad = |m: &str| Err(m.to_string());
        if self.board.w != 7 || self.board.h != 9 {
            return bad("board 는 7×9 로 고정입니다");
        }
        if self.generator.cost < 0 || self.chest.open_cost < 0 || self.order.reroll_cost < 0 {
            return bad("비용은 음수일 수 없습니다");
        }
        if self.generator.drop_table.is_empty() {
            return bad("gen.dropTable 이 비었습니다");
        }
        if self.generator.drop_table.iter().any(|r| r.iter().sum::<u32>() != 100) {
            return bad("gen.dropTable 의 각 줄 합은 100 이어야 합니다");
        }
        if !(0.0..=1.0).contains(&self.sell.refund) {
            return bad("sell.refund 는 0~1 이어야 합니다");
        }
        if self.inv.start == 0 || self.inv.start > self.inv.max {
            return bad("inv.start 는 1 이상, inv.max 이하여야 합니다");
        }
        if self.inv.costs.len() < (self.inv.max - self.inv.start) as usize {
            return bad("inv.costs 가 모자랍니다");
        }
        if self.order.slots == 0 || self.order.slots_at5 < self.order.slots {
            return bad("order.slots 가 올바르지 않습니다");
        }
        if !(0.0..=1.0).contains(&self.order.two_item_chance) {
            return bad("order.twoItemChance 는 0~1 이어야 합니다");
        }
        if self.order.level_base == 0 || self.order.level_span == 0 {
            return bad("order.levelBase/levelSpan 은 1 이상이어야 합니다");
        }
        if !self.order.star_per_value.is_finite()
            || self.order.star_per_value <= 0.0
            || !self.order.energy_per_value.is_finite()
            || self.order.energy_per_value < 0.0
        {
            return bad("order.starPerValue/energyPerValue 가 올바르지 않습니다");
        }
        let hm = &self.resident.heart_milestones;
        if hm.is_empty()
            || hm.len() != self.resident.heart_gift_levels.len()
            || hm.windows(2).any(|w| w[0] >= w[1])
        {
            return bad("resident.heartMilestones/heartGiftLevels 가 올바르지 않습니다");
        }
        let th = &self.level.thresholds;
        if th.first() != Some(&0) || th.windows(2).any(|w| w[0] >= w[1]) || self.level.step <= 0 {
            return bad("level.thresholds 는 0 에서 시작해 늘어나야 하고 step 은 양수여야 합니다");
        }
        if self.regen.interval_sec == 0 || self.regen.cap < 0 {
            return bad("regen 값이 올바르지 않습니다");
        }
        if self.clicker.clicks_per_energy == 0
            || !self.clicker.clicks_per_key.is_finite()
            || self.clicker.clicks_per_key < 0.0
        {
            return bad("clicker 값이 올바르지 않습니다");
        }
        if self.custom.min_levels > self.custom.max_levels {
            return bad("custom.minLevels 가 maxLevels 보다 큽니다");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_is_valid_and_drop_rows_sum_100() {
        let e = Economy::builtin();
        assert_eq!(e.generator.drop_table.len(), 5);
        for row in &e.generator.drop_table {
            assert_eq!(row.iter().sum::<u32>(), 100);
        }
        assert_eq!(e.inv.costs[..3], [20, 40, 80]);
        assert_eq!(e.order.slots_at5, 4);
    }

    #[test]
    fn deep_merge_overrides_only_given_keys() {
        let e = Economy::merged_from(r#"{"regen":{"intervalSec":60},"box":{"openCost":9}}"#).unwrap();
        assert_eq!(e.regen.interval_sec, 60);
        assert_eq!(e.regen.cap, 100, "형제 키는 그대로");
        assert_eq!(e.chest.open_cost, 9);
        assert_eq!(e.generator.cost, 1);
    }

    #[test]
    fn arrays_are_replaced_not_merged() {
        let e = Economy::merged_from(r#"{"inv":{"costs":[1,2,3,4,5,6,7,8,9]}}"#).unwrap();
        assert_eq!(e.inv.costs, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn broken_override_files_fall_back_to_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("merge-economy.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Economy::load_with_override(&p).regen.interval_sec, 120);
        // 검증 실패 (드롭 표 합 != 100) 면 다른 키도 적용하지 않는다
        std::fs::write(&p, r#"{"regen":{"intervalSec":30},"gen":{"dropTable":[[90,0,0]]}}"#).unwrap();
        assert_eq!(Economy::load_with_override(&p).regen.interval_sec, 120);
        // 타입이 틀려도 마찬가지
        std::fs::write(&p, r#"{"regen":{"intervalSec":"x"}}"#).unwrap();
        assert_eq!(Economy::load_with_override(&p).regen.interval_sec, 120);
        // 정상 파일은 적용
        std::fs::write(&p, r#"{"regen":{"intervalSec":30}}"#).unwrap();
        assert_eq!(Economy::load_with_override(&p).regen.interval_sec, 30);
        // 파일 없음
        assert_eq!(Economy::load_with_override(&dir.path().join("nope.json")).regen.interval_sec, 120);
    }
}
