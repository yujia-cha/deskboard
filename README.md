# deskboard

Windows 바탕화면 위젯 대시보드. 위젯 12종.

시계(스타일 8종) · 캘린더 · 날씨 · 할 일 · 바로가기 폴더 ·
Claude Code 사용량 · CPU/GPU/RAM/네트워크/프로세스 · Spotify ·
플레이타임(게임 + 등록한 프로그램) · git 저장소 · GitHub.

시계는 8가지 — 사각 블록 · 네온 · LED 도트매트릭스 · 문자 도트 · 시스템 폰트 ·
워드 클락(영어/한국어) · 바이너리 · 피보나치.

- 완전 투명한 창이 모니터 작업영역 전체를 덮고 항상 바탕화면 바로 위(다른 창 뒤)에 머문다. 설정에서 모니터 선택.
- 카드 불투명도·모서리 반경을 설정에서 조절.
- 카드 뒤 **배경화면 블러는 실험적 기능이고 기본은 꺼짐**이다. 켜면 화면을 10초마다 캡처해
  블러를 깔고(Wallpaper Engine 같은 라이브 배경화면도 반영) 상시 CPU 가 약 5%p 오른다.
  실시간 갱신을 싸게 만들기 전까지는 꺼둔 채로 쓰는 것을 권한다. 꺼져 있으면 캡처도 하지 않는다.
- 반투명 ↔ 단색 전환, 다크/라이트 팔레트(시스템 테마 추종 가능), 카드 스타일 4종, 강조색 변경.
- Win+D(바탕화면 보기)를 눌러도 대시보드는 숨지 않는다.
- 위젯을 자유롭게 배치/크기 조절 (트레이 메뉴 → 편집 잠금/해제, `Esc` 로 잠금). 위젯 설정에서 크기 프리셋/px 지정, 내용은 크기에 맞춰 자동 확대·축소.
- 위젯이 없는 빈 영역은 클릭이 바탕화면으로 통과한다 (아이콘 클릭 가능).
- 새 위젯은 폴더 하나로 추가: [docs/ADDING_A_WIDGET.md](docs/ADDING_A_WIDGET.md)

## 실행

```powershell
npm install
npm run tauri dev
```

설치 파일 만들기: `npm run tauri build` → `src-tauri/target/release/bundle/nsis/*.exe`

## 위젯별 준비물

| 위젯 | 필요한 것 |
|---|---|
| 시계 · 캘린더 | 없음 |
| 시스템 | GPU: `nvidia-smi`(NVIDIA 드라이버). CPU 온도: [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) 실행 + Options → Remote Web Server 켜기 (포트 8085) |
| Claude 한도 | 위젯의 [브라우저로 로그인] → claude.ai 승인 → 표시된 코드를 붙여넣기. 5시간 / 주간 한도 % 를 2분마다, 그리고 Claude Code 사용 직후 즉시 동기화. (Claude Code 와 같은 OAuth 클라이언트를 쓰는 비공식 방식, 토큰은 `%APPDATA%\com.user.deskboard\claude.json`) |
| 날씨 | 없음 — [Open-Meteo](https://open-meteo.com) 는 가입도 API 키도 필요 없습니다. 위젯에서 도시를 검색하거나 위도·경도를 직접 넣으세요 |
| 할 일 | 없음 |
| 플레이타임 | 없음. 앞에 떠 있는 창의 **실행 파일 이름만** 기록합니다 — 창 제목은 저장하지 않습니다. 3분 이상 입력이 없으면 세지 않습니다. 기록은 `%APPDATA%\com.user.deskboardctivity.sqlite`, 120일 뒤 자동 삭제 |
| git 저장소 | 없음. 저장소가 모여 있는 폴더를 지정하면 그 아래 2단계까지 찾습니다. `fetch` 하지 않으므로 ahead/behind 는 마지막으로 받아온 시점 기준입니다 |
| GitHub | **classic** Personal Access Token — 위젯의 [토큰 만들기] 를 누르면 필요한 스코프(`repo`, `notifications`)가 선택된 채로 열립니다. fine-grained 토큰은 리뷰 요청 PR 조회(`/search/issues`)에서 403 이 납니다. 토큰은 Windows DPAPI 로 암호화해 저장하며 이 PC 의 이 계정에서만 풀립니다 |
| Spotify | [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard) 에서 앱 생성 → Redirect URI `http://127.0.0.1:8888/callback` 추가 → Client ID 를 위젯에 입력. 재생 제어는 Premium 계정만 |

## 자동 시작
설치본은 Windows 로그인 시 자동으로 실행됩니다(기본 켜짐). 설정 → "Windows 시작 시 실행" 에서 끌 수 있고, 그 아래에 실제 등록 경로가 표시됩니다.
작업 관리자 → 시작 앱에서 deskboard 가 "사용 안 함" 이면 Windows 가 실행을 막으므로 거기서도 켜져 있어야 합니다.
이미 실행 중일 때 다시 실행하면 새 창을 만들지 않고 기존 대시보드를 보여줍니다.

## 트레이 메뉴
편집 잠금/해제 · 반투명/단색 전환 · 설정 · 표시/숨기기 · 종료
