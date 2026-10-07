//! 콘텐츠(계열·주민·스킨·레벨 보상). 규칙(id·레벨 수·해금)과 외형(이름·그림)을 한 곳에 두되
//! 보드는 id·레벨만 기억한다 (명세 I-5). 기본값은 `resources/merge-content.json`,
//! 첫 실행에 DB 로 시드하고 이후엔 DB 가 주인이다 (사용자가 도감에서 고친다).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DEFAULT_JSON: &str = include_str!("../../../resources/merge-content.json");

/// 그림. `{"emoji":"🫧"}` | `{"image":"data:…"}` | `{"text":"?"}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Icon {
    Emoji { emoji: String },
    Image { image: String },
    Text { text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChainKind {
    Item,
    Generator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelDef {
    pub name: String,
    pub icon: Icon,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainDef {
    pub name: String,
    pub kind: ChainKind,
    pub max_level: u8,
    pub unlock_level: u32,
    pub hidden: bool,
    pub emits: Option<Vec<String>>,
    pub levels: Vec<LevelDef>,
    #[serde(skip)]
    pub ord: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentDef {
    pub name: String,
    pub icon: Icon,
    /// 빈 배열 = 해금된 모든 계열 (곰)
    pub likes: Vec<String>,
    pub join_level: u32,
    pub hidden: bool,
    #[serde(skip)]
    pub ord: i64,
}

/// `levelRewards` 한 항목. `gift: ["clean:2", "any:4"]` 는 🎁 하나에 담기고, `item: "gen_box:1"` 은 그대로 놓인다.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Reward {
    pub gift: Option<Vec<String>>,
    pub item: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Content {
    pub chains: BTreeMap<String, ChainDef>,
    pub residents: BTreeMap<String, ResidentDef>,
    pub skins: BTreeMap<String, Icon>,
    #[serde(skip)]
    pub level_rewards: BTreeMap<String, Vec<Reward>>,
}

// --- 내장 JSON 의 원형 ---------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChain {
    id: String,
    kind: ChainKind,
    name: String,
    unlock_level: u32,
    #[serde(default)]
    levels: Vec<LevelDef>,
    max_level: Option<u8>,
    icon: Option<Icon>,
    emits: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResident {
    id: String,
    name: String,
    icon: Icon,
    likes: Vec<String>,
    join_level: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContent {
    chains: Vec<RawChain>,
    residents: Vec<RawResident>,
    skins: BTreeMap<String, Icon>,
    level_rewards: BTreeMap<String, Vec<Reward>>,
}

fn raw() -> RawContent {
    serde_json::from_str(DEFAULT_JSON).expect("bundled merge-content.json is valid")
}

fn expand_chain(c: RawChain, ord: i64) -> (String, ChainDef) {
    let (levels, max_level) = match c.kind {
        ChainKind::Item => {
            let n = c.levels.len() as u8;
            (c.levels, n)
        }
        ChainKind::Generator => {
            // 생산기는 레벨마다 같은 그림·이름 (카드 모서리 숫자가 레벨을 알려 준다 — I-6)
            let max = c.max_level.unwrap_or(5);
            let icon = c.icon.unwrap_or(Icon::Text { text: "?".into() });
            let lv = (0..max).map(|_| LevelDef { name: c.name.clone(), icon: icon.clone() }).collect();
            (lv, max)
        }
    };
    (
        c.id,
        ChainDef {
            name: c.name,
            kind: c.kind,
            max_level,
            unlock_level: c.unlock_level,
            hidden: false,
            emits: c.emits,
            levels,
            ord,
        },
    )
}

impl Content {
    /// 내장 JSON 을 메모리에서만 파싱한다 (DB 없이). 테스트·시드의 원본.
    pub fn builtin() -> Self {
        let r = raw();
        let chains = r.chains.into_iter().enumerate().map(|(i, c)| expand_chain(c, i as i64)).collect();
        let residents = r
            .residents
            .into_iter()
            .enumerate()
            .map(|(i, x)| {
                (
                    x.id,
                    ResidentDef {
                        name: x.name,
                        icon: x.icon,
                        likes: x.likes,
                        join_level: x.join_level,
                        hidden: false,
                        ord: i as i64,
                    },
                )
            })
            .collect();
        Content { chains, residents, skins: r.skins, level_rewards: r.level_rewards }
    }

    /// 없는 id 만 추가한다 (`builtin=1`). 사용자가 고친 행은 건드리지 않는다.
    pub fn seed(conn: &Connection) -> rusqlite::Result<()> {
        let b = Self::builtin();
        for (id, c) in &b.chains {
            let emits = c.emits.as_ref().map(|e| serde_json::to_string(e).unwrap_or_default());
            let n = conn.execute(
                "INSERT OR IGNORE INTO chains (id, kind, name, max_level, unlock_level, emits, hidden, builtin, ord)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 1, ?7)",
                params![
                    id,
                    if c.kind == ChainKind::Item { "item" } else { "generator" },
                    c.name,
                    c.max_level,
                    c.unlock_level,
                    emits,
                    c.ord
                ],
            )?;
            if n == 1 {
                for (i, lv) in c.levels.iter().enumerate() {
                    conn.execute(
                        "INSERT OR IGNORE INTO chain_levels (chain_id, level, name, icon) VALUES (?1, ?2, ?3, ?4)",
                        params![id, i as i64 + 1, lv.name, serde_json::to_string(&lv.icon).unwrap_or_default()],
                    )?;
                }
            }
        }
        for (id, r) in &b.residents {
            conn.execute(
                "INSERT OR IGNORE INTO residents (id, name, icon, likes, join_level, hidden, builtin, ord)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 1, ?6)",
                params![
                    id,
                    r.name,
                    serde_json::to_string(&r.icon).unwrap_or_default(),
                    serde_json::to_string(&r.likes).unwrap_or_default(),
                    r.join_level,
                    r.ord
                ],
            )?;
        }
        for (k, icon) in &b.skins {
            conn.execute(
                "INSERT OR IGNORE INTO skins (key, icon) VALUES (?1, ?2)",
                params![k, serde_json::to_string(icon).unwrap_or_default()],
            )?;
        }
        Ok(())
    }

    /// DB 에서 읽는다. 레벨 보상은 DB 에 두지 않고 내장값을 쓴다.
    pub fn load(conn: &Connection) -> rusqlite::Result<Self> {
        let parse_icon = |s: String| -> Icon {
            serde_json::from_str(&s).unwrap_or(Icon::Text { text: "?".into() })
        };
        let mut chains = BTreeMap::new();
        let mut st = conn.prepare(
            "SELECT id, kind, name, max_level, unlock_level, emits, hidden, ord FROM chains ORDER BY ord, id",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })?;
        let mut lv_st = conn.prepare("SELECT name, icon FROM chain_levels WHERE chain_id = ?1 ORDER BY level")?;
        for row in rows {
            let (id, kind, name, max_level, unlock, emits, hidden, ord) = row?;
            let levels = lv_st
                .query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|(name, icon)| LevelDef { name, icon: parse_icon(icon) })
                .collect();
            chains.insert(
                id,
                ChainDef {
                    name,
                    kind: if kind == "generator" { ChainKind::Generator } else { ChainKind::Item },
                    max_level: max_level.clamp(1, u8::MAX as i64) as u8,
                    unlock_level: unlock.max(1) as u32,
                    hidden: hidden != 0,
                    emits: emits.and_then(|e| serde_json::from_str(&e).ok()),
                    levels,
                    ord,
                },
            );
        }
        let mut residents = BTreeMap::new();
        let mut st = conn.prepare("SELECT id, name, icon, likes, join_level, hidden, ord FROM residents ORDER BY ord, id")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
        for row in rows {
            let (id, name, icon, likes, join, hidden, ord) = row?;
            residents.insert(
                id,
                ResidentDef {
                    name,
                    icon: parse_icon(icon),
                    likes: serde_json::from_str(&likes).unwrap_or_default(),
                    join_level: join.max(1) as u32,
                    hidden: hidden != 0,
                    ord,
                },
            );
        }
        let mut skins = BTreeMap::new();
        let mut st = conn.prepare("SELECT key, icon FROM skins")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (k, icon) = row?;
            skins.insert(k, parse_icon(icon));
        }
        Ok(Content { chains, residents, skins, level_rewards: raw().level_rewards })
    }

    // --- 조회 도우미 --------------------------------------------------------------------------

    pub fn chain(&self, id: &str) -> Option<&ChainDef> {
        self.chains.get(id)
    }

    pub fn max_level(&self, id: &str) -> Option<u8> {
        self.chains.get(id).map(|c| c.max_level)
    }

    pub fn is_generator(&self, id: &str) -> bool {
        self.chains.get(id).is_some_and(|c| c.kind == ChainKind::Generator)
    }

    /// 플레이어 레벨에서 해금됐고 숨기지 않은 **아이템** 계열 id (생산기 제외).
    pub fn active_item_chains(&self, player_level: u32) -> Vec<&str> {
        self.chains
            .iter()
            .filter(|(_, c)| c.kind == ChainKind::Item && !c.hidden && c.unlock_level <= player_level)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// 입주했고 숨기지 않은 주민 id, 입주 순서대로.
    pub fn active_residents(&self, player_level: u32) -> Vec<&str> {
        let mut v: Vec<(&String, &ResidentDef)> =
            self.residents.iter().filter(|(_, r)| !r.hidden && r.join_level <= player_level).collect();
        v.sort_by_key(|(id, r)| (r.ord, (*id).clone()));
        v.into_iter().map(|(id, _)| id.as_str()).collect()
    }

    /// 주민이 주문·선물에 쓸 수 있는 계열. 비어 있으면 "전부" (곰).
    pub fn liked_chains(&self, resident: &str, player_level: u32) -> Vec<&str> {
        let active = self.active_item_chains(player_level);
        match self.residents.get(resident) {
            Some(r) if !r.likes.is_empty() => active.into_iter().filter(|c| r.likes.iter().any(|l| l == c)).collect(),
            Some(_) => active,
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::merge::board;

    #[test]
    fn builtin_content_is_consistent() {
        let c = Content::builtin();
        for (id, ch) in &c.chains {
            assert_eq!(ch.levels.len() as u8, ch.max_level, "{id}");
            assert!(ch.max_level >= 1);
            match ch.kind {
                ChainKind::Item => assert!(ch.emits.is_none(), "{id}"),
                ChainKind::Generator => {
                    assert_eq!(ch.max_level, 5, "{id}");
                    let emits = ch.emits.as_ref().expect("생산기는 emits 가 있다");
                    assert!(!emits.is_empty());
                    for e in emits {
                        assert_eq!(c.chain(e).map(|x| x.kind), Some(ChainKind::Item), "{id} emits {e}");
                    }
                }
            }
        }
        assert_eq!(c.chains["clean"].max_level, 8);
        assert_eq!(c.chains["garden"].max_level, 8);
        assert_eq!(c.chains["cook"].max_level, 7);
        assert_eq!(c.chains["tools"].max_level, 7);
        assert_eq!(c.chains["sew"].max_level, 7);
        for (id, r) in &c.residents {
            for l in &r.likes {
                assert!(c.chains.contains_key(l), "{id} likes {l}");
            }
        }
        assert_eq!(c.residents["bear"].likes.len(), 0);
        for k in ["clicker", "box", "web", "gift"] {
            assert!(c.skins.contains_key(k), "skin {k}");
        }
    }

    #[test]
    fn level_rewards_reference_known_chains() {
        let c = Content::builtin();
        let check = |spec: &str| {
            if let Some(n) = spec.strip_prefix("any:") {
                assert!(n.parse::<u8>().is_ok(), "{spec}");
                return;
            }
            let it = board::ChainItem::parse(spec).unwrap_or_else(|| panic!("bad spec {spec}"));
            let max = c.max_level(&it.chain).unwrap_or_else(|| panic!("unknown chain in {spec}"));
            assert!(it.level >= 1 && it.level <= max, "{spec}");
        };
        for rewards in c.level_rewards.values() {
            for r in rewards {
                for g in r.gift.iter().flatten() {
                    check(g);
                }
                if let Some(i) = &r.item {
                    check(i);
                }
            }
        }
        assert!(c.level_rewards.contains_key("evenAfter10"));
    }

    #[test]
    fn serializes_to_contract_shape() {
        let c = Content::builtin();
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["chains"]["clean"]["kind"], "item");
        assert_eq!(v["chains"]["clean"]["maxLevel"], 8);
        assert_eq!(v["chains"]["clean"]["levels"][0]["icon"]["emoji"], "🫧");
        assert_eq!(v["chains"]["gen_box"]["kind"], "generator");
        assert_eq!(v["chains"]["gen_box"]["emits"][0], "clean");
        assert!(v["chains"]["clean"]["emits"].is_null());
        assert_eq!(v["residents"]["parrot"]["joinLevel"], 1);
        assert_eq!(v["skins"]["clicker"]["emoji"], "🐹");
        assert!(v.get("levelRewards").is_none() && v.get("level_rewards").is_none());
    }

    #[test]
    fn helpers_respect_unlock_and_hidden() {
        let mut c = Content::builtin();
        assert_eq!(c.active_item_chains(1), vec!["clean", "garden"]);
        assert_eq!(c.active_item_chains(5).len(), 4);
        c.chains.get_mut("garden").unwrap().hidden = true;
        assert_eq!(c.active_item_chains(1), vec!["clean"]);
        assert_eq!(c.active_residents(1), vec!["parrot", "rabbit"]);
        assert_eq!(c.active_residents(9).len(), 6);
        assert_eq!(c.liked_chains("bear", 5), vec!["clean", "cook", "tools"]);
        assert_eq!(c.liked_chains("rabbit", 1), Vec::<&str>::new());
    }
}
