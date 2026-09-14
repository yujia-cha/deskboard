# deskboard

Windows 바탕화면 위젯 대시보드. 시계 · 캘린더 · Claude Code 사용량 · CPU/GPU/RAM · Spotify.

- 완전 투명한 창이 모니터 작업영역 전체를 덮고 항상 바탕화면 바로 위(다른 창 뒤)에 머문다. 설정에서 모니터 선택.
- 반투명(Acrylic) ↔ 단색 전환, 둥근 카드, 강조색 변경.
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
| Claude 한도 | 위젯의 [브라우저로 로그인] → claude.ai 승인 → 표시된 코드를 붙여넣기. 5시간 / 주간 한도 % 를 60초마다, 그리고 Claude Code 사용 직후 즉시 동기화. (Claude Code 와 같은 OAuth 클라이언트를 쓰는 비공식 방식, 토큰은 `%APPDATA%\com.user.deskboard\claude.json`) |
| Spotify | [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard) 에서 앱 생성 → Redirect URI `http://127.0.0.1:8888/callback` 추가 → Client ID 를 위젯에 입력. 재생 제어는 Premium 계정만 |

## 트레이 메뉴
편집 잠금/해제 · 반투명/단색 전환 · 설정 · 표시/숨기기 · 종료
