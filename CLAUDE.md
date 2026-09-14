# Project: deskboard

Windows 바탕화면에 상주하는 개인 위젯 대시보드 (Tauri 2 + React 19 + TypeScript + Rust).
프레임 없는 투명 창(always-on-bottom)에 둥근 카드 위젯을 자유 배치한다.

## 기본 위젯
| id | 백엔드 Provider | 데이터 |
|---|---|---|
| `clock` | 없음 | 순수 프론트 |
| `sysmon` | `providers/sysmon` | sysinfo(CPU/RAM) + nvidia-smi(GPU) + LibreHardwareMonitor(CPU 온도, 선택) |
| `claude-usage` | `providers/claude_usage` | `~/.claude/projects/**/*.jsonl` 증분 파싱 + notify 감시, 가격표 `resources/pricing.json` |
| `calendar` | `providers/calendar` | SQLite (`%APPDATA%/com.user.deskboard/calendar.sqlite`) |
| `spotify` | `providers/spotify` | PKCE 로그인, Web API 5초 폴링(위젯 표시 중에만) |

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

## 규칙
- 색상은 CSS 변수만 (`--accent --text --text-dim --surface --surface-strong --border --ok --warn --danger`). 하드코딩 금지.
- 위젯은 서로 import 하지 않는다. `core/`, `components/` 만.
- Provider 는 `start()` 에서 즉시 반환하고 백그라운드에서 `app.emit("<id>://update", ..)`.
- 외부 자원(OS 센서, 파일, API)은 트레이트로 추상화 (`SensorSource`, `CalendarSource`), 테스트는 Fake/픽스처로.
- 커맨드 오류는 `Result<_, String>` 사용자 메시지.
- 편집 모드(잠금 해제)는 저장하지 않는다. 시작 시 항상 잠김.
- 설정 저장 위치: `%APPDATA%/com.user.deskboard/settings.json` (레이아웃·테마), `spotify.json` (토큰 — 평문), `pricing.json` (가격 덮어쓰기, 선택).

## 알려진 제약
- CPU 온도는 Windows 사용자 모드에서 직접 읽을 수 없다 → LibreHardwareMonitor 를 실행(웹서버 8085 켬)해야 표시된다.
- Spotify 재생 제어는 Premium 계정만. 푸시 API 가 없어 5초 폴링.
- 반투명(Acrylic)은 Windows 11 에서만 블러가 적용된다. Win10 은 단색으로 보인다.
