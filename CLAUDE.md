# Project: deskboard

Windows 바탕화면에 상주하는 개인 위젯 대시보드 (Tauri 2 + React 19 + TypeScript + Rust).
프레임 없는 투명 창(always-on-bottom)에 둥근 카드 위젯을 자유 배치한다.

## 기본 위젯
위젯 코드는 저장소 `widgets/<id>/` 에 있다 (앱에 컴파일하지 않는다 — 아래 "위젯 모듈 시스템"). 아래 표의 Provider 는 백엔드에 컴파일돼 있다.

| id | 백엔드 Provider | 데이터 |
|---|---|---|
| `clock` | 없음 | 순수 프론트. 스타일 8종 — 격자 숫자 4종(블록·네온·LED매트릭스·문자도트, `DotDigits` 공용) · 시스템 폰트 · 워드 클락(`words.ts`, en/ko) · 바이너리 · 피보나치(`oddtime.ts`) |
| `sysmon` | `providers/sysmon` | sysinfo(CPU/RAM/네트워크) + nvidia-smi(GPU) + LibreHardwareMonitor(CPU 온도, 선택). CPU 상위 프로세스는 위젯이 켤 때만 수집(`sysmon_set_detail`) — 목록 갱신이 비싸다 |
| `claude-usage` | `providers/claude_usage` | **구독 한도 %** (5시간 / 주간 전체모델): 위젯 내 OAuth 로그인(`auth.rs`, Claude Code 와 같은 PKCE 흐름·클라이언트 id, 코드 붙여넣기) → 토큰은 `%APPDATA%/com.user.deskboard/claude.json` → `api.anthropic.com/api/oauth/usage` 120초 폴링(429 시 5분 백오프) + 트랜스크립트 변경 시 즉시. Claude Code 의 credentials 파일은 건드리지 않음. 로컬 비용 추정(`*.jsonl` 파싱, `resources/pricing.json`)은 옵션 |
| `calendar` | `providers/calendar` | SQLite (`%APPDATA%/com.user.deskboard/calendar.sqlite`) |
| `spotify` | `providers/spotify` | PKCE 로그인, Web API 5초 폴링(위젯 표시 중에만). 위젯이 막 떴을 때·곡이 끝났을 때(`spotify_poll`)는 주기를 기다리지 않는다 — 진행 시간은 백엔드의 `fetched_at` 을 기준으로 **프론트의 `Progress` 안에서만** 1초마다 보간한다 |
| `settings` | 없음 | **셸 위젯** — `widgets/` 폴더가 아니라 `core/widgetRegistry.ts` 에 정적으로 등록된 유일한 위젯(`src/shell/SettingsLauncher.tsx`; 설정 진입점은 최소 동작이라 코드에 남긴다). 톱니바퀴 아이콘만 있는 정사각형(56×56). 클릭 = 편집 모드(잠금) 토글. 설정 패널 자체는 편집 모드의 편집 바 "⚙ 설정" 버튼이나 위젯 오버레이 ⚙ 로 연다 → `singleton` (항상 1개, 제거 불가, 로드 시 없으면 자동 추가·정사각형이 아니면 기본 크기로 보정) |
| `wallpaper` | `providers/wallpaper` | 위젯 아님 — 배경화면을 읽어 블러한 스냅샷을 `wallpaper://update` 로 푸시 (카드 뒤 '진짜 반투명' 재료) |
| `weather` | `providers/weather` | [Open-Meteo](https://open-meteo.com) — **키·가입 불필요**. 위젯이 떠 있을 때만 15분 폴링, 도시 검색은 `weather_search`(지오코딩, 역시 키 불필요) |
| `notes` | `providers/notes` | 메모·할 일. SQLite (`notes.sqlite`). 목록은 **위젯 인스턴스마다 독립** — 여러 개 띄워 용도별로 나눠 쓴다. 순서는 손잡이(⠿)를 끌어 바꾼다 — 계산은 `notes/reorder.ts`("어느 항목 **앞**에 둘지"로 말해 완료 항목을 숨겨 둔 목록에서도 숨은 것들이 제자리를 지킨다), 저장은 손을 뗄 때 `notes_reorder` 한 번 |
| `playtime` | `providers/activity` | `playtime` 은 게임(분류 규칙)을 자동으로 세고, 그 밖의 프로그램은 설정의 `extra` 에 등록한 것만 더해 도넛으로 보여준다 — "게임만"과 "앱 전부"로 위젯을 나누면 같은 데이터를 두 번 보게 된다. 앞에 떠 있는 창의 실행 파일 이름만 **2초마다** 확인해 `activity.sqlite` 에 누적 — **창 제목은 저장하지 않는다.** 확인은 자주, 기록은 드물게(30초마다 한 번 flush) — 비싼 것은 Win32 호출이 아니라 SQL 이다. 더하는 값은 **실제로 흐른 시간**(`MAX_TICK` 으로 자름)이고, 전경이 대시보드 자신이거나 읽지 못한 경우(`Foreground::Ours`/`Unknown`)에는 **아무것도 하지 않는다** — 위젯을 클릭했다고 세던 구간을 끊으면 하루가 토막 난다. 입력이 3분 없으면 자리비움으로 보고 세지 않고, 대시보드 자신도 세지 않는다. 분류는 `resources/activity-rules.json` + 사용자 규칙(`rules` 테이블이 우선). **목록에 없는 게임은 기록은 되지만 위젯에서 걸러진다** — 그래서 두 길을 더 뒀다: ① 게임 설치 폴더(`steamapps\common`·`Epic Games`·`Riot Games`·`XboxGames` 등, `rules::looks_like_game_path`)에서 실행된 exe 는 처음 볼 때 `rules` 에 `game` 으로 넣는다(`set_rule_if_absent` — seed 와 사용자 선택이 우선, **경로는 판단에만 쓰고 저장하지 않는다**), ② "셀 프로그램 고르기"의 🎮 버튼(`activity_set_category`). `extra` 는 사람 이름("VS Code")도 받는다(`parseExtras` 가 exe 이름으로 바꾼다) |
| `github` | `providers/github` | 기여도 잔디(1년)·리뷰 요청·담당 이슈·미확인 알림, 그리고 등록한 저장소(8개)마다 열린 PR/이슈·내 차례인 것·그 저장소 알림·기본 브랜치 CI. **주기마다 호출은 두 번뿐** — GraphQL 하나(`parse::build_query`, 저장소는 `r0:` `r1:` 별칭으로 나란히)와 REST `/notifications` 하나(GraphQL 에 알림 API 가 없다). 저장소를 늘려도 호출 수는 그대로다. classic PAT 에 `repo`·`notifications`·**`read:user`**(잔디) 필요. PAT 는 **DPAPI 로 암호화**해 `github.dat` 에 (`providers/secrets`) |
| `folder` | `providers/folders` | 디스코드식 바로가기 폴더. 인스턴스당 실제 디렉터리 1개. **`settings.source` 가 두 가지를 가른다** — `managed`(전용 폴더를 `%APPDATA%/com.user.deskboard/folders/<instanceId>` 에 만들어 쓴다. `dir` 은 쓰지 않고 저장도 하지 않는다)와 `link`(사용자가 고른 기존 폴더. **없으면 만들지 않고** 위젯이 이유를 보여준다). 드롭 시 이동/복사, `IShellItemImageFactory` 로 아이콘 추출, `notify` 로 변경 감지 |
| `scrap` | `providers/scrap` | 📰 주제(프롬프트)별 기사·자료 모음. 백엔드 셋(`backend` 설정, 기본 `auto` = 쓸 수 있는 것을 **api → cli → rss** 순으로 시도, 실패하면 다음으로 넘어가고 이유는 `note` 로 남긴다): ① **Claude API 웹 검색**(`api.rs`, `web_search_20260209` + `web_fetch_20260209`, 기본 `claude-opus-5-5` / 선택 `claude-sonnet-5-5`, 구조화 출력은 거부되면 포맷 없이 재요청하는 폴백, API 키는 DPAPI `anthropic.dat`) ② **Claude Code CLI**(`cli.rs`, `claude -p` + WebSearch/WebFetch, 구독 한도 사용. exe 는 PATH → `~/.local/bin` → Claude 데스크톱 앱의 `%APPDATA%/Claude/claude-code/<버전>/<해시>`. 인증은 **Claude 한도 위젯과 공유하는 앱 내 OAuth 로그인**(`claude.json`, 🔑 → 브라우저로 로그인, 터미널 불필요) — `claude_usage::auth::valid_token` 이 만료 전 갱신한 access token 을 `CLAUDE_CODE_OAUTH_TOKEN` 으로 넘긴다. 갱신은 전역 뮤텍스(`refresh_locked`)로 직렬화(리프레시 토큰이 돌아간다). 로그인이 없으면 CLI 자체 로그인에 맡긴다. `claude setup-token` 은 없앴다) ③ **Google 뉴스 RSS**(`rss.rs`, 무료·요약 없음). **구독 OAuth 토큰으로 API 직접 호출은 안 된다**(실측 429 "Error") — 그래서 구독은 CLI 로만 쓴다. 결과는 인스턴스별 `scrap/<instanceId>.json` 캐시 — 재시작해도 다시 부르지 않는다. 자동 갱신 끔/3h/6h/매일, **프롬프트가 바뀌었으면 자동으로 돌리지 않고** 수동 새로고침을 권한다(비용). 웹 내용은 텍스트로만 렌더 |

새 위젯: [docs/WIDGETS.md](docs/WIDGETS.md) — `widgets/<id>/` 폴더 하나(`widget.json` + `index.tsx`). 재빌드 없이 사용자 폴더에도 둘 수 있다. 셸 설정 파일은 [docs/CONFIG.md](docs/CONFIG.md).

## 구조
```
widgets/                  위젯 폴더 (내장). <id>/{widget.json, index.tsx, …} — 런타임에 로드, 앱 번들에는 안 들어간다
src/                      React 프론트
├─ core/    theme.css(토큰) · settings.ts(zustand + plugin-store 영속) · ipc.ts(이벤트/커맨드 훅)
│           widgetRegistry.ts(위젯 목록·정의·config 덮어쓰기) · widgetTypes.ts · config.ts(셸 설정 병합·t())
│           frameBridge.ts(iframe 메시지 허용 목록) · loader/(compile=sucrase+해시 캐시 · modules=CJS 평가 · host=import 허용 목록)
├─ sdk/     "deskboard" 모듈 — 위젯이 import 하는 것 전부 (index.ts · context.ts)
├─ shell/   셸에 박힌 위젯 (SettingsLauncher)
└─ components/  WidgetHost(정의 조회·오류 격리·command 수명) · FrameWidget(iframe) · WidgetFrame(카드·이동·크기조절) · Canvas · SettingsPanel · Gauge · Sparkline
src-tauri/src/
├─ lib.rs        플러그인·프로바이더·커맨드 등록, `dbw` 스킴 등록, setup 에서 userland::init
├─ window.rs     트레이 메뉴(문구는 strings.jsonc), 작업영역 맞춤, 히트영역 click-through, Win+D 제외
├─ userland/     위젯 폴더 스캔(scan)·`dbw` 스킴(scheme)·명령 실행(runner)·저장소(storage)·설정 파일(config/jsonc)·샘플 seed
└─ providers/    mod.rs(Provider 트레이트 + all()) · <id>/
src-tauri/resources/userland/   데이터 폴더에 깔리는 샘플·사용자 README·iframe bridge.js
```

## 명령
```bash
npm run tauri dev      # 개발 실행 (Rust 변경 시 자동 재시작, 프론트는 HMR)
npm run typecheck      # tsc
npm test               # vitest (src-tauri/** 제외 — tauri 가 widgets/ 를 target/ 으로 복사한다)
cd src-tauri && cargo test
npm run tauri build    # NSIS 설치 파일 → src-tauri/target/release/bundle/nsis/
```

## 배포·업데이트
- **저장소에는 소스만 올린다.** `v*` 태그를 밀면 `.github/workflows/release.yml` 이
  windows-latest 에서 검사(typecheck·vitest·cargo test) → `tauri build` → 서명 → 릴리스에
  `*-setup.exe` · `.sig` · `latest.json` 을 붙인다. 절차는 [docs/RELEASE.md](docs/RELEASE.md).
- 앱은 `tauri-plugin-updater` 로 `releases/latest/download/latest.json` 을 본다
  (`tauri.conf.json` 의 `plugins.updater`). 확인은 **시작 8초 뒤 한 번**과 설정에서 누를 때뿐 —
  주기적으로 긁지 않는다 (`core/updater.ts`, 설정 패널의 "업데이트" 섹션).
  상태는 zustand 스토어라 어디서나 읽는다 — 새 버전이 있으면(`available`/`ready`) 톱니 위젯 모서리·편집 바
  "⚙ 설정"·설정 패널 "업데이트" 제목에 강조색 6px 점(`components/UpdateDot`)이 뜬다. 애니메이션 없음.
  DEV 에서는 콘솔의 `__updater.setState({state:{kind:"available",version:"9.9.9",notes:""}})` 로 흉내 낸다.
- 업데이터 플러그인은 **release 빌드에서만** 건다 (`lib.rs`). 개발 실행의 버전은 설치본과
  무관해서, 걸어 두면 `tauri dev` 가 매번 자기를 업데이트하겠다고 나선다. 프론트도
  `UPDATER_ENABLED` 로 DEV 에서는 확인을 건너뛴다.
- **설치 전에 실행 중인 앱을 끝낸다** (`src-tauri/nsis-hooks.nsh`, `nsis.installerHooks`).
  덮어 설치는 옛 버전의 언인스톨러를 먼저 돌리는데, 그때 `deskboard.exe` 가 잠겨 있으면
  "기존 버전을 제거할 수 없습니다" 로 멈춘다 — 자동 시작이 켜져 있으니 사실상 항상 그렇다.
  창이 트레이에만 있어 "닫아 주세요" 안내로는 빠져나갈 수 없으므로 훅에서 `taskkill` 한다.
  **실제로 구해 주는 것은 `NSIS_HOOK_PREUNINSTALL` 쪽이다** — 덮어 설치는 새 설치본이 옛
  언인스톨러를 먼저 `ExecWait` 하고 그게 실패하면 멈추므로, 훅은 **이미 깔려 있던 버전**의
  것이 돈다. 그래서 이 기능은 0.3.0 이 깔린 **다음** 설치부터 효과가 난다.
- 서명 키는 `%USERPROFILE%\.tauri\deskboard.key` (저장소 밖). 공개 키만 `tauri.conf.json` 에
  있고, CI 는 `TAURI_SIGNING_PRIVATE_KEY` 시크릿으로 서명한다. **개인 키를 잃으면 이미 깔린
  앱들이 새 릴리스를 거부한다** — 사용자가 설치 파일로 한 번 덮어써야 복구된다.

## 시작/종료
- 자동 시작: `autostart.rs::sync` 가 시작 시 `settings.json` 의 `v1.autostart`(기본 true) 에 맞춰 HKCU Run 값을 enable/disable (enable 은 현재 exe 경로로 덮어써 교정). **debug 빌드는 절대 등록하지 않고**, Run 값이 `\target\debug|release\` 를 가리키면 지운다. 설정 토글은 `settings.ts::setAutostart`.
- 단일 인스턴스: `tauri-plugin-single-instance` — release 빌드에서만. 두 번째 실행은 기존 창 `show()` 후 종료.
- 트레이 종료: `ui://quit` 이벤트로 프론트가 설정을 즉시 저장한 뒤 400ms 후 `app.exit`.

## 창 동작
- 창은 항상 완전 투명(OS 블러 없음). 카드만 CSS `--surface` 로 그린다.
  모양은 `<html>` 의 세 속성이 결정한다 (`settings.ts::applyTheme`):
  `data-theme-mode`(translucent|solid) · `data-palette`(dark|light, 설정의 `auto` 는 OS 테마 추종) ·
  `data-card-style`(glass|minimal|borderless|none).
  카드 불투명도·블러 강도·모서리 반경·**테두리 진하기·두께**는 설정 슬라이더 →
  `--surface-alpha`/`--radius`/`--border-k`/`--border-w`. `--border-k`(0~1)는 팔레트별 기준
  알파에 섞여 `--border` 와 `--card-highlight` 를 함께 진하게 한다 — 기본 0.65 로, 배경화면
  위에서 카드 경계가 묻히지 않는 값이다. 두께(`--border-w`, 1~6px)는 **따로 둔다** —
  얇고 진한 선과 두껍고 은은한 선은 다른 모양이다.
- 창은 선택 모니터의 작업영역(`GetMonitorInfoW.rcWork`) 전체를 덮는다 (`window.rs::fit_to_work_area`, 2초마다 변화 감지). 위젯 좌표 = 작업영역 좌표.
- **z-order 는 불변식 하나로 관리한다 — 바탕화면 바로 위, 나머지 앱 아래**
  (`window.rs::enforce_z_order`). Win+D 가 Progman 을 끌어올리면 다시 그 위로 돌아간다.
  창은 평범한 최상위 창이라 DWM 알파 합성·포커스·IME 가 모두 정상이다.
  - **`SetWindowPos` 의 두 번째 인자를 조심할 것.** `hWndInsertAfter` 는 "positioned window
    **앞에 올** 창"이다 — 우리는 그 창 **바로 아래**로 간다. 그래서 `SetWindowPos(me, 바탕화면)`
    은 바탕화면 *밑에* 묻는다. 이걸 "바로 위로 올린다"고 착각한 채 오래 썼고, **올리는 쪽이
    한 번도 작동하지 않은 진짜 이유**가 이것이었다. 바로 위로 가려면
    `GetWindow(바탕화면, GW_HWNDPREV)` 를 대상으로 준다.
  - 훅(`SetWinEventHook`, `EVENT_SYSTEM_FOREGROUND`~`EVENT_SYSTEM_MINIMIZESTART`)은 **신호로만**
    쓴다. 셸은 전경을 먼저 바꾸고 **그 다음에** Progman 을 올리므로(실측: 훅이 뜬 시점에
    `desktop_above` 는 아직 false), 훅에서 한 번 고쳐 봐야 곧 덮인다. 훅은 **버스트**를 켜고
    (`arm_burst`) 실제 교정은 루프가 한다. Win+D 는 창을 하나씩 최소화하므로 매 단계가 버스트를
    갱신한다 — 창이 많은 기계에서는 저절로 길어져 상수를 손으로 맞출 필요가 없다.
  - **주기는 동적이다**: 평소 50ms, 버스트 중 16ms(≈한 프레임). 깜빡임은 *얼마나 자주 고치느냐*가
    아니라 *잘못된 상태가 얼마나 오래 유지되느냐*다 — 화면은 프레임마다 합성되므로 그 안에
    되돌리면 아예 그려지지 않는다. 실측: 200ms → 1.5초 내내 어긋남(사실상 상시), 16ms → 15~49ms.
    **8ms 까지 내리지는 않는다** — 그 속도면 셸의 "바탕화면 보기" 절차에 끼어들어 Win+D 가
    8회 중 1~2회 통째로 씹혔다. 유휴 CPU 는 1.30% 로 원래 수준 그대로다.
  - 싼 검사(`first_window_below`)가 매 틱의 비용을 떠받친다 — `EnumWindows` 대신 `GW_HWNDNEXT`
    로 아래로 몇 걸음만 내려가 "바로 아래가 바탕화면인가" 만 묻는다. 깨졌을 때만 `scan_z` 를 돈다.
    두 가지를 빠뜨리면 검사가 **영영 거짓**이 되어 쉬지 않고 헛교정을 돌린다 (둘 다 실측으로 당함):
    **다른 가상 데스크톱의 창**(DWM cloaked — `IsWindowVisible` 이 참이라 안 거르면 끼어 있는 것처럼
    보인다)과 **Progman 이 `WS_EX_TOOLWINDOW` 를 달고 있다는 것**(도구 창을 먼저 걸러내면 정작
    찾던 바탕화면을 건너뛰고 목록 끝까지 간다 — 클래스를 **먼저** 본다).
  - 같은 자리를 노리는 다른 바탕화면 위젯 앱(Rainmeter 등)과 무한히 싸우지 않도록, 고쳐도
    소용없는 일이 두 번 연속이면 버스트를 켜지 않는다 (`INEFFECTIVE`).
  - **전경이 우리일 때는 내리지 않는다** — 위젯 클릭으로 얻은 포커스와 싸우지 않기 위해서다.
    그동안 잠깐 앱 위로 올라올 수 있다(허용). 아예 못 올라오게 하는 건 남은 일.
  탈락한 시도들은 `window.rs` 의 "Win+D 에서 살아남기" 머리말에 실측과 함께 남겨 두었다 —
  `WS_MINIMIZEBOX` 제거(이미 적용돼 있어 무의미) · topmost(이 창에서 `WS_EX_TOPMOST` 가 안 켜짐) ·
  `SetParent`(자식 창은 DWM 알파 합성을 못 받아 아이콘이 사라짐) ·
  **소유자(owner) 지정**(Win+D 는 풀리지만 활성화까지 끌고 와 셸의 `ToggleDesktop` 방향 판정이
  멈추고 한/영이 죽는다) · **`visible_apps == 0` 으로 "바탕화면 보기" 판정하기**(끝내 최소화되지
  않는 창이 하나만 있어도 영영 거짓. 지금 설계는 이 판정을 **아예 하지 않는다** — 불변식이
  깨졌는지만 묻는다).
- `alwaysOnBottom` 은 **끈다**(`tauri.conf.json`). 맨 아래로 두면 바탕화면 밑으로 깔린다.
  `exclude_from_show_desktop`(`WS_MINIMIZEBOX` 제거)과 2초마다의 `IsIconic` →
  `SW_SHOWNOACTIVATE` 는 최소화에 대한 보험으로 남겨 둔다.
- **칠 것이 없는 위젯을 눌렀을 때는 키보드 포커스를 돌려준다** (`window.rs::give_focus_back_if_idle`).
  대시보드가 전경이 되면 Windows 는 IME 를 이쪽으로 옮기는데, 입력 요소가 없으면 Chromium 이
  "여기엔 입력이 없다" 고 알려 표시기가 **"IME 를 사용하지 않습니다"** 가 된다. 그대로 남으면
  한/영 키가 갈 곳을 잃는다 — 시계를 한 번 눌렀을 뿐인데 다른 앱에서 한글이 안 쳐진다.
  - 프론트엔드(`App.tsx`)가 `focusin`/`focusout` 으로 **입력 요소가 포커스를 쥐었는지**
    `ui_set_text_focus` 로 알려 준다. 그게 참이면 포커스를 그대로 둔다.
  - **250ms 기다렸다가 판단한다.** 창이 먼저 활성화되고 웹뷰의 포커스 이벤트는 몇 ms 뒤에 온다 —
    즉시 판단하면 정당한 입력란 클릭까지 되돌려 버린다.
    실측: 위젯 클릭 시 82ms 에 전경을 얻었다가 290ms 에 반환, 입력란 클릭 시에는 계속 유지.
  - **`WS_EX_NOACTIVATE` 로 막는 방식은 쓸 수 없다** (실측). 활성화는 확실히 막히지만 클릭이
    DOM 포커스도 잡지 못해 텍스트 입력이 통째로 죽는다 — 입력란을 누르고 쳐도 글자가 안 들어갔다.
  - 돌려줄 대상은 훅이 기억한 "우리가 아니었던 마지막 전경 창"이다. 바탕화면·작업표시줄은
    제외한다 — 거기로 돌려주면 IME 가 다시 갈 곳을 잃는다.
- 히트 영역 click-through (`window.rs::start_hit_test`): 잠금 상태에서 커서가 위젯 사각형 밖이면 `set_ignore_cursor_events(true)` → 빈 영역 클릭이 바탕화면 아이콘으로 통과. 프론트가 `set_hit_regions` 로 사각형을 보낸다 (편집 모드·설정 패널 열림 = 비활성).
- 팝업·메뉴 닫기는 `core/dismiss.ts::useDismiss` (창 안 바깥 클릭 + 히트 영역 밖 클릭 `hit://outside-press` + Esc). 창 `blur` 는 always-on-bottom 창에서 클릭 직후에도 발생하므로 쓰지 않는다. 위젯 밖으로 튀어나오는 팝업은 `setOverlayRect` 로 히트 영역 등록.
- 편집 모드의 **다중 선택**: 빈 곳 드래그(마키) 또는 Ctrl/Shift 클릭으로 고르고, 하나를 끌면 전부 같은 만큼 움직인다
  (`core/layout.ts` — 격자 맞춤은 끄는 위젯에만 적용하고 그 차이를 전부에 더한다. 각자를 따로 맞추면 간격이 무너진다.
  경계 보정도 묶음 전체로 본다). 원위치는 드래그를 **시작할 때** 찍는다 — 매 프레임 현재 좌표에 더하면 반올림 오차가 쌓인다.
  선택은 저장하지 않고 잠그면 비운다. Esc 는 선택 해제 → 잠금 순으로 한 단계씩.
- 내용 자동 맞춤: zoom = `contentScale()`(기본 크기 대비 비율, autoScale 꺼지면 1) × 넘침 보정(`WidgetFrame` 이 body 의 scroll/client 비율을 재서 넘치면 축소). 내용이 잘리지 않음을 보장. 위젯은 배율을 뺀 `size` 를 받는다.
  내용 여백(`--wpad`)도 카드 크기를 따라간다(짧은 변의 7%, 6~14px). **배율로 나눠서 넣는다** —
  여백은 body 안에 있어 `zoom` 과 함께 커지므로, 안 그러면 크게 키운 위젯만 테두리가 뚱뚱해진다.
- 크기 조절은 **네 변 + 네 모서리 여덟 방향**이다 (`core/layout::resizeBox`, `WidgetFrame` 의
  `.widget-resize.r-*`). 끄는 변만 옮기고 반대쪽 변은 시작 자리에 고정한다 — 왼쪽/위를 끌면
  `x`/`y` 가 함께 줄어든다. 격자 맞춤은 **끄는 변의 화면 좌표**에 건다(크기에 걸면 반대쪽 변이
  격자에서 떨어진다). 손잡이 띠는 카드 **안쪽**으로 깐다 — 바깥으로 내밀면 히트 영역
  (`set_hit_regions`, 위젯 사각형 그대로)을 벗어나 클릭이 통과한다.

## 위젯 모듈 시스템
세부는 [docs/WIDGETS.md](docs/WIDGETS.md) · [docs/CONFIG.md](docs/CONFIG.md). 여기는 건드릴 때 걸리는 것만.
- **위젯 = 폴더, 폴더명 = id.** 두 루트: 내장(`widgets/`, release 는 `bundle.resources` 로 `$RESOURCE/widgets`, **debug 는 저장소를 직접 읽어 저장 즉시 핫 리로드**)과
  사용자(`%APPDATA%/com.user.deskboard/widgets/`). 같은 이름이면 사용자 폴더가 내장본을 덮어쓴다. `_`/`.` 접두 = 비활성. 깨진 폴더도 오류와 함께 목록에 남는다.
- **로더**: 백엔드 `widgets_bundle` → sucrase(지연 로드 청크, 변환 결과는 소스 해시로 IndexedDB 캐시) → `new Function` 으로 CJS 평가, 커스텀 `require`.
  **위젯이 import 할 수 있는 것**: `react` · `react/jsx-runtime` · `react-dom` · `deskboard`(`src/sdk/index.ts`) · `date-fns` · `date-fns/locale` · 같은 폴더 상대 파일(`.css` 는 `user.css` 앞에 주입, `.json`).
  그 밖은 throw. 허용 목록은 `core/loader/host.ts`, 늘리면 사용자 README(`resources/userland/widgets/README.md`)도 고친다.
  `host.ts` 의 모듈 맵은 **처음 쓸 때 만든다** — sdk 가 settings → 레지스트리 → host 로 이어지는 순환 import 안에 있어 평가 시점에 펼치면 초기화 전 바인딩을 만진다.
- **SDK** `deskboard` = tsconfig `paths` + vite alias(vitest 용)가 `src/sdk/index.ts` 를 가리킨다. 위젯 훅·유틸을 늘릴 때는 여기에 내보내고 WIDGETS.md 표를 고친다.
- **레지스트리**(`core/widgetRegistry.ts`): 셸 위젯은 settings 하나. `widgets://changed` 를 받아 **지문(version)이 바뀐 위젯만** 다시 불러온다.
  `config.jsonc` 의 `widgets.<id>` 덮어쓰기(`applyOverrides`)는 정의만 다시 계산한다. **시작 순서: `config.init` + `registry.init` → `settings.load`** (migrate 가 모듈을 필요로 하고, 기본값이 config 를 필요로 한다).
  두 init 은 throw 하지 않는다 — 위젯을 하나도 못 불러도 셸은 뜬다.
- **정의가 없는 인스턴스는 `load()` 가 버리지 않는다** → `MissingWidget` 카드(폴더가 돌아오면 같은 설정으로 뜬다, 지우는 건 사용자가 ✕). 옛 `migrate` 분기는 각 위젯 모듈의 `export function migrate` 로 옮겨 갔다.
- **오류 격리**: `WidgetHost` 가 카드마다 error boundary + `file:line` + "다시 시도". 덮어쓴 사용자 폴더가 깨졌으면 "내장본으로 되돌리기"(`widgets_disable`: 폴더명에 `_`). **무한 루프는 격리할 수 없다.**
- **command**(`widget.json`): 실행기는 인스턴스별 직렬 + 전역 4개, 명령 문자열은 **스캔한 manifest 에서만**, 설정은 **환경 변수로만**(`DESKBOARD_SETTING_*`). 시작·정지는 **셸이** `WidgetHost` 에서 `invokeInOrder` 로 건다(위젯은 `useCommand` 로 읽기만).
  함정: ① PowerShell 은 UTF-8 설정을 앞에 붙이지만 **`cmd` 의 내장 `echo` 는 `chcp 65001` 을 무시**하고 ANSI(CP949)로 낸다 — 한글은 `type`/PowerShell. ② `.ps1` 을 가리키면 `-File` 로 돌아 인코딩 설정이 안 붙는다(스크립트에서 직접).
  ③ **타임아웃은 직계 자식만 죽인다** — 손자 프로세스는 남는다(프로세스 트리 kill 은 TODO, Job Object).
- **storage**: `useStorage` → `userdata/<instanceId>.json`(256KB, tmp → rename). 평문이므로 비밀값은 넣지 않는다.
- **HTML(iframe) 위젯**: `sandbox="allow-scripts allow-forms"`(same-origin 없음). 파일은 커스텀 스킴 `dbw` — **`http://dbw.localhost/<id>/<file>`**(Windows). `convertFileSrc` 는 `/` 까지 `%2F` 로 인코딩해 상대 경로가 안 풀리니 쓰지 않는다(`frameBridge.dbwUrl`).
  `bridge.js` 는 스킴이 `<head>` 앞에 자동 주입. **권한의 전부는 `frameBridge.ts` 의 메서드 화이트리스트 + `PUBLIC_EVENTS`(∩ `subscribe`)** — 늘릴 때는 켜야 도는 provider 이벤트를 열지 않는다. 메시지는 `e.source` 로도 검증.
  **IME**: 브리지가 `focus{editable}` 을 보내면 `iframe.dataset.editable` 을 세우고 `App.tsx::editable()` 이 이를 입력 요소로 인정한다 — 빠지면 포커스를 돌려줘 한글이 안 쳐진다.
- **셸 설정 파일**(데이터 폴더, 상세 CONFIG.md): `config.jsonc` / `strings.jsonc` / `user.css` — 감시해서 핫 리로드. `*.sample.*` 은 **매 시작 덮어쓰고 실제 파일은 절대 건드리지 않는다**(seed).
  병합: 객체 깊은 병합·**배열 교체**·타입 불일치/모르는 키는 기본값+경고(설정 패널 "사용자 설정 파일" 섹션)·깨진 파일은 무시. 트레이 문구는 `set_text` 로만 갱신(메뉴 재생성 없음, `init` 이 `build_tray` **전에** 돈다).
  폴링 주기는 `userland::config::interval(key, default, min)`(sysmon/weather/github/spotify/claudeUsage) — wallpaper·activity·창 감시 타이밍은 실측값이라 **일부러 코드에 둔다**.
  **`user.css` 로 앱이 인라인으로 꽂는 토큰(`--accent --radius --surface-alpha --border-k --border-w`)을 덮으려면 `!important`.**
- **설정 패널**: 위젯 목록을 내장/내 위젯으로 나누고 오류 배지·"폴더 열기"·내장 위젯 "복사해서 고치기"(`widgets_eject`)를 둔다.
- **테스트**: `src/core/loader/widgets.test.ts` 가 `widgets/*` 전부를 실제 로더로 불러 본다(import 허용 목록·default export·manifest). 새 위젯은 자동으로 걸린다.
- **보안 트레이드오프**: JSX 위젯은 **앱 전체 권한**으로 돈다(README·패널에 경고). 격리가 필요하면 iframe 위젯. 로드 대상은 두 루트뿐, 네트워크 설치 경로 없음.

## 규칙
- 색상·타이포는 `core/theme.css` 의 CSS 변수만 쓴다. 하드코딩 금지.
  - 색: `--accent --text --text-dim --surface --surface-strong --border --ok --warn --danger`
    `--fill/--fill-hover/--fill-active`(표면 위 채움) `--track`(빈 진행바) `--shadow-card/--shadow-pop` `--scrim`
    `--on-light`(밝은 브랜드색 위 글자, 고정) `--on-text`(`--text` 를 배경으로 깔았을 때의 글자색, 팔레트마다 뒤집힘)
  - 타이포: `--fs-value/-title/-sub/-label/-meta/-micro`, `--fw-value/-title/-bold/-label`
  - 모션: `--ease --dur --dur-fast` (`prefers-reduced-motion` 은 theme.css 가 전역으로 처리)
- 위젯은 서로 import 하지 않는다. **`deskboard` SDK · `react`·`react-dom`·`date-fns` · 자기 폴더 파일만** import 한다 (`core/`·`components/`·`@tauri-apps/*` 직접 import 는 로더가 막는다).
- 백엔드를 켜고 끄는 토글(`*_set_active` 류)은 **`core/ipc.ts::invokeInOrder`** 로 보낸다.
  `invoke` 는 도착 순서를 보장하지 않는데, StrictMode 가 effect 를 마운트→언마운트→마운트로
  돌리면 `true → false → true` 가 연달아 나간다. `false` 가 마지막에 닿으면 기능이 꺼진 채로
  남는다 — 실제로 활동 추적이 이렇게 통째로 죽어 있었다.
- Provider 는 `start()` 에서 즉시 반환하고 백그라운드에서 `app.emit("<id>://update", ..)`.
- 외부 자원(OS 센서, 파일, API)은 트레이트로 추상화 (`SensorSource`, `CalendarSource`), 테스트는 Fake/픽스처로.
- 커맨드 오류는 `Result<_, String>` 사용자 메시지.
- 편집 모드(잠금 해제)는 저장하지 않는다. 시작 시 항상 잠김.
- 설정 저장 위치: `%APPDATA%/com.user.deskboard/`
  - `settings.json` (레이아웃·테마·모니터) · `calendar.sqlite` · `notes.sqlite` · `activity.sqlite`
  - `widgets/` (사용자 위젯 + 예제 `_example-*`) · `userdata/<instanceId>.json` (위젯 저장소) · `scrap/<instanceId>.json` (스크랩 결과 캐시) · `folders/<instanceId>`
  - `config.jsonc` · `strings.jsonc` · `user.css` (사용자가 만드는 설정 파일) · `*.sample.*` · `widgets/README.md`·`widget.schema.json` (매 시작 덮어쓰는 샘플·문서)
  - `spotify.json` / `claude.json` — 토큰이 **평문**이다. 새로 쓰는 비밀값은 `providers/secrets`
    (Windows DPAPI, 현재 계정으로만 복호화)를 거친다: `github.dat` · `anthropic.dat`(스크랩 API 키). 기존 둘의 이전은 남은 일.
  - `pricing.json` / `activity-rules.json` — 있으면 기본값을 덮어쓴다 (선택).

## 알려진 제약
- CPU 온도는 Windows 사용자 모드에서 직접 읽을 수 없다 → LibreHardwareMonitor 를 실행(웹서버 8085 켬)해야 표시된다.
- Spotify 재생 제어는 Premium 계정만. 푸시 API 가 없어 5초 폴링.
- 반투명은 기본적으로 카드 표면의 알파값(`--surface-alpha`)일 뿐이다.
- **배경화면 블러는 기본 켜짐**(`blurStrength: 3`). `providers/wallpaper` 가 블러한 스냅샷을
  만들고, 각 카드가 자기 위치만큼 밀어 깐다 (`WidgetFrame.css` 의 `.widget::before`).
  얻는 방법이 둘이다:
  - **캡처**(기본): 아이콘을 품은 `Progman`/`WorkerW` 를 `PrintWindow(PW_RENDERFULLCONTENT)`.
    Wallpaper Engine 같은 라이브 배경화면은 **파일을 바꾸지 않고** 이 창에 직접 그리므로
    `SPI_GETDESKWALLPAPER` 로는 엉뚱한 옛 이미지가 나온다. 캡처하면 배치 계산도 필요 없다.
  - **파일**(폴백, 30초): 캡처가 비어 있으면(`looks_blank`) 배경화면 파일을 읽고
    `WallpaperStyle` 레지스트리에 맞춰 채우기/맞춤/늘이기/가운데/바둑판을 계산한다.
  `Wallpaper.source` 가 어느 쪽인지 알려주고 설정 패널이 표시한다.
  처음 구현은 상시 CPU 를 0.94% → 6.35% 로 올려 기본이 꺼짐이었다. 기본으로 켤 수 있게 된 것은
  셋을 고친 뒤다 (`providers/wallpaper/mod.rs` 머리말에 자세히):
  **① GDI 안에서 자르고 줄여 받는다**(`capture_monitor` 의 `StretchBlt` — CPU 로 넘어오는
  픽셀이 200만 → 6만), **② 지문이 같으면 블러도 인코딩도 emit 도 건너뛴다**,
  **③ 주기를 결과에 맞춘다**(움직이면 2초, 3번 연속 그대로면 10초),
  **④ 카드가 안 보이면 아예 찍지 않는다**(`dashboard_visible` — 창이 숨겨졌거나 전경 창이
  모니터를 통째로 덮은 전체화면 게임·영상. 남은 비용인 `PrintWindow` 까지 0 이 된다).
  고친 뒤 실측(release, 1920x1080, 60초씩): **끔 1.46% vs 켬 1.95%** — 블러가 더하는 비용이
  5.4%p 에서 0.5%p 로 줄어 기본값을 켜짐으로 돌렸다.
  끄면 `wallpaper_set_active(false)` 로 캡처 루프까지 멈춘다.
  **남은 비용은 `PrintWindow` 자체다** — 바탕화면을 다시 그리게 하는 일이라 피할 수 없다.
