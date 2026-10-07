//! 보드와 아이템 — 명세 §2~§8 의 순수 규칙. `now`·rng 는 인자로 받는다 (테스트에서 고정).
//! 칸 번호는 `y*7 + x`, 보관함 칸은 `100 + 슬롯` 으로 같은 번호 공간에 둔다.

use super::content::Content;
use super::economy::Economy;
use super::energy::{self, Fx};
use super::orders::{self, Order};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const W: usize = 7;
pub const H: usize = 9;
pub const CELLS: usize = W * H;
/// 이 번호 이상이면 보관함 칸 (`n - 100` 번째)
pub const STORAGE_BASE: usize = 100;
/// 보드 한가운데 (3, 4)
pub const CENTER: usize = 4 * W + 3;
pub const CURRENT_VERSION: u32 = 1;

const BOARD_JSON: &str = include_str!("../../../resources/merge-board.json");

/// 규칙 위반. 문구는 그대로 프론트의 경고 줄에 뜬다.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("에너지가 부족합니다")]
    NoEnergy,
    #[error("보드가 가득 찼어요")]
    BoardFull,
    #[error("그 칸은 옮길 수 없습니다")]
    NotMovable,
    #[error("합칠 수 없는 조합입니다")]
    CannotMerge,
    #[error("최고 레벨입니다")]
    MaxLevel,
    #[error("생산기는 팔 수 없습니다")]
    GeneratorNotSellable,
    #[error("선물은 팔 수 없습니다")]
    GiftNotSellable,
    #[error("보관함은 더 늘릴 수 없습니다")]
    InvMax,
    #[error("주문을 채울 아이템이 없습니다")]
    NoItems,
    #[error("잘못된 칸 번호입니다")]
    BadCell,
    #[error("도감에서 계열을 연결하세요")]
    NoEmits,
    #[error("잘못된 단계입니다")]
    BadStep,
    #[error("상자는 먼저 열어야 합니다")]
    BoxClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainItem {
    pub chain: String,
    pub level: u8,
}

impl ChainItem {
    pub fn new(chain: &str, level: u8) -> Self {
        Self { chain: chain.to_string(), level }
    }

    /// `"clean:2"` → `ChainItem`
    pub fn parse(spec: &str) -> Option<Self> {
        let (chain, level) = spec.rsplit_once(':')?;
        let level: u8 = level.parse().ok()?;
        (level >= 1 && !chain.is_empty()).then(|| Self::new(chain, level))
    }

    /// `u(n) = 2^(n-1)` — Lv1 몇 개분인가 (명세 I-2)
    pub fn value(&self) -> i64 {
        1i64 << (self.level.max(1) - 1).min(40)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Item {
    Chain(ChainItem),
    Gift { items: Vec<ChainItem> },
}

impl Item {
    pub fn chain(chain: &str, level: u8) -> Self {
        Item::Chain(ChainItem::new(chain, level))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Cell {
    Empty,
    Free { item: Item },
    Web { item: Item },
    Box { item: Item },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Game {
    pub v: u32,
    pub board: Vec<Cell>,
    pub inv: Vec<Option<Item>>,
    pub inv_slots: u8,
    /// 놓을 곳이 없어 쌓인 받을 선물 (레벨업 보상을 잃지 않는다 — L-2)
    pub pending_gifts: Vec<Item>,
    pub orders: Vec<Order>,
    pub order_rr: usize,
    pub affection: BTreeMap<String, u32>,
    pub stars: i64,
    pub level: u32,
    /// 장부 합의 캐시
    pub energy: i64,
    /// 마지막으로 시간 회복을 정산한 시각 (unix ms)
    pub regen_anchor: i64,
    pub clicker_rem: f64,
    pub key_clicker: bool,
    pub key_notice_seen: bool,
    pub tutorial_step: u8,
    pub day_start_hour: u8,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            v: CURRENT_VERSION,
            board: vec![Cell::Empty; CELLS],
            inv: vec![None; 4],
            inv_slots: 4,
            pending_gifts: Vec::new(),
            orders: Vec::new(),
            order_rr: 0,
            affection: BTreeMap::new(),
            stars: 0,
            level: 1,
            energy: 0,
            regen_anchor: 0,
            clicker_rem: 0.0,
            key_clicker: false,
            key_notice_seen: false,
            tutorial_step: 0,
            day_start_hour: 0,
        }
    }
}

impl Game {
    /// 저장된 JSON 이 깨져 있어도(칸 수·보관함 길이) 규칙 함수가 패닉하지 않게 맞춘다.
    pub fn normalize(&mut self) {
        self.board.resize(CELLS, Cell::Empty);
        self.inv.resize(self.inv_slots as usize, None);
        self.level = self.level.max(1);
    }
}

// --- 칸 번호 -----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pos {
    Board(usize),
    Storage(usize),
}

fn decode(game: &Game, idx: usize) -> Result<Pos, RuleError> {
    if idx < CELLS {
        Ok(Pos::Board(idx))
    } else if idx >= STORAGE_BASE && idx - STORAGE_BASE < game.inv.len() {
        Ok(Pos::Storage(idx - STORAGE_BASE))
    } else {
        Err(RuleError::BadCell)
    }
}

fn free_item(game: &Game, pos: Pos) -> Option<&Item> {
    match pos {
        Pos::Board(i) => match &game.board[i] {
            Cell::Free { item } => Some(item),
            _ => None,
        },
        Pos::Storage(i) => game.inv[i].as_ref(),
    }
}

fn clear(game: &mut Game, pos: Pos) {
    match pos {
        Pos::Board(i) => game.board[i] = Cell::Empty,
        Pos::Storage(i) => game.inv[i] = None,
    }
}

fn put_free(game: &mut Game, pos: Pos, item: Item) {
    match pos {
        Pos::Board(i) => game.board[i] = Cell::Free { item },
        Pos::Storage(i) => game.inv[i] = Some(item),
    }
}

pub fn xy(idx: usize) -> (usize, usize) {
    (idx % W, idx / W)
}

/// 상하좌우 4칸 (대각선은 이웃이 아니다 — B-3)
pub fn neighbors(idx: usize) -> Vec<usize> {
    let (x, y) = xy(idx);
    let mut v = Vec::with_capacity(4);
    if y > 0 {
        v.push(idx - W);
    }
    if x > 0 {
        v.push(idx - 1);
    }
    if x + 1 < W {
        v.push(idx + 1);
    }
    if y + 1 < H {
        v.push(idx + W);
    }
    v
}

/// 가장 가까운 빈 칸 — 맨해튼 거리, 같으면 위쪽 → 왼쪽 (G-2)
pub fn nearest_empty(board: &[Cell], from: usize) -> Option<usize> {
    let (fx, fy) = xy(from);
    board
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, Cell::Empty))
        .min_by_key(|(i, _)| {
            let (x, y) = xy(*i);
            (x.abs_diff(fx) + y.abs_diff(fy), y, x)
        })
        .map(|(i, _)| i)
}

// --- 합성 ---------------------------------------------------------------------------------------

/// 같은 아이템 둘을 합친 결과 레벨. 최고 레벨이면 `MaxLevel`, 같은 아이템이 아니면 `CannotMerge`.
pub fn merge_check(content: &Content, a: &Item, b: &Item) -> Result<u8, RuleError> {
    match (a, b) {
        (Item::Chain(x), Item::Chain(y)) if x == y => match content.max_level(&x.chain) {
            Some(m) if x.level < m => Ok(x.level + 1),
            _ => Err(RuleError::MaxLevel),
        },
        _ => Err(RuleError::CannotMerge),
    }
}

fn is_mergeable(content: &Content, item: &Item) -> bool {
    matches!(item, Item::Chain(c) if content.max_level(&c.chain).is_some_and(|m| c.level < m))
}

/// W-1·W-3: 합성이 일어난 칸의 이웃 — 상자는 web 이 되고, 더는 합칠 수 없는 web(최고 레벨·🎁)은 풀린다.
fn open_around(game: &mut Game, content: &Content, idx: usize) {
    for n in neighbors(idx) {
        match game.board[n].clone() {
            Cell::Box { item } => game.board[n] = Cell::Web { item },
            Cell::Web { item } if !is_mergeable(content, &item) => game.board[n] = Cell::Free { item },
            _ => {}
        }
    }
}

fn merge_into(game: &mut Game, content: &Content, src: Pos, dst: usize, chain: &str, level: u8) {
    clear(game, src);
    game.board[dst] = Cell::Free { item: Item::chain(chain, level) };
    open_around(game, content, dst);
}

/// 끌어 놓기 (M-1). `to`/`from` ≥ 100 은 보관함. 아무 일 없는 경우(상자·다른 web)는 `Ok` 로 둔다.
pub fn move_item(game: &mut Game, content: &Content, from: usize, to: usize) -> Result<(), RuleError> {
    let src = decode(game, from)?;
    let dst = decode(game, to)?;
    let item = free_item(game, src).cloned().ok_or(RuleError::NotMovable)?;
    if src == dst {
        return Ok(());
    }
    match dst {
        // 보관함 안·보관함으로는 합치지 않는다 (V-2)
        Pos::Storage(d) => {
            let other = game.inv[d].take();
            clear(game, src);
            game.inv[d] = Some(item);
            if let Some(o) = other {
                put_free(game, src, o);
            }
        }
        Pos::Board(d) => match game.board[d].clone() {
            Cell::Empty => {
                clear(game, src);
                game.board[d] = Cell::Free { item };
            }
            Cell::Free { item: other } => {
                if item == other && matches!(item, Item::Chain(_)) {
                    let next = merge_check(content, &item, &other)?;
                    if let Item::Chain(c) = &item {
                        merge_into(game, content, src, d, &c.chain, next);
                    }
                } else {
                    clear(game, src);
                    game.board[d] = Cell::Free { item };
                    put_free(game, src, other);
                }
            }
            // web 은 같은 아이템만 받는다 — 합성되며 풀리고, 이것도 합성이라 W-1 이 또 일어난다 (W-2)
            Cell::Web { item: other } => {
                if item == other && matches!(item, Item::Chain(_)) {
                    let next = merge_check(content, &item, &other)?;
                    if let Item::Chain(c) = &item {
                        merge_into(game, content, src, d, &c.chain, next);
                    }
                }
            }
            Cell::Box { .. } => {}
        },
    }
    Ok(())
}

// --- 생산 ---------------------------------------------------------------------------------------

/// 생산기를 눌러 아이템 하나를 만든다 (G-1~G-5). 검사는 모두 과금 **전에** 한다 — 오류면 아무것도 쓰지 않는다.
/// 놓인 칸 번호를 돌려준다.
pub fn spawn_from_generator(
    game: &mut Game,
    content: &Content,
    eco: &Economy,
    idx: usize,
    rng: &mut impl Rng,
    fx: &mut Fx,
) -> Result<usize, RuleError> {
    let pos = decode(game, idx)?;
    let gen = match free_item(game, pos) {
        Some(Item::Chain(c)) if content.is_generator(&c.chain) => c.clone(),
        _ => return Err(RuleError::NotMovable),
    };
    let Pos::Board(origin) = pos else { return Err(RuleError::NotMovable) };
    let emits = content.chain(&gen.chain).and_then(|c| c.emits.as_ref()).cloned().unwrap_or_default();
    let eligible: Vec<&str> = emits
        .iter()
        .map(String::as_str)
        .filter(|id| {
            content
                .chain(id)
                .is_some_and(|c| c.kind == super::content::ChainKind::Item && !c.hidden && c.unlock_level <= game.level)
        })
        .collect();
    if eligible.is_empty() {
        return Err(RuleError::NoEmits);
    }
    let spot = nearest_empty(&game.board, origin).ok_or(RuleError::BoardFull)?;

    let chain = eligible[rng.random_range(0..eligible.len())];
    let table = &eco.generator.drop_table;
    let row = table[(gen.level as usize).saturating_sub(1).min(table.len() - 1)];
    let roll = rng.random_range(0..100u32);
    let mut level = 1u8;
    let mut acc = 0;
    for (i, p) in row.iter().enumerate() {
        acc += p;
        if roll < acc {
            level = i as u8 + 1;
            break;
        }
    }
    let level = level.min(content.max_level(chain).unwrap_or(1));

    energy::spend(game, eco.generator.cost, "spend:spawn", fx)?;
    game.board[spot] = Cell::Free { item: Item::chain(chain, level) };
    Ok(spot)
}

/// 🎁 를 눌러 안의 아이템을 하나 꺼낸다 (S-1). 다 나오면 🎁 가 사라진다. 놓인 칸 번호를 돌려준다.
pub fn pop_gift(game: &mut Game, idx: usize) -> Result<usize, RuleError> {
    let pos = decode(game, idx)?;
    let items = match free_item(game, pos) {
        Some(Item::Gift { items }) => items.clone(),
        _ => return Err(RuleError::NotMovable),
    };
    let origin = match pos {
        Pos::Board(i) => i,
        Pos::Storage(_) => CENTER,
    };
    let Some((first, rest)) = items.split_first() else {
        clear(game, pos);
        return Ok(origin);
    };
    let last = rest.is_empty();
    let spot = match nearest_empty(&game.board, origin) {
        Some(s) => s,
        // 마지막 한 개는 🎁 가 있던 자리에 그대로 놓을 수 있다
        None if last && matches!(pos, Pos::Board(_)) => origin,
        None => return Err(RuleError::BoardFull),
    };
    if last {
        clear(game, pos);
    } else {
        put_free(game, pos, Item::Gift { items: rest.to_vec() });
    }
    game.board[spot] = Cell::Free { item: Item::Chain(first.clone()) };
    Ok(spot)
}

/// 칸을 눌렀을 때: 생산기 = 생산, 🎁 = 꺼내기, 상자 = 오류(프론트가 확인 뒤 `open_box`), 그 밖은 아무 일 없음.
pub fn tap(
    game: &mut Game,
    content: &Content,
    eco: &Economy,
    idx: usize,
    rng: &mut impl Rng,
    fx: &mut Fx,
) -> Result<(), RuleError> {
    let pos = decode(game, idx)?;
    if let Pos::Board(i) = pos {
        if matches!(game.board[i], Cell::Box { .. }) {
            return Err(RuleError::BoxClosed);
        }
    }
    match free_item(game, pos) {
        Some(Item::Gift { .. }) => pop_gift(game, idx).map(|_| ()),
        Some(Item::Chain(c)) if content.is_generator(&c.chain) && matches!(pos, Pos::Board(_)) => {
            spawn_from_generator(game, content, eco, idx, rng, fx).map(|_| ())
        }
        _ => Ok(()),
    }
}

/// 상자 바로 열기 (W-4) — web 이 될 뿐 이웃은 열리지 않는다.
pub fn open_box(game: &mut Game, eco: &Economy, idx: usize, fx: &mut Fx) -> Result<(), RuleError> {
    let Pos::Board(i) = decode(game, idx)? else { return Err(RuleError::NotMovable) };
    let Cell::Box { item } = game.board[i].clone() else { return Err(RuleError::NotMovable) };
    energy::spend(game, eco.chest.open_cost, "spend:box", fx)?;
    game.board[i] = Cell::Web { item };
    Ok(())
}

// --- 판매·보관함 ---------------------------------------------------------------------------------

/// `floor(u(n) × refund)` (M-3)
pub fn sell_value(eco: &Economy, level: u8) -> i64 {
    let v = ChainItem::new("", level).value() as f64;
    (v * eco.sell.refund + 1e-9).floor() as i64
}

pub fn sell(game: &mut Game, content: &Content, eco: &Economy, idx: usize, fx: &mut Fx) -> Result<i64, RuleError> {
    let pos = decode(game, idx)?;
    let level = match free_item(game, pos) {
        None => return Err(RuleError::NotMovable),
        Some(Item::Gift { .. }) => return Err(RuleError::GiftNotSellable),
        Some(Item::Chain(c)) if content.is_generator(&c.chain) => return Err(RuleError::GeneratorNotSellable),
        Some(Item::Chain(c)) => c.level,
    };
    let refund = sell_value(eco, level);
    clear(game, pos);
    energy::credit(game, refund, "sell", None, fx);
    Ok(refund)
}

/// 다음 칸 늘리기 비용. 이미 최대면 `None`.
pub fn inv_next_cost(game: &Game, eco: &Economy) -> Option<i64> {
    if game.inv_slots >= eco.inv.max {
        return None;
    }
    let i = game.inv_slots.saturating_sub(eco.inv.start) as usize;
    eco.inv.costs.get(i).copied().or_else(|| eco.inv.costs.last().map(|c| c.saturating_mul(2)))
}

pub fn inv_expand(game: &mut Game, eco: &Economy, fx: &mut Fx) -> Result<(), RuleError> {
    let cost = inv_next_cost(game, eco).ok_or(RuleError::InvMax)?;
    energy::spend(game, cost, "spend:inv", fx)?;
    game.inv_slots += 1;
    game.inv.resize(game.inv_slots as usize, None);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Board(usize),
    Storage(usize),
    Queued,
}

fn try_place(game: &mut Game, item: Item, near: Option<usize>) -> Result<Placement, Item> {
    if let Some(i) = nearest_empty(&game.board, near.unwrap_or(CENTER)) {
        game.board[i] = Cell::Free { item };
        return Ok(Placement::Board(i));
    }
    if let Some(s) = game.inv.iter().position(Option::is_none) {
        game.inv[s] = Some(item);
        return Ok(Placement::Storage(s));
    }
    Err(item)
}

/// 보상·선물을 놓는다: 보드(가장 가까운 빈 칸, 기준이 없으면 가운데) → 보관함 → 받을 선물 줄 (L-2).
pub fn place_or_queue(game: &mut Game, item: Item, near: Option<usize>) -> Placement {
    match try_place(game, item, near) {
        Ok(p) => p,
        Err(item) => {
            game.pending_gifts.push(item);
            Placement::Queued
        }
    }
}

/// 받을 선물 줄에서 하나 꺼내 놓는다. 놓을 곳이 없으면 `BoardFull` (줄은 그대로).
pub fn claim_gift(game: &mut Game) -> Result<(), RuleError> {
    let Some(first) = game.pending_gifts.first().cloned() else { return Ok(()) };
    try_place(game, first, None).map_err(|_| RuleError::BoardFull)?;
    game.pending_gifts.remove(0);
    Ok(())
}

// --- 시작 상태 -------------------------------------------------------------------------------------

pub struct StartBoard {
    pub cells: Vec<Cell>,
    pub orders: Vec<Order>,
}

#[derive(Deserialize)]
struct RawBoard {
    cells: Vec<Vec<String>>,
    gifts: BTreeMap<String, Vec<String>>,
    orders: Vec<BTreeMap<String, Vec<String>>>,
}

/// `merge-board.json` 의 칸 인코딩: `""` · `"clean:1"` · `"web:clean:2"` · `"box:garden:1"` · `"box:gift:a"` · `"gen_box:1"`
pub fn parse_start_board(text: &str) -> Result<StartBoard, String> {
    let raw: RawBoard = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if raw.cells.len() != H || raw.cells.iter().any(|r| r.len() != W) {
        return Err(format!("시작 보드는 {H}행 {W}열이어야 합니다"));
    }
    let item_of = |spec: &str| ChainItem::parse(spec).ok_or_else(|| format!("잘못된 아이템 {spec:?}"));
    let mut cells = Vec::with_capacity(CELLS);
    for spec in raw.cells.iter().flatten() {
        let (state, rest) = match spec.split_once(':') {
            Some(("web", r)) => ("web", r),
            Some(("box", r)) => ("box", r),
            _ => ("free", spec.as_str()),
        };
        if spec.is_empty() {
            cells.push(Cell::Empty);
            continue;
        }
        let item = match rest.strip_prefix("gift:") {
            Some(key) => {
                let list = raw.gifts.get(key).ok_or_else(|| format!("없는 선물 {key:?}"))?;
                Item::Gift { items: list.iter().map(|s| item_of(s)).collect::<Result<_, _>>()? }
            }
            None => Item::Chain(item_of(rest)?),
        };
        cells.push(match state {
            "web" => Cell::Web { item },
            "box" => Cell::Box { item },
            _ => Cell::Free { item },
        });
    }
    let mut orders = Vec::new();
    for o in &raw.orders {
        for (resident, wants) in o {
            let wants = wants.iter().map(|s| item_of(s)).collect::<Result<_, _>>()?;
            orders.push(Order { resident: resident.clone(), wants });
        }
    }
    Ok(StartBoard { cells, orders })
}

/// 새 게임: 시작 보드 + 시작 주문 + 시작 ⚡ (장부의 `start` 행은 저장소가 함께 쓴다).
pub fn new_game(eco: &Economy, content: &Content, now_ms: i64, rng: &mut impl Rng) -> Game {
    let sb = parse_start_board(BOARD_JSON).expect("bundled merge-board.json is valid");
    let mut g = Game {
        board: sb.cells,
        order_rr: sb.orders.len(),
        orders: sb.orders,
        inv_slots: eco.inv.start,
        inv: vec![None; eco.inv.start as usize],
        energy: eco.energy.start,
        regen_anchor: now_ms,
        ..Game::default()
    };
    orders::refill_slots(&mut g, content, eco, rng);
    g
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    pub fn setup() -> (Game, Content, Economy) {
        (Game::default(), Content::builtin(), Economy::builtin())
    }

    pub fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    pub fn at(x: usize, y: usize) -> usize {
        y * W + x
    }

    pub fn free(chain: &str, level: u8) -> Cell {
        Cell::Free { item: Item::chain(chain, level) }
    }

    fn web(chain: &str, level: u8) -> Cell {
        Cell::Web { item: Item::chain(chain, level) }
    }

    fn boxed(chain: &str, level: u8) -> Cell {
        Cell::Box { item: Item::chain(chain, level) }
    }

    fn webs_count(g: &Game) -> usize {
        g.board.iter().filter(|c| matches!(c, Cell::Web { .. })).count()
    }

    #[test]
    fn start_board_matches_spec_19() {
        let sb = parse_start_board(BOARD_JSON).unwrap();
        assert_eq!(sb.cells.len(), CELLS);
        let (mut boxes, mut webs, mut free_n, mut empty) = (0, 0, 0, 0);
        let (mut cg, mut ct, mut gifts, mut gen_box) = (0, 0, 0, 0);
        for c in &sb.cells {
            match c {
                Cell::Empty => empty += 1,
                Cell::Free { .. } => free_n += 1,
                Cell::Web { .. } => webs += 1,
                Cell::Box { item } => {
                    boxes += 1;
                    match item {
                        Item::Gift { .. } => gifts += 1,
                        Item::Chain(c) => match c.chain.as_str() {
                            "clean" | "garden" => {
                                assert!((1..=2).contains(&c.level));
                                cg += 1
                            }
                            "cook" | "tools" => {
                                assert!((1..=3).contains(&c.level));
                                ct += 1
                            }
                            "gen_box" => {
                                assert_eq!(c.level, 1);
                                gen_box += 1
                            }
                            other => panic!("예상 밖 상자 {other}"),
                        },
                    }
                }
            }
        }
        assert_eq!((boxes, webs, free_n, empty), (42, 12, 4, 5));
        assert_eq!((cg, ct, gifts, gen_box), (31, 8, 2, 1));
        // 거미줄 12칸 위치 (§19)
        let web_cells: Vec<usize> =
            sb.cells.iter().enumerate().filter(|(_, c)| matches!(c, Cell::Web { .. })).map(|(i, _)| i).collect();
        let expect = [(2, 2), (3, 2), (4, 2), (1, 3), (5, 3), (1, 4), (5, 4), (1, 5), (5, 5), (2, 6), (3, 6), (4, 6)];
        assert_eq!(web_cells, expect.iter().map(|&(x, y)| at(x, y)).collect::<Vec<_>>());
        // 가운데
        assert_eq!(sb.cells[at(2, 4)], free("gen_box", 1));
        assert_eq!(sb.cells[at(4, 4)], free("gen_cart", 1));
        assert_eq!(sb.cells[at(3, 3)], free("clean", 1));
        assert_eq!(sb.cells[at(3, 5)], free("garden", 1));
        assert_eq!(sb.orders.len(), 2);
        assert_eq!(sb.orders[0].resident, "parrot");
        assert_eq!(sb.orders[0].wants, vec![ChainItem::new("clean", 2)]);
        assert_eq!(sb.orders[1].wants, vec![ChainItem::new("garden", 2)]);
    }

    #[test]
    fn new_game_has_three_orders_and_start_energy() {
        let (_, c, e) = setup();
        let g = new_game(&e, &c, 1234, &mut rng());
        assert_eq!(g.orders.len(), 3);
        assert_eq!(g.energy, 100);
        assert_eq!(g.regen_anchor, 1234);
        assert_eq!(g.inv.len(), 4);
    }

    #[test]
    fn cell_and_item_json_shape() {
        let v = serde_json::to_value(Cell::Empty).unwrap();
        assert_eq!(v, serde_json::json!({"state":"empty"}));
        let v = serde_json::to_value(free("clean", 1)).unwrap();
        assert_eq!(v, serde_json::json!({"state":"free","item":{"kind":"chain","chain":"clean","level":1}}));
        let g = Cell::Box { item: Item::Gift { items: vec![ChainItem::new("clean", 2)] } };
        let v = serde_json::to_value(g).unwrap();
        assert_eq!(v, serde_json::json!({"state":"box","item":{"kind":"gift","items":[{"chain":"clean","level":2}]}}));
    }

    #[test]
    fn move_to_empty_and_swap() {
        let (mut g, c, _) = setup();
        g.board[at(1, 1)] = free("clean", 1);
        g.board[at(2, 1)] = free("garden", 3);
        move_item(&mut g, &c, at(1, 1), at(1, 2)).unwrap();
        assert_eq!(g.board[at(1, 1)], Cell::Empty);
        assert_eq!(g.board[at(1, 2)], free("clean", 1));
        move_item(&mut g, &c, at(1, 2), at(2, 1)).unwrap();
        assert_eq!(g.board[at(1, 2)], free("garden", 3), "다른 아이템은 자리 교환");
        assert_eq!(g.board[at(2, 1)], free("clean", 1));
    }

    #[test]
    fn move_rejects_unmovable_sources_and_bad_cells() {
        let (mut g, c, _) = setup();
        g.board[at(1, 1)] = boxed("clean", 1);
        g.board[at(2, 1)] = web("clean", 1);
        for from in [at(0, 0), at(1, 1), at(2, 1)] {
            assert_eq!(move_item(&mut g, &c, from, at(3, 3)), Err(RuleError::NotMovable));
        }
        assert_eq!(move_item(&mut g, &c, 63, 0), Err(RuleError::BadCell));
        assert_eq!(move_item(&mut g, &c, 0, 99), Err(RuleError::BadCell));
        assert_eq!(move_item(&mut g, &c, 0, 104), Err(RuleError::BadCell), "보관함은 4칸");
    }

    #[test]
    fn merge_puts_next_level_in_target_and_clears_source() {
        let (mut g, c, _) = setup();
        g.board[at(1, 1)] = free("clean", 1);
        g.board[at(2, 1)] = free("clean", 1);
        move_item(&mut g, &c, at(1, 1), at(2, 1)).unwrap();
        assert_eq!(g.board[at(1, 1)], Cell::Empty);
        assert_eq!(g.board[at(2, 1)], free("clean", 2));
    }

    #[test]
    fn max_level_cannot_merge() {
        let (mut g, c, _) = setup();
        g.board[at(1, 1)] = free("clean", 8);
        g.board[at(2, 1)] = free("clean", 8);
        let before = g.clone();
        assert_eq!(move_item(&mut g, &c, at(1, 1), at(2, 1)), Err(RuleError::MaxLevel));
        assert_eq!(g, before);
        // 생산기도 Lv5 가 끝
        g.board[at(1, 1)] = free("gen_box", 5);
        g.board[at(2, 1)] = free("gen_box", 5);
        assert_eq!(move_item(&mut g, &c, at(1, 1), at(2, 1)), Err(RuleError::MaxLevel));
        // 그 아래 레벨은 합쳐진다
        g.board[at(1, 1)] = free("gen_box", 4);
        g.board[at(2, 1)] = free("gen_box", 4);
        move_item(&mut g, &c, at(1, 1), at(2, 1)).unwrap();
        assert_eq!(g.board[at(2, 1)], free("gen_box", 5));
    }

    #[test]
    fn merge_opens_only_the_four_orthogonal_boxes() {
        let (mut g, c, _) = setup();
        let centre = at(3, 4);
        for (dx, dy) in [(0i32, -1i32), (0, 1), (-1, 0), (1, 0), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
            g.board[at((3 + dx) as usize, (4 + dy) as usize)] = boxed("garden", 1);
        }
        g.board[at(0, 0)] = free("clean", 1);
        g.board[centre] = free("clean", 1);
        g.board[at(0, 1)] = free("clean", 1);
        // (0,1)->... 합성은 centre 에서: (0,0) 을 centre 로 끌어 놓지 않고 이웃 칸을 이용
        g.board[at(3, 6)] = free("clean", 1);
        move_item(&mut g, &c, at(3, 6), centre).unwrap();
        assert_eq!(g.board[centre], free("clean", 2));
        let n_web = webs_count(&g);
        assert_eq!(n_web, 4);
        for (dx, dy) in [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
            assert!(matches!(g.board[at((3 + dx) as usize, (4 + dy) as usize)], Cell::Web { .. }));
        }
        for (dx, dy) in [(-1i32, -1i32), (1, 1), (-1, 1), (1, -1)] {
            assert!(matches!(g.board[at((3 + dx) as usize, (4 + dy) as usize)], Cell::Box { .. }), "대각선은 이웃이 아니다");
        }
    }

    #[test]
    fn move_swap_and_spawn_do_not_open_boxes() {
        let (mut g, c, e) = setup();
        g.board[at(3, 3)] = free("clean", 1);
        g.board[at(4, 3)] = free("garden", 1);
        g.board[at(3, 2)] = boxed("clean", 1);
        g.board[at(5, 3)] = boxed("clean", 1);
        g.board[at(3, 4)] = free("gen_box", 1);
        move_item(&mut g, &c, at(3, 3), at(4, 3)).unwrap(); // 교환
        move_item(&mut g, &c, at(4, 3), at(4, 4)).unwrap(); // 이동
        let mut fx = Fx::default();
        g.energy = 10;
        spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx).unwrap();
        assert_eq!(webs_count(&g), 0);
        assert!(matches!(g.board[at(3, 2)], Cell::Box { .. }));
        assert!(matches!(g.board[at(5, 3)], Cell::Box { .. }));
    }

    #[test]
    fn web_accepts_only_the_same_item_and_frees_with_next_level() {
        let (mut g, c, _) = setup();
        g.board[at(3, 3)] = web("clean", 2);
        g.board[at(2, 3)] = free("clean", 1);
        g.board[at(3, 4)] = free("clean", 2);
        g.board[at(3, 2)] = boxed("garden", 1);
        // 다른 아이템 → 아무 일 없음
        let before = g.clone();
        move_item(&mut g, &c, at(2, 3), at(3, 3)).unwrap();
        assert_eq!(g, before);
        // 같은 아이템 → 합성 + 풀림 + 이웃 상자 열림 (W-2)
        move_item(&mut g, &c, at(3, 4), at(3, 3)).unwrap();
        assert_eq!(g.board[at(3, 3)], free("clean", 3));
        assert_eq!(g.board[at(3, 4)], Cell::Empty);
        assert!(matches!(g.board[at(3, 2)], Cell::Web { .. }), "web 풀기도 합성이라 바깥으로 열려 나간다");
    }

    #[test]
    fn web_holding_max_level_item_frees_on_neighbour_merge() {
        let (mut g, c, _) = setup();
        g.board[at(3, 3)] = web("clean", 8);
        g.board[at(3, 2)] = boxed("clean", 8); // 열리며 web 이 되고 아직 풀리지는 않는다
        g.board[at(2, 4)] = free("garden", 1);
        g.board[at(2, 3)] = free("garden", 1);
        move_item(&mut g, &c, at(2, 4), at(2, 3)).unwrap(); // (2,3) 합성 → 이웃 (3,3) 이 풀림
        assert_eq!(g.board[at(3, 3)], free("clean", 8));
        assert_eq!(g.board[at(3, 2)], boxed("clean", 8), "대각선이라 영향 없음");
        // 🎁 가 든 web 도 더는 합칠 수 없으므로 풀린다
        g.board[at(5, 5)] = Cell::Web { item: Item::Gift { items: vec![ChainItem::new("clean", 2)] } };
        g.board[at(4, 6)] = free("clean", 1);
        g.board[at(5, 6)] = free("clean", 1);
        move_item(&mut g, &c, at(4, 6), at(5, 6)).unwrap();
        assert!(matches!(g.board[at(5, 5)], Cell::Free { item: Item::Gift { .. } }));
    }

    #[test]
    fn box_cannot_be_dropped_on_and_foreign_web_is_noop() {
        let (mut g, c, _) = setup();
        g.board[at(1, 1)] = free("clean", 1);
        g.board[at(2, 1)] = boxed("clean", 1);
        let before = g.clone();
        move_item(&mut g, &c, at(1, 1), at(2, 1)).unwrap();
        assert_eq!(g, before);
    }

    #[test]
    fn storage_moves_and_never_merges() {
        let (mut g, c, _) = setup();
        g.board[at(1, 1)] = free("clean", 1);
        g.inv[0] = Some(Item::chain("clean", 1));
        // 보드 → 같은 아이템이 든 보관함: 합쳐지지 않고 교환
        move_item(&mut g, &c, at(1, 1), 100).unwrap();
        assert_eq!(g.inv[0], Some(Item::chain("clean", 1)));
        assert_eq!(g.board[at(1, 1)], free("clean", 1));
        // 빈 보관함 칸으로
        move_item(&mut g, &c, at(1, 1), 101).unwrap();
        assert_eq!(g.board[at(1, 1)], Cell::Empty);
        assert_eq!(g.inv[1], Some(Item::chain("clean", 1)));
        // 보관함 → 보관함도 합치지 않는다
        move_item(&mut g, &c, 100, 101).unwrap();
        assert_eq!(g.inv[0], Some(Item::chain("clean", 1)));
        assert_eq!(g.inv[1], Some(Item::chain("clean", 1)));
        // 보관함 → 보드의 같은 아이템 위: 합성
        g.board[at(2, 2)] = free("clean", 1);
        move_item(&mut g, &c, 100, at(2, 2)).unwrap();
        assert_eq!(g.board[at(2, 2)], free("clean", 2));
        assert_eq!(g.inv[0], None);
        // 생산기·🎁 도 넣을 수 있다
        g.board[at(0, 0)] = free("gen_box", 1);
        g.board[at(1, 0)] = Cell::Free { item: Item::Gift { items: vec![ChainItem::new("clean", 2)] } };
        move_item(&mut g, &c, at(0, 0), 100).unwrap();
        move_item(&mut g, &c, at(1, 0), 102).unwrap();
        assert_eq!(g.inv[0], Some(Item::chain("gen_box", 1)));
        assert!(matches!(g.inv[2], Some(Item::Gift { .. })));
    }

    fn only_empty(g: &mut Game, cells: &[(usize, usize)]) {
        g.board = vec![boxed("clean", 1); CELLS];
        for &(x, y) in cells {
            g.board[at(x, y)] = Cell::Empty;
        }
    }

    #[test]
    fn nearest_empty_breaks_ties_upper_then_left() {
        let (mut g, _, _) = setup();
        let from = at(3, 4);
        only_empty(&mut g, &[(3, 3), (2, 4), (4, 4), (3, 5)]);
        assert_eq!(nearest_empty(&g.board, from), Some(at(3, 3)), "위");
        only_empty(&mut g, &[(2, 4), (4, 4), (3, 5)]);
        assert_eq!(nearest_empty(&g.board, from), Some(at(2, 4)), "왼쪽");
        only_empty(&mut g, &[(4, 4), (3, 5)]);
        assert_eq!(nearest_empty(&g.board, from), Some(at(4, 4)), "오른쪽이 아래보다 위");
        only_empty(&mut g, &[(3, 5), (6, 8)]);
        assert_eq!(nearest_empty(&g.board, from), Some(at(3, 5)));
        only_empty(&mut g, &[(0, 0), (6, 8)]);
        assert_eq!(nearest_empty(&g.board, from), Some(at(0, 0)), "거리 7 대 8");
        only_empty(&mut g, &[]);
        assert_eq!(nearest_empty(&g.board, from), None);
    }

    #[test]
    fn spawn_charges_one_and_lands_on_nearest_empty() {
        let (mut g, c, e) = setup();
        g.energy = 5;
        g.board[at(3, 4)] = free("gen_box", 1);
        let mut fx = Fx::default();
        let spot = spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx).unwrap();
        assert_eq!(spot, at(3, 3));
        assert_eq!(g.energy, 4);
        assert_eq!(fx.deltas.len(), 1);
        assert_eq!((fx.deltas[0].reason, fx.deltas[0].delta), ("spend:spawn", -1));
        match &g.board[spot] {
            Cell::Free { item: Item::Chain(i) } => {
                assert_eq!(i.chain, "clean", "Lv1 에서는 tools(해금 Lv5) 가 나오지 않는다");
                assert_eq!(i.level, 1, "생산기 Lv1 은 Lv1 만");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn full_board_errors_before_charging() {
        let (mut g, c, e) = setup();
        g.energy = 5;
        g.board = vec![free("garden", 1); CELLS];
        g.board[at(3, 4)] = free("gen_box", 1);
        let mut fx = Fx::default();
        let before = g.clone();
        assert_eq!(spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx), Err(RuleError::BoardFull));
        assert_eq!(g, before);
        assert!(fx.deltas.is_empty());
    }

    #[test]
    fn no_energy_blocks_spawn_without_side_effects() {
        let (mut g, c, e) = setup();
        g.energy = 0;
        g.board[at(3, 4)] = free("gen_box", 1);
        let before = g.clone();
        let mut fx = Fx::default();
        assert_eq!(spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx), Err(RuleError::NoEnergy));
        assert_eq!(g, before);
    }

    #[test]
    fn generator_skips_hidden_and_locked_chains() {
        let (mut g, mut c, e) = setup();
        g.energy = 1000;
        g.board[at(3, 4)] = free("gen_box", 5);
        let mut fx = Fx::default();
        let mut r = rng();
        // Lv1: tools 는 잠겨 있다 → 항상 clean
        for _ in 0..40 {
            let s = spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut r, &mut fx).unwrap();
            assert!(matches!(&g.board[s], Cell::Free { item: Item::Chain(i) } if i.chain == "clean"));
            g.board[s] = Cell::Empty;
        }
        // Lv5 면 둘 다 나온다
        g.level = 5;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..80 {
            let s = spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut r, &mut fx).unwrap();
            if let Cell::Free { item: Item::Chain(i) } = &g.board[s] {
                seen.insert(i.chain.clone());
                assert!(i.level <= 3);
            }
            g.board[s] = Cell::Empty;
        }
        assert_eq!(seen.len(), 2);
        // 숨기면 빠진다
        c.chains.get_mut("clean").unwrap().hidden = true;
        for _ in 0..30 {
            let s = spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut r, &mut fx).unwrap();
            assert!(matches!(&g.board[s], Cell::Free { item: Item::Chain(i) } if i.chain == "tools"));
            g.board[s] = Cell::Empty;
        }
    }

    #[test]
    fn generator_without_usable_emits_errors_without_charging() {
        let (mut g, mut c, e) = setup();
        g.energy = 10;
        g.board[at(3, 4)] = free("gen_box", 1);
        let mut fx = Fx::default();
        c.chains.get_mut("gen_box").unwrap().emits = Some(vec![]);
        assert_eq!(spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx), Err(RuleError::NoEmits));
        c.chains.get_mut("gen_box").unwrap().emits = Some(vec!["clean".into()]);
        c.chains.get_mut("clean").unwrap().hidden = true;
        assert_eq!(spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx), Err(RuleError::NoEmits));
        assert_eq!(g.energy, 10);
        assert!(fx.deltas.is_empty());
        assert_eq!(RuleError::NoEmits.to_string(), "도감에서 계열을 연결하세요");
    }

    #[test]
    fn drop_table_levels_follow_generator_level() {
        let (mut g, c, e) = setup();
        g.energy = 100_000;
        let mut fx = Fx::default();
        let mut r = rng();
        let mut counts = [[0u32; 3]; 5];
        for gl in 1..=5u8 {
            g.board[at(3, 4)] = free("gen_box", gl);
            for _ in 0..600 {
                let s = spawn_from_generator(&mut g, &c, &e, at(3, 4), &mut r, &mut fx).unwrap();
                if let Cell::Free { item: Item::Chain(i) } = &g.board[s] {
                    counts[gl as usize - 1][i.level as usize - 1] += 1;
                }
                g.board[s] = Cell::Empty;
            }
        }
        assert_eq!(counts[0], [600, 0, 0]);
        assert_eq!(counts[1][2], 0);
        assert!(counts[1][1] > 40 && counts[1][1] < 160, "15% 근처: {:?}", counts[1]);
        assert!(counts[4][0] > 180 && counts[4][1] > 200 && counts[4][2] > 20, "{:?}", counts[4]);
    }

    #[test]
    fn tap_dispatches_and_box_tap_is_an_error() {
        let (mut g, c, e) = setup();
        g.energy = 3;
        g.board[at(3, 4)] = free("gen_box", 1);
        g.board[at(0, 0)] = boxed("clean", 1);
        g.board[at(1, 0)] = free("clean", 1);
        let mut fx = Fx::default();
        assert_eq!(tap(&mut g, &c, &e, at(0, 0), &mut rng(), &mut fx), Err(RuleError::BoxClosed));
        tap(&mut g, &c, &e, at(1, 0), &mut rng(), &mut fx).unwrap(); // 일반 아이템은 아무 일 없음
        assert_eq!(g.energy, 3);
        tap(&mut g, &c, &e, at(3, 4), &mut rng(), &mut fx).unwrap();
        assert_eq!(g.energy, 2);
        assert_eq!(tap(&mut g, &c, &e, 63, &mut rng(), &mut fx), Err(RuleError::BadCell));
    }

    #[test]
    fn gift_pops_one_item_at_a_time_and_vanishes() {
        let (mut g, _, _) = setup();
        g.board[at(3, 4)] = Cell::Free {
            item: Item::Gift { items: vec![ChainItem::new("clean", 2), ChainItem::new("garden", 2)] },
        };
        let s1 = pop_gift(&mut g, at(3, 4)).unwrap();
        assert_eq!(s1, at(3, 3));
        assert_eq!(g.board[s1], free("clean", 2));
        assert!(matches!(&g.board[at(3, 4)], Cell::Free { item: Item::Gift { items } } if items.len() == 1));
        let s2 = pop_gift(&mut g, at(3, 4)).unwrap();
        assert_eq!(g.board[s2], free("garden", 2));
        assert_eq!(g.board[at(3, 4)], Cell::Empty, "다 나오면 사라진다");
    }

    #[test]
    fn gift_on_full_board_errors_but_last_item_fits_in_place() {
        let (mut g, _, _) = setup();
        g.board = vec![free("garden", 1); CELLS];
        g.board[at(3, 4)] = Cell::Free {
            item: Item::Gift { items: vec![ChainItem::new("clean", 2), ChainItem::new("clean", 3)] },
        };
        let before = g.clone();
        assert_eq!(pop_gift(&mut g, at(3, 4)), Err(RuleError::BoardFull));
        assert_eq!(g, before);
        g.board[at(3, 4)] = Cell::Free { item: Item::Gift { items: vec![ChainItem::new("clean", 2)] } };
        pop_gift(&mut g, at(3, 4)).unwrap();
        assert_eq!(g.board[at(3, 4)], free("clean", 2));
    }

    #[test]
    fn sell_rules() {
        let (mut g, c, e) = setup();
        g.board[at(0, 0)] = free("clean", 1);
        g.board[at(1, 0)] = free("clean", 2);
        g.board[at(2, 0)] = free("clean", 4);
        g.board[at(3, 0)] = free("clean", 6);
        g.board[at(4, 0)] = free("gen_box", 1);
        g.board[at(5, 0)] = Cell::Free { item: Item::Gift { items: vec![ChainItem::new("clean", 2)] } };
        g.board[at(6, 0)] = boxed("clean", 1);
        let mut fx = Fx::default();
        assert_eq!(sell(&mut g, &c, &e, at(0, 0), &mut fx), Ok(0), "Lv1 은 환급 0");
        assert!(fx.deltas.is_empty(), "0 이면 장부 행이 없다");
        assert_eq!(g.board[at(0, 0)], Cell::Empty);
        assert_eq!(sell(&mut g, &c, &e, at(1, 0), &mut fx), Ok(1));
        assert_eq!(sell(&mut g, &c, &e, at(2, 0), &mut fx), Ok(4));
        assert_eq!(sell(&mut g, &c, &e, at(3, 0), &mut fx), Ok(16));
        assert_eq!(g.energy, 21);
        assert!(fx.deltas.iter().all(|d| d.reason == "sell"));
        assert_eq!(sell(&mut g, &c, &e, at(4, 0), &mut fx), Err(RuleError::GeneratorNotSellable));
        assert_eq!(sell(&mut g, &c, &e, at(5, 0), &mut fx), Err(RuleError::GiftNotSellable));
        assert_eq!(sell(&mut g, &c, &e, at(6, 0), &mut fx), Err(RuleError::NotMovable));
        assert_eq!(sell(&mut g, &c, &e, at(0, 1), &mut fx), Err(RuleError::NotMovable));
        // 보관함 것도 판다
        g.inv[1] = Some(Item::chain("garden", 3));
        assert_eq!(sell(&mut g, &c, &e, 101, &mut fx), Ok(2));
        assert_eq!(g.inv[1], None);
        // 알 수 없는 계열도 환급할 수 있다
        g.board[at(0, 2)] = free("zzz", 3);
        assert_eq!(sell(&mut g, &c, &e, at(0, 2), &mut fx), Ok(2));
    }

    #[test]
    fn place_or_queue_order_is_board_then_storage_then_pending() {
        let (mut g, _, _) = setup();
        let gift = || Item::Gift { items: vec![ChainItem::new("clean", 2)] };
        assert_eq!(place_or_queue(&mut g, gift(), None), Placement::Board(CENTER));
        assert_eq!(place_or_queue(&mut g, gift(), Some(at(0, 0))), Placement::Board(at(0, 0)));
        g.board = vec![free("clean", 1); CELLS];
        assert_eq!(place_or_queue(&mut g, gift(), None), Placement::Storage(0));
        g.inv = vec![Some(Item::chain("clean", 1)); 4];
        assert_eq!(place_or_queue(&mut g, gift(), None), Placement::Queued);
        assert_eq!(g.pending_gifts.len(), 1);
        // 받기: 자리가 없으면 줄이 그대로
        assert_eq!(claim_gift(&mut g), Err(RuleError::BoardFull));
        assert_eq!(g.pending_gifts.len(), 1);
        g.board[at(0, 0)] = Cell::Empty;
        claim_gift(&mut g).unwrap();
        assert!(g.pending_gifts.is_empty());
        assert!(matches!(g.board[at(0, 0)], Cell::Free { item: Item::Gift { .. } }));
    }

    #[test]
    fn open_box_costs_five_and_leaves_neighbours_closed() {
        let (mut g, _, e) = setup();
        g.energy = 7;
        g.board[at(1, 1)] = boxed("clean", 1);
        g.board[at(2, 1)] = boxed("garden", 1);
        let mut fx = Fx::default();
        open_box(&mut g, &e, at(1, 1), &mut fx).unwrap();
        assert!(matches!(g.board[at(1, 1)], Cell::Web { .. }));
        assert!(matches!(g.board[at(2, 1)], Cell::Box { .. }));
        assert_eq!(g.energy, 2);
        assert_eq!(fx.deltas[0].reason, "spend:box");
        assert_eq!(open_box(&mut g, &e, at(2, 1), &mut fx), Err(RuleError::NoEnergy));
        assert_eq!(open_box(&mut g, &e, at(0, 0), &mut fx), Err(RuleError::NotMovable), "상자가 아니다");
    }

    #[test]
    fn inventory_expansion_costs_and_limit() {
        let (mut g, _, e) = setup();
        g.energy = 10_000;
        let mut fx = Fx::default();
        assert_eq!(inv_next_cost(&g, &e), Some(20));
        inv_expand(&mut g, &e, &mut fx).unwrap();
        assert_eq!((g.inv_slots, g.inv.len(), g.energy), (5, 5, 9980));
        assert_eq!(inv_next_cost(&g, &e), Some(40));
        while g.inv_slots < 12 {
            inv_expand(&mut g, &e, &mut fx).unwrap();
        }
        assert_eq!(inv_next_cost(&g, &e), None);
        assert_eq!(inv_expand(&mut g, &e, &mut fx), Err(RuleError::InvMax));
        assert!(fx.deltas.iter().all(|d| d.reason == "spend:inv"));
        let mut poor = Game { energy: 5, ..Game::default() };
        assert_eq!(inv_expand(&mut poor, &e, &mut Fx::default()), Err(RuleError::NoEnergy));
        assert_eq!(poor.inv_slots, 4);
    }

    #[test]
    fn game_json_roundtrip_and_defaults() {
        let (_, c, e) = setup();
        let g = new_game(&e, &c, 99, &mut rng());
        let back: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back, g);
        let sparse: Game = serde_json::from_str(r#"{"stars":5}"#).unwrap();
        assert_eq!((sparse.stars, sparse.level, sparse.board.len(), sparse.v), (5, 1, CELLS, 1));
        let mut broken: Game = serde_json::from_str(r#"{"board":[],"inv":[],"inv_slots":4}"#).unwrap();
        broken.normalize();
        assert_eq!((broken.board.len(), broken.inv.len()), (CELLS, 4));
    }
}
