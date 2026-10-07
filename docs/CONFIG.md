# 설정 파일로 고치기

코드를 건드리지 않고 셸의 기본값·문구·스타일을 바꾼다. 파일은 데이터 폴더 `%APPDATA%/com.user.deskboard/` 에 둔다
(설정 패널 → "사용자 설정 파일" → "📂 설정 폴더 열기"). **저장하면 바로 반영된다** — 폴더를 감시한다(300ms 디바운스).

| 파일 | 내용 |
|---|---|
| `config.jsonc` | 새로 쓰는 값의 기본값 · 배치 · 크기 프리셋 · 폴링 주기 · 위젯별 덮어쓰기 |
| `strings.jsonc` | 트레이 메뉴 · 편집 바 문구 |
| `user.css` | `theme.css` 위에 덮는 스타일 |

## 쓰는 법

1. 같은 폴더의 `config.sample.jsonc` / `strings.sample.jsonc` / `user.sample.css` 를 같은 이름에서 `.sample` 만 뺀 파일로 복사한다.
2. **바꾸고 싶은 항목만 남긴다.** 적지 않은 것은 기본값을 쓴다.
3. 주석(`//`, `/* */`)과 마지막 쉼표는 써도 된다.

`*.sample.*` 는 **앱이 시작할 때마다 다시 써진다** (직접 고치지 않는다). 실제 파일(`config.jsonc` 등)은 앱이 **절대 만들거나 고치지 않는다.**

## 병합 규칙

- 객체는 **깊게 병합**, 배열은 **통째로 교체** (배치 목록을 "일부만" 덮는다는 건 뜻이 없다).
- 타입이 다르면 기본값을 두고 경고, **모르는 키도 경고**(오타를 조용히 넘기지 않는다). `widgets`·`intervals`·`tray` 안의 키는 열린 맵이라 경고하지 않는다.
- 숫자는 범위로 잘라 넣고 경고, 선택지가 정해진 문자열이 틀리면 기본값.
- 파일이 **깨져 있으면 그 파일만 무시**하고 기본값으로 뜬다 — 손으로 쓴 한 줄이 대시보드를 못 띄우게 하지 않는다.
  JSONC 문법 오류는 `줄:칸`, 병합 경고는 키 경로와 함께 설정 패널 "사용자 설정 파일" 섹션에 뜬다.
- `defaults` 는 **저장된 값이 없을 때의 기본값**이다 — 이미 설정 패널에서 고른 값(`settings.json`)이 항상 이긴다.

## config.jsonc

### `defaults` — 테마 기본값

| 키 | 기본값 | 범위·선택지 |
|---|---|---|
| `themeMode` | `"translucent"` | `translucent` \| `solid` |
| `palette` | `"dark"` | `dark` \| `light` \| `auto`(OS 테마 추종) |
| `cardStyle` | `"glass"` | `glass` \| `minimal` \| `borderless` \| `none` |
| `surfaceOpacity` | `62` | 10~100 |
| `blurStrength` | `3` | 0~6 (0 = 캡처 루프까지 멈춤) |
| `cornerRadius` | `16` | 0~40 px |
| `borderStrength` | `65` | 0~100 |
| `borderWidth` | `1` | 1~6 px |
| `accent` | `"#7c9cff"` | 색 문자열 |
| `gridSnap` | `8` | 1~64 px |
| `autoScale` | `true` | 카드 크기에 맞춘 내용 자동 확대·축소 |

### `layout`

| 키 | 기본값 | 설명 |
|---|---|---|
| `sizePresets` | `[{작게 0.75}, {기본 1}, {크게 1.4}, {아주 크게 1.8}]` | 설정 패널의 크기 프리셋. 항목은 `{ "label": 문자열, "k": 양수 }` (k = 기본 크기 대비 배율), 틀린 항목은 버린다 |
| `contentScale` | `{ "min": 0.6, "max": 2.5 }` | 내용 자동 배율의 한계. `min` 0.1~1, `max` 1~6 |
| `freeSlot` | `{ "step": 40, "margin": 24 }` | 새 위젯을 놓을 빈자리 탐색 간격(4~400) / 화면 가장자리 여백(0~400) |
| `default` | settings · clock · sysmon · claude-usage · spotify · calendar 6개 | **저장된 배치가 없을 때** 놓일 위젯. 항목은 `{ widget, x, y, w, h }` (작업영역 px). 틀린 항목은 버린다 |

기본 `default` 는 `settings(1104,24 56×56)` · `clock(24,24 300×140)` · `sysmon(24,184 360×240)` · `claude-usage(404,24 340×150)` · `spotify(404,194 340×320)` · `calendar(764,24 320×400)`.

### `widgets` — 위젯별 덮어쓰기 (기본 `{}`)

키는 위젯 id(폴더 이름). 적은 항목만 `widget.json` 의 값을 대신한다. 이 설정은 코드를 다시 불러오지 않고 **정의만** 다시 계산한다.

| 키 | 설명 |
|---|---|
| `title` · `icon` | 이름 · 아이콘 (문자열) |
| `defaultSize` · `minSize` | `{ "w": 양수, "h": 양수 }`. `minSize` 가 `defaultSize` 보다 크면 맞춰 줄이고 경고 |
| `settings` | 설정 **기본값** `{ "<스키마 key>": 값 }`. 스키마에 없는 key·타입이 안 맞는 값은 버리고 경고 (select 는 options 안의 값만) |
| `hidden` | `true` 면 "위젯 추가" 목록에서 숨김 (이미 놓인 카드는 그대로) |

```jsonc
"widgets": { "clock": { "title": "시계", "settings": { "digitStyle": "neon" } } }
```

### `intervals` — 폴링 주기 (초)

백엔드가 **매 주기마다** 읽으므로 고치면 다음 주기부터 반영된다. 숫자가 아니거나 0 이하면 기본값, 최솟값보다 작으면 최솟값.

| 키 | 기본값 | 최솟값 |
|---|---|---|
| `sysmon` | 2 | 1 |
| `weather` | 900 | 300 |
| `github` | 300 | 60 |
| `spotify` | 5 | 2 |
| `claudeUsage` | 120 | 60 |

**wallpaper · activity · 창 감시의 타이밍은 일부러 코드에 박아 뒀다** — 실측으로 맞춘 값이라(Win+D 대응 16/50ms, 플레이타임 2초 등) 사용자가 건드릴 값이 아니다.

## strings.jsonc

`tray` 는 백엔드가 읽는다 — 파일이 바뀌면 메뉴를 다시 짓지 않고 `set_text` 로 문구만 바꾼다. 비어 있거나 문자열이 아니면 기본값.

| 키 | 기본값 |
|---|---|
| `tray.toggle_lock` | `편집 잠금/해제` |
| `tray.toggle_theme` | `반투명/단색 전환` |
| `tray.settings` | `설정...` |
| `tray.show` | `표시/숨기기` |
| `tray.quit` | `종료` |
| `tray.tooltip` | `deskboard` |
| `editBar.hint` | `편집 모드 — 헤더 드래그: 이동 · 빈 곳 드래그: 여러 개 선택 · Ctrl/Shift 클릭: 추가 · 테두리·모서리 드래그: 크기` |
| `editBar.multi` | `{n}개 선택 — 하나를 끌면 함께 움직입니다 · Esc: 선택 해제` (`{n}` = 선택한 개수) |
| `editBar.settings` | `⚙ 설정` |
| `editBar.lock` | `🔒 잠금` |

## user.css

앱 스타일 맨 끝(`<head>` 끝, 위젯 CSS 뒤)에 들어간다 — 같은 선택자(`:root`)면 기본값을 덮어쓴다. HTML(iframe) 위젯에는 바뀐 토큰이 다시 전달된다.
쓸 수 있는 토큰 목록은 `user.sample.css` 머리말에 있다.

**`!important` 가 필요한 토큰**: 앱이 설정 슬라이더 값을 `<html>` 에 **인라인 스타일**로 직접 꽂는 `--accent` · `--radius` · `--surface-alpha` · `--border-k` · `--border-w`.
인라인은 일반 CSS 보다 우선하므로 이 다섯 개를 덮으려면 `!important` 를 붙인다.

팔레트별로 나누려면 `:root[data-palette="light"]` 처럼 속성으로 좁힌다 (`data-palette` · `data-theme-mode` · `data-card-style`).

```css
:root { --fs-value: 32px; --accent: #ff7ab8 !important; }
```
