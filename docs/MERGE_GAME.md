# Merge 게임 위젯 — 설계

> 상태: **설계만** (구현 전). 위젯 id 는 `merge`, 백엔드는 `providers/merge`.
>
> 한 줄 요약: **할 일을 해내면 에너지가 생기고, 그 에너지로 보드에서 아이템을 꺼내 합친다.**
> 에너지는 기다리거나(시간 회복) 클리커를 눌러서도 얻는다. 재화는 에너지 하나뿐이다.
> 친구와는 **할 일과 점수만** 주고받는다. 우리가 운영하는 서버는 두지 않는다.

장르 문법(상자로 덮인 보드, 거미줄, 생산기, 동물 주민의 주문)은 Merge Mansion·동물맨션류에서
빌려 오지만, **이름·그림·캐릭터는 가져오지 않는다**. 기본 그림은 이모지이고(그림 자원도 저작권 문제도
생기지 않는다), 아이템·생산기·주민의 이름과 그림은 사용자가 이모지나 이미지로 바꾼다.

> **게임 규칙·콘텐츠·숫자의 기준은 [MERGE_GAME_SPEC.md](MERGE_GAME_SPEC.md)** 다 (규칙 번호 `B-1`, `E-4` …).
> 이 문서는 구조·저장·네트워크를 다루고, 규칙은 번호로만 가리킨다. 둘이 어긋나면 명세가 맞다.

## 0. 핵심 순환

```
 ⚡ 얻는 곳 (재화는 ⚡ 하나)                     ⚡ 쓰는 곳
 ┌───────────────────────────┐             ┌─────────────────────────────────────┐
 │ ☑ 할 일 완료     +10~90⚡  │             │ 🧰 생산기 클릭 1⚡ → 🫧 Lv1           │
 │ ⏱ 시간 회복  2분에 +1⚡    │ ──── ⚡ ──▶ │ 🫧+🫧 → 🧼 → 🧽 … 끌어다 겹치면 합성 │
 │   (100 까지만)             │             │ 🦜 주민 주문 → 전달 → ⭐ (점수)       │
 │ 🐹 클리커  20번에 +1⚡     │             │ 상자 열기 · 보관함 · 주문 교체        │
 └───────────────────────────┘             └─────────────────────────────────────┘
                         ⭐·할 일 달성 현황 ──▶ 친구들에게 공유 (iroh)
```

할 일이 에너지를 가장 많이 주지만 유일한 길은 아니다 — 기다리거나(회복) 그냥 누르면(클리커) 된다.
그래서 할 일 쪽에는 남용 방지 장치를 두지 않는다 (명세 T-9).

## 1. 단계

| 단계 | 내용 | 네트워크 |
|---|---|---|
| **P0** | 보드·생산기·합성·판매·주문·별·레벨 + **에너지 장부·시간 회복·클리커** + 콘텐츠를 DB 에 (기본값만, 편집 화면 없이) | 없음 |
| **P1** | 일간·주간 할 일 → 에너지, 연속 보너스 | 없음 |
| P1.5 | **도감(꾸미기)** 편집 화면 — 이름·이모지·이미지, 새 계열·주민 | 없음 |
| **P2** | 친구: 신원 키, **친구 코드**로 추가, **상태 코드**를 메신저에 붙여넣어 공유 | **없음** — 이 앱만으로 끝 |
| **P3** | 자동 동기화 — **iroh** | LAN 은 외부 의존 없음, 인터넷은 n0 의 공개 relay |

P0 에 에너지 출처 둘(회복·클리커)이 들어가므로 할 일 없이도 게임이 돈다. 콘텐츠를 처음부터 DB 에 두는
이유: 나중에 꾸미기를 붙일 때 저장 구조를 바꾸지 않으려고 (P1.5 는 화면만 더한다).

---

## 2. 보드 (P0)

### 2.1 화면

```
┌───────────────────────────────────────────────┐
│ 🍀3  ⚡84 (+1 01:23)  ⭐37   [보드|할 일 2|친구|도감] │  머리줄 — "할 일 2" = 오늘 남은 할 일
├───────────────────────────────────────────────┤
│ 🦜 🧽 🧼 ✔ [전달]   🐰 🌿   🐱 🍳          │ 🐹  │  주문 줄          클리커 (넓으면 옆,
├────────────────────────────────────────────┤ ◔  │                   좁으면 아래 — 명세 K-5)
│ 📦 📦 📦 📦 📦 📦 📦                       │ 7/20│
│ 📦 📦 🕸🪴 🕸🔧 📦 📦 📦                     │     │
│ 📦 🕸🥚 ·  🧰  ·  🕸🪣 📦                    │     │  7 × 9 보드
│ 🕸🌱 🫧  ·   ·  [🧼] 🕸🔩 📦                 │     │
│ …                                          │     │
├────────────────────────────────────────────┴─────┤
│ 🧼 비누 (Lv2) — 합쳐서 🧽 스펀지(Lv3)      [팔기 +1⚡] │  선택한 아이템
│ [🧰][🌿][  ][  ] [+20⚡]                          │  보관함
└───────────────────────────────────────────────┘
```

기본 크기 400×560 (클리커 세로 줄 포함), 최소 240×400. 넘치면 `WidgetFrame` 의 자동 축소가 맞춰 준다.
짧은 변이 280px 미만이면 바닥줄(아이템 정보)을 감추고 길게 누르기 툴팁으로 대신한다.

### 2.2 규칙이 사는 곳

규칙은 명세에 있다. 이 문서는 그 규칙을 **어느 모듈에 두느냐**만 정한다:

| 규칙 | 코드 |
|---|---|
| 칸 `B-*` · 조작 `M-*` · 생산기 `G-*` · 상자·거미줄 `W-*` · 🎁 `S-*` · 보관함 `V-*` | `board.rs` — 순수 함수, rng 는 인자 |
| 주문·주민 `O-*` | `orders.rs` |
| 별·레벨·해금 `L-*` | `progress.rs` |
| 에너지 `E-*` · 시간 회복 `E-3` · 클리커 `K-*` | `energy.rs` (장부) |
| 할 일 `T-*` | `todos.rs` |
| 꾸미기 `Z-*` | `content.rs` · `assets.rs` |

### 2.3 숫자와 콘텐츠를 나눈다

| | 어디 | 바꾸는 사람 |
|---|---|---|
| **숫자** (명세 §16) | `resources/merge-economy.json` — 바이너리에 포함, 앱 데이터 폴더의 같은 이름 파일이 키 단위로 덮어씀 (`pricing.json` 과 같은 방식) | 개발자 / 고급 사용자 |
| **콘텐츠** (계열·생산기·주민·외형) | `merge.sqlite` 의 `chains`·`chain_levels`·`residents`·`skins`. 첫 실행 때 `resources/merge-content.json` 으로 채운다 | **사용자** (도감) |
| **시작 보드** (명세 §19) | `resources/merge-board.json` — 계열 **id** 로 적는다 | 개발자 |

- 보드·주문·보관함은 아이템을 `(chain_id, level)` 로만 기억한다 → 이름·그림을 아무리 바꿔도 저장된 게임이 깨지지 않는다 (명세 I-5).
- 기본 콘텐츠의 행에는 `builtin = 1` 을 두어 [기본값으로] 가 `merge-content.json` 에서 다시 읽게 한다 (Z-3).
  앱 업데이트로 기본 콘텐츠가 늘면 **없는 id 만** 더한다 — 사용자가 고친 것은 덮지 않는다.
- 계열·주민은 지우지 않고 `hidden` (Z-4). 레벨 수 줄이기는 넘치는 아이템이 없을 때만 (Z-5) — 검사는 `content.rs` 가
  `game` JSON 을 읽어 한다.
- 무결성 테스트: 기본 콘텐츠의 모든 계열에 레벨이 있고, 생산기의 `emits` 가 있는 계열을 가리키고, 드롭 표 합이 100%,
  시작 보드의 id 가 모두 존재.

### 2.4 꾸민 그림 (`assets.rs`)

- 도감에서 이미지를 고르면 프론트가 `@tauri-apps/plugin-dialog` 의 `open()` 으로 경로를 받아(설정 패널의 `type: "path"` 와 같은 방식)
  `merge_asset_import(path)` 를 부른다.
- Rust 가 `image` 크레이트로 읽어 128×128 로 맞춘다(비율 유지, 남는 곳 투명) → PNG 로 인코딩 →
  SHA-256 을 이름으로 `%APPDATA%/com.user.deskboard/merge-assets/<hash>.png` 에 둔다. 같은 그림은 한 번만 저장된다.
- 프론트에는 `data:image/png;base64,…` 로 보낸다 — 폴더 위젯 아이콘(`folders/icon.rs`)·배경화면과 같은 방식이라
  asset 프로토콜 설정을 새로 열 필요가 없다. 위젯은 해시 → data URI 를 메모리에 캐시한다.
- 그림 값의 모양: `{"emoji":"🧼"}` · `{"image":"<hash>"}` · `{"text":"A"}`. 어떤 행도 가리키지 않는 이미지 파일은 시작 시 지운다.

---

## 3. 할 일과 에너지 (P0~P1)

### 3.1 할 일

규칙은 명세 §11 (`T-*`). **기간 키**: 일간 `2026-10-07`(지역 날짜, `dayStartHour` 만큼 당김),
주간 `2026-W41`(ISO 주, 월요일 시작). 기간이 바뀌어도 지울 것이 없다 — 완료 기록이 기간 키에 묶여 있으니
새 기간에는 저절로 체크가 비어 보인다. 크기·목표를 고치면 바로 바뀌고, 이미 받은 것은 `granted` 에 남아 있다 (T-5).

### 3.2 에너지 장부

규칙은 명세 §12·§13 (`E-*`, `K-*`). 에너지는 숫자 하나가 아니라 **장부(ledger)의 합**이다.
재화가 ⚡ 하나뿐이고 출처·쓰임이 계속 늘어날 것이므로(명세 §17), "지금 ⚡ 가 왜 이만큼인가" 를
언제든 장부로 설명할 수 있게 한다.

```
ledger(id, at, delta, reason, ref, day_key)
  얻기: start | todo | todo_undo | bonus_allclear | bonus_streak | regen | clicker | sell | order
  쓰기: spend:spawn | spend:box | spend:inv | spend:reroll
```

- **체크** → `completions(todo_id, period_key)` 에 한 줄 (UNIQUE) + `ledger(+n, 'todo', completion_id)`.
  같은 트랜잭션이라 두 번 눌러도 한 번만 지급된다 (E-2). 상한이 없으므로 `granted` 는 지급표 값 그대로다.
- **해제** → `ledger(-granted, 'todo_undo')`, 보너스가 깨지면 그것도 되돌린다 (E-5). 음수 허용 (C-3).
- **잦은 것은 하루 단위로 묶는다**: `spend:spawn`·`regen`·`clicker` 는 `(reason, day_key)` 한 줄을 갱신한다
  (UPSERT). 하루 수백 번이라 한 줄씩 쓰면 장부가 불어난다. E-7 화면도 묶어서 보여 준다.
- 현재 에너지 = `SUM(delta)` — `game` JSON 에 캐시하고, 시작 시 장부로 다시 계산해 맞춘다.

### 3.3 시간 회복 — 타이머 없이 (명세 E-3)

백그라운드 타이머를 두지 않는다. `game` 에 `regen_anchor`(마지막으로 회복을 정산한 시각)만 두고,
**⚡ 를 읽거나 쓰는 모든 커맨드의 첫머리에서** 정산한다:

```
n = floor((now - anchor) / interval)
if energy >= cap:      anchor = now                       // 차 있는 동안은 시간이 쌓이지 않는다
else: add = min(n, cap - energy); energy += add; anchor += n * interval (꽉 찼으면 anchor = now)
if now < anchor:       anchor = now                       // 시계를 되돌린 경우
```

앱이 꺼져 있던 시간도 이 계산으로 저절로 들어온다. 머리줄의 "+1 까지 01:23" 은 프론트가 `anchor`·`interval` 로
계산해 **위젯이 보이고 ⚡ < cap 일 때만** 1초 타이머로 그린다 (그 밖에는 타이머 없음 — 상시 CPU 0).

### 3.4 클리커 (명세 §13)

- 클릭마다 `invoke` 하지 않는다. 프론트가 클릭 수를 모아 **고리가 찰 때마다**, 그리고 마지막 클릭 1초 뒤에
  `merge_clicker_add(n)` 를 한 번 보낸다 (`invokeInOrder`). Rust 가 `clicker_rem += n` → `clicks_per_energy` 마다 +1⚡,
  나머지는 `game` 에 남긴다 (K-4).
- 고리의 진행은 프론트가 즉시 그린다 (낙관적). 응답의 `clicker_rem` 으로 다시 맞춘다.
- 마우스 클릭(`pointerdown`, 주 버튼)만 센다. 클리커 버튼은 `<button>` 이 아니라 `role="button"` 의 div 로 두어
  스페이스·엔터 자동 반복이 들어오지 않게 한다 (K-2).
- 클리커를 누를 때도 포커스 문제(§4.2)는 같다 — 클릭은 짧아서 끊길 일은 없지만, 보드와 같은 영역(`data-capture-keys`) 안에 둔다.

### 3.5 할 일 탭

```
오늘 (10/7)                          +30⚡ 받음
 ⠿ ☑ 운동 30분          보통 +20⚡
 ⠿ ☑ 영어 단어 30개      작음 +10⚡    🔒
 ⠿ ☐ 방 청소             보통 +20⚡
 [ + 할 일 추가 …           ] [작음▾] [오늘▾]
이번 주 (W41)                         +90⚡ 받음
 ⠿ ☑ 책 한 권 끝내기      큼 +90⚡
 ⠿ ☐ 헬스장 1/3           보통 +60⚡
[에너지 기록 ▸]
```

체크하는 순간 에너지 칩이 머리줄 ⚡ 쪽으로 날아가는 짧은 애니메이션(`--dur`). 순서는 notes 위젯처럼 손잡이(⠿)로
바꾼다 (`notes/reorder.ts` 의 계산을 참고하되 위젯끼리 import 는 금지 — 같은 계산이 두 위젯에 필요해지면 `core/` 로 옮긴다).

**기존 메모·할 일 위젯과 합치지 않는다.** 그쪽은 반복(일간·주간)이 없는 목록이고, 에너지와 공유가 얽히면
메모 위젯의 단순함이 무너진다. 나중에 "메모 위젯의 목록 하나를 에너지 할 일로 연결" 은 가능하다.

### 3.6 도감 탭 (P1.5)

```
계열              생산기           주민
 🫧 청소 (8)  ✎    🧰 도구함  ✎     🦜 앵무  ✎
 🌱 정원 (8)  ✎    🛒 장바구니 ✎    🐰 토끼  ✎
 🥚 요리 🔒Lv3 ✎                    + 새 주민
 + 새 계열
기타: 🐹 클리커 ✎  📦 상자 ✎  🕸 거미줄 ✎  🎁 선물 ✎        [전부 기본값으로]
```

✎ 를 누르면 같은 위젯 안에서 편집 시트가 열린다 — 레벨마다 [그림][이름] 한 줄, 그림 칸을 누르면
"이모지 입력 / 이미지 불러오기 / 글자" 를 고른다. 이모지 입력은 `Intl.Segmenter` 로 **글자 하나**만 받는다.
입력란을 쓰므로 포커스 반환(§4.2)과 부딪히지 않는다.

---

## 4. 백엔드 구조

**게임 상태의 주인은 Rust 다.** 에너지와 보드가 서로 다른 곳에 있으면 "에너지는 깎였는데 아이템은 안 나옴"
같은 어긋남이 생긴다. 모든 조작을 커맨드 하나 = SQLite 트랜잭션 하나로 처리하고, 프론트는 결과를 받아 그린다.
로컬 `invoke` 왕복은 1~2ms 라 끌어 놓기 반응에 지장이 없다.

```
src-tauri/src/providers/merge/
├─ mod.rs        Provider · 커맨드 · 이벤트
├─ economy.rs    숫자 로드 (포함본 + 덮어쓰기)
├─ content.rs    계열·생산기·주민·외형 — 기본값 채우기, 편집, 숨기기, 레벨 수 검사
├─ assets.rs     꾸민 이미지 가져오기·줄이기·저장·data URI·청소
├─ board.rs      순수 규칙: 이동·합성·상자·거미줄·드롭 위치·🎁·보관함 (rng 는 인자)
├─ orders.rs     주문 생성·충족 판정·보상·교체·친밀도
├─ progress.rs   별 → 레벨, 해금, 레벨업 보상·받을 선물 줄
├─ energy.rs     장부, 시간 회복 정산, 클리커 나머지
├─ todos.rs      기간 키, 완료·해제, 보너스·연속
├─ store.rs      MergeStore 트레이트 + SqliteStore (`merge.sqlite`, in_memory 로 테스트)
├─ identity.rs   (P2) 키 생성·DPAPI 저장
├─ share.rs      (P2) 상태 스냅샷 만들기·서명·검증, 친구/상태 코드
└─ sync/         (P3) FriendTransport 트레이트 + iroh 구현 + 테스트용 Fake
```

시각(`now`)은 모든 규칙 함수에 **인자로** 넘긴다 — 회복·기간 키 테스트에서 시계를 마음대로 돌리려고.

`merge.sqlite`:

```sql
CREATE TABLE game (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER, json TEXT);
  -- 보드 63칸 + 보관함 + 받을 선물 + 주문·주민 친밀도 + 별·레벨 + 에너지 캐시
  -- + regen_anchor + clicker_rem + 튜토리얼 단계. 작아서 통째로 JSON 한 줄, 조작마다 다시 쓴다.
CREATE TABLE ledger (id INTEGER PRIMARY KEY, at INTEGER, delta INTEGER, reason TEXT, ref TEXT, day_key TEXT);
CREATE UNIQUE INDEX ledger_daily ON ledger(reason, day_key) WHERE reason IN ('spend:spawn','regen','clicker');
-- 콘텐츠 (명세 §3·§9·§14)
CREATE TABLE chains (id TEXT PRIMARY KEY, kind TEXT /* item | generator */, name TEXT, max_level INTEGER,
                     unlock_level INTEGER, emits TEXT /* 생산기: JSON 계열 id 목록 */, hidden INTEGER DEFAULT 0,
                     builtin INTEGER, ord INTEGER);
CREATE TABLE chain_levels (chain_id TEXT, level INTEGER, name TEXT, icon TEXT /* JSON */, PRIMARY KEY(chain_id, level));
CREATE TABLE residents (id TEXT PRIMARY KEY, name TEXT, icon TEXT, likes TEXT /* JSON, 빈 배열 = 전부 */,
                        join_level INTEGER, hidden INTEGER DEFAULT 0, builtin INTEGER, ord INTEGER);
CREATE TABLE skins (key TEXT PRIMARY KEY /* clicker | box | web | gift */, icon TEXT);
-- 할 일 (명세 §11)
CREATE TABLE todos (id TEXT PRIMARY KEY, text TEXT, period TEXT, size INTEGER, target INTEGER DEFAULT 1,
                    shared INTEGER, ord INTEGER, created_at INTEGER, archived INTEGER DEFAULT 0);
CREATE TABLE completions (id INTEGER PRIMARY KEY, todo_id TEXT, period_key TEXT,
                          progress INTEGER, done_at INTEGER /* 목표를 채운 시각, 아직이면 NULL */,
                          granted INTEGER, UNIQUE(todo_id, period_key));
CREATE TABLE stars (id INTEGER PRIMARY KEY, at INTEGER, delta INTEGER, week_key TEXT); -- 이번 주 ⭐ 를 세려고
-- P2
CREATE TABLE friends (pubkey TEXT PRIMARY KEY, nickname TEXT, added_at INTEGER);
CREATE TABLE snapshots (author TEXT PRIMARY KEY, seq INTEGER, json TEXT, sig BLOB, received_at INTEGER);
```

커맨드 (오류는 `Result<_, String>` 사용자 메시지):

| 커맨드 | |
|---|---|
| `merge_state()` | 보드·보관함·주문·⚡(회복 정산 후)·`regen_anchor`·클리커 나머지·⭐·레벨 |
| `merge_tap(cell)` | 생산기 → 드롭, 🎁 → 하나 꺼내기 |
| `merge_move(from, to)` | 이동·교환·합성·거미줄 풀기 (+ 이웃 상자 열기). 칸 번호 0~62 는 보드, 100~ 은 보관함 |
| `merge_sell(cell)` · `merge_deliver(slot)` · `merge_reroll(slot)` · `merge_open_box(cell)` | ⚡ 로 값을 치르거나 돌려받는다 |
| `merge_inv_expand()` · `merge_claim_gift()` · `merge_tutorial(step)` | |
| `merge_clicker_add(n)` → `{energy, clicker_rem}` | `invokeInOrder` 로 |
| `merge_content()` | 렌더링용 계열·생산기·주민·스킨 (그림은 data URI 로 풀어서) |
| (P1.5) `merge_content_upsert(kind, row)` · `merge_content_hide(kind, id, hidden)` · `merge_content_reset(kind?, id?)` | |
| (P1.5) `merge_asset_import(path)` → `{hash, dataUri}` | |
| `merge_todos()` · `merge_todo_upsert(todo)` · `merge_todo_archive(id)` · `merge_todo_toggle(id, done)` · `merge_todo_reorder(ids)` | |
| `merge_ledger(days)` | 에너지 기록 (E-7) |
| (P2) `merge_my_friend_code()` · `merge_friend_add(code)` · `merge_friend_remove(pubkey)` · `merge_friends()` | |
| (P2) `merge_status_code()` · `merge_status_open(code)` | |
| (P3) `merge_set_sync(mode)` — `invokeInOrder` 로 | |

이벤트: `merge://changed`(게임) · `merge://content` · `merge://todos` · `merge://friends`.
위젯이 여러 개 떠 있어도 같은 보드를 보여 주므로 이벤트로 다시 읽는다 — notes 와 같은 꼴.

### 4.1 프론트

```
src/widgets/merge/
├─ index.ts      defaultSize 400×560, minSize 240×400
├─ Merge.tsx     머리줄 + 탭(보드 | 할 일 | 친구 | 도감)
├─ Board.tsx     격자·끌어 놓기·선택 정보·보관함
├─ Clicker.tsx   버튼·진행 고리·클릭 모으기
├─ Orders.tsx · Todos.tsx · Friends.tsx · Codex.tsx (도감)
├─ Icon.tsx      {emoji|image|text} 그리기 + data URI 캐시
├─ drag.ts       포인터 좌표 → 칸 (+ zoom 보정) — 순수 함수, 테스트
├─ regen.ts      "다음 +1 까지" 계산 — 순수 함수, 테스트
└─ Merge.css
```

설정 스키마: `nickname`(친구에게 보일 이름) · `sharedDefault`(새 할 일 기본 공개) · `dayStartHour`(0~6) ·
`hints` · (P3) `sync`(끔 / 같은 네트워크만 / 인터넷).

### 4.2 이 앱에서 조심할 것

- **끌기 도중 포커스를 빼앗기는 문제.** `window.rs::give_focus_back_if_idle` 은 위젯을 누르고 250ms 뒤
  포커스를 원래 앱에 돌려준다(입력란에 포커스가 있을 때만 예외). 아이템을 끄는 데는 250ms 보다
  오래 걸리므로, 그 순간 포인터 캡처가 끊길 수 있다 — **실측 필요**. 대응: 보드 요소를 `tabIndex={0}` +
  `data-capture-keys` 로 표시하고 `App.tsx` 의 `editable()` 에 `[data-capture-keys]` 를 더한다.
  보드를 누르면 "입력 중" 으로 취급돼 포커스를 그대로 둔다. 바탕화면이나 다른 앱을 누르면 저절로 풀린다.
  이 동안 대시보드가 잠깐 다른 앱 위로 올라올 수 있는데, 입력란에서 이미 허용하는 동작이다.
  위젯 공통 장치를 하나 늘리는 일이므로 구현 PR 에서 CLAUDE.md 의 해당 절도 고친다.
- **zoom 보정.** `WidgetFrame` 은 내용을 CSS `zoom` 으로 줄인다. 포인터의 `clientX` 와 보드의
  `getBoundingClientRect()` 를 같은 좌표계로 맞춰 칸을 계산한다 (`drag.ts`, 배율을 바꿔 가며 테스트).
- 편집 모드(`editing`)에서는 보드 입력을 받지 않는다 — 카드를 옮겨야 한다.
- 색은 `theme.css` 변수만: 칸 바탕 `--fill`, 상자 칸 `--fill-active`, 선택 테두리 `--accent`,
  주문 충족 ✔ `--ok`, 에너지 부족 `--warn`. 거미줄은 반투명 🕸 를 아이템 위에 겹친다.
- 애니메이션은 `transform` + `--dur-fast` 뿐, `requestAnimationFrame` 루프 없음 → **유휴 CPU 0**.
- 끄는 중의 유령 이미지는 카드 안에서만 움직인다 (밖으로 나가면 히트 영역을 벗어난다).

---

## 5. 친구와 공유 (P2)

### 5.1 무엇을 공유하나 — 상태 스냅샷

```jsonc
{
  "v": 1,
  "author": "<ed25519 공개키>",
  "seq": 128,                 // 만들 때마다 1씩 증가. 받는 쪽은 더 큰 것만 받는다
  "at": 1791360000,
  "name": "민수",
  "level": 7, "stars": 412, "starsWeek": 58, "week": "2026-W41", "streak": 4,
  "today": { "key": "2026-10-07", "todos": [
      { "text": "운동 30분", "done": true,  "size": 2 },
      { "text": null,        "done": true,  "size": 1 },   // 비공개
      { "text": "방 청소",   "done": false, "size": 2 } ] },
  "week":  { "key": "2026-W41", "todos": [ { "text": "책 한 권", "done": false, "size": 3 } ] },
  "audience": ["<친구 공개키>", "…"]   // P3 에서 중계 범위를 정할 때 쓴다
}
```

보드 자체는 보내지 않는다 (요구사항이 "할 일과 점수" 뿐이고, 보드를 보내면 크기만 커진다).

### 5.2 신원

- 설치마다 **ed25519 키 한 쌍**. 비밀키는 `providers/secrets`(DPAPI)로 `merge-id.dat` 에 둔다 —
  새 비밀값은 평문으로 두지 않는다는 규칙대로.
- **공개키가 곧 친구의 id** 다. 이름은 겹칠 수 있으니 목록에 공개키 앞 4글자를 작게 붙인다.
- 서명은 Rust 에서만 한다 (비밀키가 프론트로 나가지 않는다). 크레이트: `ed25519-dalek`.
- 서명이 보증하는 것은 **"이 상태를 이 사람이 보냈다"** 까지다. 할 일을 진짜 했는지는 보증하지 못한다 (§3.2).
- 윈도우를 다시 깔면 새 사람이 된다. 키 내보내기/가져오기는 남은 일.

### 5.3 코드 두 가지 — 서버 없이

| 코드 | 모양 | 언제 |
|---|---|---|
| **친구 코드** | `DBF1.<공개키 base32>.<이름>` (약 70자) | 처음 한 번. 서로 주고받아 친구로 추가 |
| **상태 코드** | `DBS1.<base64url(스냅샷 압축)>.<서명>` (할 일 10개 기준 약 500자) | 공유하고 싶을 때마다. P3 이 켜져 있으면 쓸 일이 없다 |

- 받는 쪽은 **입력란에 붙여넣는다** — 클립보드 읽기 권한이 필요 없고 입력란이라 포커스 문제도 없다.
  보내는 쪽은 `navigator.clipboard.writeText` 로 복사.
- 상태 코드를 열면: 형식 → 서명 → 보낸 사람이 친구 목록에 있는지 → `seq` 가 가진 것보다 큰지 → 저장.
  친구가 아닌 사람의 상태 코드면 "친구로 추가할까요?" 를 먼저 묻는다.
- 메신저에서 링크로 바로 열고 싶으면 `deskboard://` 딥링크를 붙일 수 있다
  (`tauri-plugin-deep-link` + single-instance 의 `deep-link` 기능. 단일 인스턴스가 release 빌드에서만
  걸려 있어 `tauri dev` 로는 확인할 수 없다). 선택 사항.

### 5.4 친구 탭

```
 민수 · a3f9     🍀7  ⭐412 (이번 주 +58)   🔥4일     2시간 전
   오늘 2/3  ☑ 운동 30분  ☑ (비공개)  ☐ 방 청소
   이번 주 0/1  ☐ 책 한 권
 지연 · 77c1     🍀5  ⭐230 (이번 주 +71)              어제
   …
 [내 친구 코드 복사]  [내 상태 코드 복사]  [코드 붙여넣기 ……… ]
```

정렬은 이번 주 ⭐. 오래된 상태(24시간 이상)는 흐리게 보이고 언제 받은 것인지 적는다 — 지난 날짜의
"오늘" 을 오늘처럼 보이면 안 된다 (`today.key` 가 지금 날짜와 다르면 "어제 기준" 표시).

---

## 6. 자동 동기화 (P3) — iroh

매일 상태 코드를 붙여넣는 것은 귀찮다. 대시보드는 **로그인하면 자동 시작해 트레이에 상주**하므로,
친구들의 PC 가 동시에 켜져 있는 시간이 하루 중 꽤 길다 — 그 시간에 저절로 주고받으면 된다.

### 6.1 방법 비교

| 방법 | 오프라인 친구에게 전달 | 서버 | 내용이 남는 곳 | 판정 |
|---|---|---|---|---|
| **iroh 1.0** (공개키로 연결하는 QUIC P2P) | 둘 다 켜져 있을 때만 → §6.3 의 친구 중계로 보완 | n0 공개 relay·DNS (무료, SLA 없음, 속도 제한). **LAN 은 mDNS 로 외부 의존 0** | 친구 PC 뿐. relay 는 암호문만 지나간다 | **결정** (2026-10-07) |
| Nostr 공개 릴레이 (NIP-78 앱 데이터) | ✅ 릴레이가 마지막 상태를 보관 | 제3자 릴레이 (무료, 언제 지워질지 모름) | 남의 릴레이 — 친구마다 암호화(NIP-44)하지 않으면 공개 | 제외 — 남의 릴레이에 기록이 남는다. 오프라인 전달은 §6.3 친구 중계로 메운다 |
| 공유 클라우드 폴더 (OneDrive 등) | ✅ | 각자의 클라우드 | 공유 폴더 | 친구 모두 같은 서비스 계정·공유 설정 필요. 진입 장벽이 높다 |
| GitHub Gist | ✅ | GitHub | Gist | 친구도 GitHub·PAT 필요 |
| 직접 서버 | ✅ | **우리 서버** | | 요구사항 위반 |

iroh 를 고른 이유: 친구 목록의 **공개키가 그대로 접속 주소**가 되고(§5.2 의 키를 iroh 의 노드 키로
재사용), 데이터가 제3자 서버에 쌓이지 않으며, 같은 네트워크(같은 집·사무실)에서는 relay 를 꺼도 된다.
1.0 이 2026-06-15 에 나와 와이어 프로토콜이 안정됐고, n0 는 공개 relay 를 무료·취미 용도에 계속 열어 두되
SLA 는 없고 최신 안정판만 공식 지원한다고 밝혔다 — 업데이터가 이미 있어 버전을 따라가기는 쉽다.

### 6.2 프로토콜

- ALPN `deskboard/friends/1`, 스트림 하나에 길이 접두 JSON.
- 켜는 조건: 설정 `sync` 가 켜져 있고 **merge 위젯이 하나라도 있을 때**. `merge_set_sync(mode)` 를
  `invokeInOrder` 로 보낸다 (StrictMode 의 켜기→끄기→켜기 순서 문제).
- 내 상태가 바뀌면 30초 디바운스 후, 그리고 10분마다 친구들에게 차례로 다이얼:

```
A → B: Have { A: 128, C: 40, … }          // 내가 가진 스냅샷별 seq
B → A: Snapshots [ B@77, C@41 ]            // A 가 가진 것보다 새것만, 그리고 A 가 audience 에 있는 것만
A → B: Snapshots [ A@128 ]
```

- 받은 스냅샷은 §5.3 과 **같은 검증 파이프라인**(서명 → 친구 여부 → seq)을 거친다. 경로만 다르다.
- 실패한 다이얼은 지수 백오프(최대 1시간). 친구가 꺼져 있는 것은 정상이다.

### 6.3 친구 중계 — 오프라인 친구 메우기

스냅샷은 서명돼 있으므로 **누가 전달해도 위조할 수 없다.** 그래서 A·B·C 가 서로 친구일 때 A 가 꺼져
있어도 B 가 갖고 있던 A 의 최신 상태를 C 에게 건네줄 수 있다. 다만 **A 의 `audience` 에 C 가 있을 때만**
건넨다 — A 가 친구로 삼지 않은 사람에게 A 의 할 일이 퍼지면 안 된다. 친구가 셋 이상이면 대개 누군가는
켜져 있어, 서버 없이도 오프라인 전달이 상당 부분 메워진다.

### 6.4 사용자에게 알릴 것 (설정의 note 로)

- 인터넷 모드는 n0.computer 의 공개 relay·DNS 를 거친다. 직접 연결되면 **친구에게 내 IP 가 보인다.**
  relay 는 내용을 볼 수 없지만 누가 누구와 연결하는지는 안다.
- 처음 켤 때 **Windows 방화벽 허용 창**이 뜰 수 있다(UDP 소켓, 특히 mDNS 수신). 설치가 `currentUser` 라
  설치 훅에서 규칙을 넣을 수 없다 — 거부해도 relay 경유로는 대개 동작한다. **실측 필요**.

---

## 7. 비용·위험

| 항목 | 내용 | 대응 |
|---|---|---|
| 설치 파일 크기 | iroh 와 의존(quinn 등)이 붙는다. tokio·rustls 는 이미 있음 | P3 착수 전 설치본 크기 실측. 크면 cargo feature 로 분리 |
| 상시 비용 | 보드는 조작할 때만 그린다. 회복 카운트다운은 보일 때만 1초 타이머. 동기화는 10분마다 몇 KB | 켬/끔 CPU·트래픽 실측을 PR 에 남긴다 (배경화면 블러 때처럼) |
| 밸런스 | 숫자는 초안 | `merge-economy.json` 덮어쓰기로 다시 빌드하지 않고 조정 |
| 콘텐츠 편집 | 사용자가 계열을 지우거나 레벨을 줄이면 보드에 정체 모를 아이템이 남는다 | 지우기 대신 숨기기(Z-4), 레벨 줄이기는 넘치는 아이템이 없을 때만(Z-5). 그래도 모르는 `(id, level)` 을 만나면 판매 환급만큼 ⚡ 로 바꾼다 |
| 꾸민 이미지 | 큰 파일·이상한 파일 | 128px 로 줄여 저장, 읽기 실패는 사용자 메시지. 원본 크기 상한 20MB |
| 클리커 자동화 | 오토 클리커로 ⚡ 를 무한히 | 막지 않는다 — 개인용 게임이고 ⭐ 의 뜻은 명세 P-7 에 적었다 |
| relay 정책 변경 | 무료 relay 가 바뀔 수 있음 | P2(코드)와 LAN 모드는 영향 없음 |
| 키 분실 | 재설치 시 새 사람 | 키 내보내기는 남은 일 |

## 8. 테스트

- **보드 규칙 (cargo test, `board.rs`)**: 합성·교환·이동, 최고 레벨 합성 불가, 합성 시 이웃 상자만 열림
  (대각선 아님), 거미줄은 같은 아이템으로만, 최고 레벨 거미줄은 이웃 합성으로 풀림, 드롭 위치·가득 찼을 때 ⚡ 미차감,
  숨긴 계열은 생산되지 않음. rng 는 시드 고정.
- **에너지 (`energy.rs`)**: 회복 정산 — 꺼져 있던 3시간, cap 에서 멈춤, cap 위에서 시작, 시계 되돌림, 경계(정확히 120초);
  클리커 나머지 이월; 하루 묶음 UPSERT; 장부 합 = 캐시.
- **할 일 (`todos.rs`)**: 기간 키(자정·`dayStartHour`·ISO 주 경계·연말 주차), 두 번 체크해도 한 번 지급,
  해제 시 되돌림·음수 허용, 지난 기간 잠김, 목표 횟수, 모두 완료·연속 보너스와 그 되돌림.
- **주문 (`orders.rs`)**: 해금·숨김 반영, 좋아하는 계열이 전부 숨겨진 주민은 주문하지 않음, 충족 판정, 보상.
- **콘텐츠 (`content.rs`)**: 기본값 채우기·업데이트 시 없는 id 만 추가, 기본값으로 되돌리기, 레벨 수 줄이기 거부,
  기본 콘텐츠·시작 보드 무결성. **`assets.rs`**: 가로로 긴 이미지 → 128×128 투명 여백, 같은 그림 한 번만 저장, 고아 정리.
- **공유 (`share.rs`)**: 코드 왕복, 잘린 코드·다른 키의 서명·낮은 seq 거부, 비공개 할 일의 `text` 가 스냅샷에
  **아예 안 실리는지**, 꾸민 외형이 안 실리는지, audience 밖으로 중계하지 않는지.
- **동기화 (`sync/`)**: `FriendTransport` Fake 로 Have/Snapshots 교환과 중계 규칙. iroh 실물 테스트는 한 프로세스에
  엔드포인트 둘을 relay 없이 띄워 왕복 한 번 — 외부 망에 의존하는 테스트는 두지 않는다 (CI 는 windows-latest).
- **프론트 (vitest)**: `drag.ts` 좌표→칸 (zoom 0.6~1.4), `regen.ts` 카운트다운, 이모지 한 글자 판정.

## 9. 구현 순서

1. **P0** — `merge-economy.json`·`merge-content.json`·`merge-board.json` → `board.rs`/`orders.rs`/`progress.rs`/`energy.rs`(+테스트)
   → `content.rs`(기본값 채우기만) → `store.rs` → 커맨드 → `Board.tsx` 끌어 놓기·`Clicker.tsx` →
   `App.tsx` `data-capture-keys` → registry 한 줄. 새 의존성 없음.
2. **P1** — `todos.rs` · 할 일 탭 · 머리줄 남은 할 일 배지 · 에너지 기록.
3. **P1.5** — 도감: `assets.rs` · 편집 시트 · 새 계열·주민.
4. **P2** — `identity.rs`(DPAPI) · `share.rs`(`ed25519-dalek`) · 친구 탭 · 코드 복사/붙여넣기.
5. **P3** — iroh: LAN(mDNS) 먼저, 크기·CPU 실측 후 인터넷 모드, 친구 중계.

## 10. 정해진 것

- 자동 동기화는 iroh (§6). 친구에게는 할 일·점수만, 보기만 한다 (명세 P-6).
- 재화는 ⚡ 하나. 할 일·에너지 상한 없음. 시간 회복과 클리커가 있다. 방금 만든 할 일도 바로 ⚡ (명세 "결정된 방향").
- 계열·생산기·주민·클리커·상자의 외형은 사용자가 꾸민다 (명세 §14).
- 나머지 규칙·숫자는 명세의 ✏️ 항목과 §16 조정값 표에서 조금씩 정해 간다.
