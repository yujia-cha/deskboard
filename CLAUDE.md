# Project: deskboard

Windows 바탕화면에 상주하는 개인 위젯 대시보드 (Tauri 2 + React 19 + TypeScript + Rust).
프레임 없는 투명 창(always-on-bottom)에 둥근 카드 위젯을 자유 배치한다.

## 기본 위젯
| id | 백엔드 Provider | 데이터 |
|---|---|---|
| `clock` | 없음 | 순수 프론트. 스타일 8종 — 격자 숫자 4종(블록·네온·LED매트릭스·문자도트, `DotDigits` 공용) · 시스템 폰트 · 워드 클락(`words.ts`, en/ko) · 바이너리 · 피보나치(`oddtime.ts`) |
| `sysmon` | `providers/sysmon` | sysinfo(CPU/RAM/네트워크) + nvidia-smi(GPU) + LibreHardwareMonitor(CPU 온도, 선택). CPU 상위 프로세스는 위젯이 켤 때만 수집(`sysmon_set_detail`) — 목록 갱신이 비싸다 |
| `claude-usage` | `providers/claude_usage` | **구독 한도 %** (5시간 / 주간 전체모델): 위젯 내 OAuth 로그인(`auth.rs`, Claude Code 와 같은 PKCE 흐름·클라이언트 id, 코드 붙여넣기) → 토큰은 `%APPDATA%/com.user.deskboard/claude.json` → `api.anthropic.com/api/oauth/usage` 120초 폴링(429 시 5분 백오프) + 트랜스크립트 변경 시 즉시. Claude Code 의 credentials 파일은 건드리지 않음. 로컬 비용 추정(`*.jsonl` 파싱, `resources/pricing.json`)은 옵션 |
| `calendar` | `providers/calendar` | SQLite (`%APPDATA%/com.user.deskboard/calendar.sqlite`) |
| `spotify` | `providers/spotify` | PKCE 로그인, Web API 5초 폴링(위젯 표시 중에만) |
| `settings` | 없음 | 톱니바퀴 아이콘만 있는 정사각형(56×56). 클릭 = 편집 모드(잠금) 토글. 설정 패널 자체는 편집 모드의 편집 바 "⚙ 설정" 버튼이나 위젯 오버레이 ⚙ 로 연다 → `singleton` (항상 1개, 제거 불가, 로드 시 없으면 자동 추가·정사각형이 아니면 기본 크기로 보정) |
| `wallpaper` | `providers/wallpaper` | 위젯 아님 — 배경화면을 읽어 블러한 스냅샷을 `wallpaper://update` 로 푸시 (카드 뒤 '진짜 반투명' 재료) |
| `weather` | `providers/weather` | [Open-Meteo](https://open-meteo.com) — **키·가입 불필요**. 위젯이 떠 있을 때만 15분 폴링, 도시 검색은 `weather_search`(지오코딩, 역시 키 불필요) |
| `notes` | `providers/notes` | 메모·할 일. SQLite (`notes.sqlite`). 목록은 **위젯 인스턴스마다 독립** — 여러 개 띄워 용도별로 나눠 쓴다 |
| `playtime` | `providers/activity` | `playtime` 은 게임(분류 규칙)을 자동으로 세고, 그 밖의 프로그램은 설정의 `extra` 에 등록한 것만 더해 도넛으로 보여준다 — "게임만"과 "앱 전부"로 위젯을 나누면 같은 데이터를 두 번 보게 된다. 앞에 떠 있는 창의 실행 파일 이름만 5초마다 확인해 `activity.sqlite` 에 누적 — **창 제목은 저장하지 않는다.** 입력이 3분 없으면 자리비움으로 보고 세지 않고, 대시보드 자신도 세지 않는다. 분류는 `resources/activity-rules.json` + 사용자 규칙(`rules` 테이블이 우선) |
| `gitstatus` | `providers/git` | 등록한 폴더 아래 2단계까지 `.git` 탐색 → 브랜치·미커밋 수·ahead/behind. `git2` crate (repo 마다 프로세스를 띄우지 않으려고). **fetch 하지 않는다** — ahead/behind 는 마지막으로 받아온 시점 기준 |
| `github` | `providers/github` | 리뷰 요청 PR·미확인 알림·설정한 저장소의 최신 CI. PAT 는 **DPAPI 로 암호화**해 `github.dat` 에 (`providers/secrets`) |
| `folder` | `providers/folders` | 디스코드식 바로가기 폴더. 인스턴스당 실제 디렉터리 1개(`%APPDATA%/com.user.deskboard/folders/<instanceId>` 기본) — 드롭 시 이동/복사, `IShellItemImageFactory` 로 아이콘 추출, `notify` 로 변경 감지 |

새 위젯: [docs/ADDING_A_WIDGET.md](docs/ADDING_A_WIDGET.md) — 폴더 하나 + `registry.ts` 한 줄 (+ Provider).

## 구조
```
src/                      React 프론트
├─ core/    theme.css(토큰) · settings.ts(zustand + plugin-store 영속) · ipc.ts(이벤트/커맨드 훅)
├─ components/  WidgetFrame(카드·이동·크기조절) · Canvas · SettingsPanel(스키마 자동 폼) · Gauge · Sparkline
└─ widgets/  registry.ts · types.ts · <id>/
src-tauri/src/
├─ lib.rs        플러그인·프로바이더·커맨드 등록
├─ window.rs     트레이 메뉴, 작업영역 맞춤, 히트영역 click-through, Win+D 제외
└─ providers/    mod.rs(Provider 트레이트 + all()) · <id>/
```

## 명령
```bash
npm run tauri dev      # 개발 실행 (Rust 변경 시 자동 재시작, 프론트는 HMR)
npm run typecheck      # tsc
npm test               # vitest
cd src-tauri && cargo test
npm run tauri build    # NSIS 설치 파일 → src-tauri/target/release/bundle/nsis/
```

## 시작/종료
- 자동 시작: `autostart.rs::sync` 가 시작 시 `settings.json` 의 `v1.autostart`(기본 true) 에 맞춰 HKCU Run 값을 enable/disable (enable 은 현재 exe 경로로 덮어써 교정). **debug 빌드는 절대 등록하지 않고**, Run 값이 `\target\debug|release\` 를 가리키면 지운다. 설정 토글은 `settings.ts::setAutostart`.
- 단일 인스턴스: `tauri-plugin-single-instance` — release 빌드에서만. 두 번째 실행은 기존 창 `show()` 후 종료.
- 트레이 종료: `ui://quit` 이벤트로 프론트가 설정을 즉시 저장한 뒤 400ms 후 `app.exit`.

## 창 동작
- 창은 항상 완전 투명(OS 블러 없음). 카드만 CSS `--surface` 로 그린다.
  모양은 `<html>` 의 세 속성이 결정한다 (`settings.ts::applyTheme`):
  `data-theme-mode`(translucent|solid) · `data-palette`(dark|light, 설정의 `auto` 는 OS 테마 추종) ·
  `data-card-style`(glass|minimal|borderless|none).
  카드 불투명도·블러 강도·모서리 반경은 설정 슬라이더 → `--surface-alpha`/`--radius`.
- 창은 선택 모니터의 작업영역(`GetMonitorInfoW.rcWork`) 전체를 덮는다 (`window.rs::fit_to_work_area`, 2초마다 변화 감지). 위젯 좌표 = 작업영역 좌표.
- **z-order 는 불변식 하나로 관리한다 — 바탕화면 바로 위, 나머지 앱 아래**
  (`window.rs::enforce_z_order`). Win+D 가 Progman 을 끌어올리면 다시 그 위로 돌아간다.
  창은 평범한 최상위 창이라 DWM 알파 합성·포커스·IME 가 모두 정상이다.
  - **`SetWindowPos` 의 두 번째 인자를 조심할 것.** `hWndInsertAfter` 는 "positioned window
    **앞에 올** 창"이다 — 우리는 그 창 **바로 아래**로 간다. 그래서 `SetWindowPos(me, 바탕화면)`
    은 바탕화면 *밑에* 묻는다. 이걸 "바로 위로 올린다"고 착각한 채 오래 썼고, **올리는 쪽이
    한 번도 작동하지 않은 진짜 이유**가 이것이었다. 바로 위로 가려면
    `GetWindow(바탕화면, GW_HWNDPREV)` 를 대상으로 준다.
  - 훅(`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`)은 **신호로만** 쓴다. 셸은 전경을 먼저 바꾸고
    **그 다음에** Progman 을 올리므로(실측: 훅이 뜬 시점에 `desktop_above` 는 아직 false),
    훅에서 한 번 고쳐 봐야 곧 덮인다. 훅은 "지켜보라"는 표시만 세우고 루프가 매 틱 교정한다.
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
- 히트 영역 click-through (`window.rs::start_hit_test`): 잠금 상태에서 커서가 위젯 사각형 밖이면 `set_ignore_cursor_events(true)` → 빈 영역 클릭이 바탕화면 아이콘으로 통과. 프론트가 `set_hit_regions` 로 사각형을 보낸다 (편집 모드·설정 패널 열림 = 비활성).
- 팝업·메뉴 닫기는 `core/dismiss.ts::useDismiss` (창 안 바깥 클릭 + 히트 영역 밖 클릭 `hit://outside-press` + Esc). 창 `blur` 는 always-on-bottom 창에서 클릭 직후에도 발생하므로 쓰지 않는다. 위젯 밖으로 튀어나오는 팝업은 `setOverlayRect` 로 히트 영역 등록.
- 내용 자동 맞춤: zoom = `contentScale()`(기본 크기 대비 비율, autoScale 꺼지면 1) × 넘침 보정(`WidgetFrame` 이 body 의 scroll/client 비율을 재서 넘치면 축소). 내용이 잘리지 않음을 보장. 위젯은 배율을 뺀 `size` 를 받는다.

## 규칙
- 색상·타이포는 `core/theme.css` 의 CSS 변수만 쓴다. 하드코딩 금지.
  - 색: `--accent --text --text-dim --surface --surface-strong --border --ok --warn --danger`
    `--fill/--fill-hover/--fill-active`(표면 위 채움) `--track`(빈 진행바) `--shadow-card/--shadow-pop` `--scrim`
    `--on-light`(밝은 브랜드색 위 글자, 고정) `--on-text`(`--text` 를 배경으로 깔았을 때의 글자색, 팔레트마다 뒤집힘)
  - 타이포: `--fs-value/-title/-sub/-label/-meta/-micro`, `--fw-value/-title/-bold/-label`
  - 모션: `--ease --dur --dur-fast` (`prefers-reduced-motion` 은 theme.css 가 전역으로 처리)
- 위젯은 서로 import 하지 않는다. `core/`, `components/` 만.
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
  - `spotify.json` / `claude.json` — 토큰이 **평문**이다. 새로 쓰는 비밀값은 `providers/secrets`
    (Windows DPAPI, 현재 계정으로만 복호화)를 거친다: `github.dat`. 기존 둘의 이전은 남은 일.
  - `pricing.json` / `activity-rules.json` — 있으면 기본값을 덮어쓴다 (선택).

## 알려진 제약
- CPU 온도는 Windows 사용자 모드에서 직접 읽을 수 없다 → LibreHardwareMonitor 를 실행(웹서버 8085 켬)해야 표시된다.
- Spotify 재생 제어는 Premium 계정만. 푸시 API 가 없어 5초 폴링.
- 반투명은 기본적으로 카드 표면의 알파값(`--surface-alpha`)일 뿐이다.
- **배경화면 블러는 실험적이고 기본 꺼짐**(`blurStrength: 0`). 켜면
  `providers/wallpaper` 가 블러한 스냅샷을 만들고, 각 카드가 자기 위치만큼 밀어 깐다
  (`WidgetFrame.css` 의 `.widget::before`). 얻는 방법이 둘이다:
  - **캡처**(기본, 10초): 아이콘을 품은 `Progman`/`WorkerW` 를 `PrintWindow(PW_RENDERFULLCONTENT)`.
    Wallpaper Engine 같은 라이브 배경화면은 **파일을 바꾸지 않고** 이 창에 직접 그리므로
    `SPI_GETDESKWALLPAPER` 로는 엉뚱한 옛 이미지가 나온다. 캡처하면 배치 계산도 필요 없다.
  - **파일**(폴백, 30초): 캡처가 비어 있으면(`looks_blank`) 배경화면 파일을 읽고
    `WallpaperStyle` 레지스트리에 맞춰 채우기/맞춤/늘이기/가운데/바둑판을 계산한다.
  `Wallpaper.source` 가 어느 쪽인지 알려주고 설정 패널이 표시한다.
  꺼져 있으면 `wallpaper_set_active(false)` 로 캡처 루프까지 멈춘다 — 실측 상시 CPU
  0.94%(끔) vs 6.35%(켬, 1코어 기준). 싸게 만드는 방법은 `providers/wallpaper/mod.rs` 머리말 참고.
