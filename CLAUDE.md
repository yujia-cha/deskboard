# Project: deskboard

Windows 바탕화면에 상주하는 개인 위젯 대시보드 (Tauri 2 + React 19 + TypeScript + Rust).
프레임 없는 투명 창(always-on-bottom)에 둥근 카드 위젯을 자유 배치한다.

## 기본 위젯
| id | 백엔드 Provider | 데이터 |
|---|---|---|
| `clock` | 없음 | 순수 프론트 |
| `sysmon` | `providers/sysmon` | sysinfo(CPU/RAM) + nvidia-smi(GPU) + LibreHardwareMonitor(CPU 온도, 선택) |
| `claude-usage` | `providers/claude_usage` | **구독 한도 %** (5시간 / 주간 전체모델): 위젯 내 OAuth 로그인(`auth.rs`, Claude Code 와 같은 PKCE 흐름·클라이언트 id, 코드 붙여넣기) → 토큰은 `%APPDATA%/com.user.deskboard/claude.json` → `api.anthropic.com/api/oauth/usage` 120초 폴링(429 시 5분 백오프) + 트랜스크립트 변경 시 즉시. Claude Code 의 credentials 파일은 건드리지 않음. 로컬 비용 추정(`*.jsonl` 파싱, `resources/pricing.json`)은 옵션 |
| `calendar` | `providers/calendar` | SQLite (`%APPDATA%/com.user.deskboard/calendar.sqlite`) |
| `spotify` | `providers/spotify` | PKCE 로그인, Web API 5초 폴링(위젯 표시 중에만) |
| `settings` | 없음 | 톱니바퀴 아이콘만 있는 정사각형(56×56). 클릭 = 편집 모드(잠금) 토글. 설정 패널 자체는 편집 모드의 편집 바 "⚙ 설정" 버튼이나 위젯 오버레이 ⚙ 로 연다 → `singleton` (항상 1개, 제거 불가, 로드 시 없으면 자동 추가·정사각형이 아니면 기본 크기로 보정) |
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
├─ window.rs     Acrylic 반투명/단색 전환, 트레이 메뉴
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
- 자동 시작: `settings.ts::load` 에서 `autostart`(기본 true) 에 따라 plugin-autostart enable/disable. 개발 실행(`import.meta.env.DEV`)에서는 등록하지 않는다.
- 단일 인스턴스: `tauri-plugin-single-instance` — 두 번째 실행은 기존 창 `show()` 후 종료.
- 트레이 종료: `ui://quit` 이벤트로 프론트가 설정을 즉시 저장한 뒤 400ms 후 `app.exit`.

## 창 동작
- 창은 항상 완전 투명(OS 블러 없음). 카드만 CSS `--surface` 로 그린다. 반투명/단색은 CSS 변수 전환.
- 창은 선택 모니터의 작업영역(`GetMonitorInfoW.rcWork`) 전체를 덮는다 (`window.rs::fit_to_work_area`, 2초마다 변화 감지). 위젯 좌표 = 작업영역 좌표.
- `alwaysOnBottom` — 다른 앱 뒤, 바탕화면 위.
- 히트 영역 click-through (`window.rs::start_hit_test`): 잠금 상태에서 커서가 위젯 사각형 밖이면 `set_ignore_cursor_events(true)` → 빈 영역 클릭이 바탕화면 아이콘으로 통과. 프론트가 `set_hit_regions` 로 사각형을 보낸다 (편집 모드·설정 패널 열림 = 비활성).
- 팝업·메뉴 닫기는 `core/dismiss.ts::useDismiss` (창 안 바깥 클릭 + 히트 영역 밖 클릭 `hit://outside-press` + Esc). 창 `blur` 는 always-on-bottom 창에서 클릭 직후에도 발생하므로 쓰지 않는다. 위젯 밖으로 튀어나오는 팝업은 `setOverlayRect` 로 히트 영역 등록.
- 내용 자동 맞춤: zoom = `contentScale()`(기본 크기 대비 비율, autoScale 꺼지면 1) × 넘침 보정(`WidgetFrame` 이 body 의 scroll/client 비율을 재서 넘치면 축소). 내용이 잘리지 않음을 보장. 위젯은 배율을 뺀 `size` 를 받는다.

## 규칙
- 색상은 CSS 변수만 (`--accent --text --text-dim --surface --surface-strong --border --ok --warn --danger`). 하드코딩 금지.
- 위젯은 서로 import 하지 않는다. `core/`, `components/` 만.
- Provider 는 `start()` 에서 즉시 반환하고 백그라운드에서 `app.emit("<id>://update", ..)`.
- 외부 자원(OS 센서, 파일, API)은 트레이트로 추상화 (`SensorSource`, `CalendarSource`), 테스트는 Fake/픽스처로.
- 커맨드 오류는 `Result<_, String>` 사용자 메시지.
- 편집 모드(잠금 해제)는 저장하지 않는다. 시작 시 항상 잠김.
- 설정 저장 위치: `%APPDATA%/com.user.deskboard/settings.json` (레이아웃·테마·모니터), `spotify.json` / `claude.json` (토큰 — 평문), `pricing.json` (가격 덮어쓰기, 선택).

## 알려진 제약
- CPU 온도는 Windows 사용자 모드에서 직접 읽을 수 없다 → LibreHardwareMonitor 를 실행(웹서버 8085 켬)해야 표시된다.
- Spotify 재생 제어는 Premium 계정만. 푸시 API 가 없어 5초 폴링.
- 반투명(Acrylic)은 Windows 11 에서만 블러가 적용된다. Win10 은 단색으로 보인다.
