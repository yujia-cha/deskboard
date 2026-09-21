# 배포 절차

**저장소에는 소스만 올린다.** 설치 파일은 GitHub Actions 가 Windows 러너에서 만들어
릴리스에 붙이고, 이미 깔린 앱은 그 릴리스를 보고 스스로 업데이트한다.

```
태그 v0.3.0 푸시
      ↓
.github/workflows/release.yml  (windows-latest)
  typecheck · vitest · cargo test → tauri build → 서명
      ↓
GitHub 릴리스 v0.3.0
  deskboard_0.3.0_x64-setup.exe  +  .sig  +  latest.json
      ↓
설치된 앱: plugins.updater.endpoints → .../releases/latest/download/latest.json
  새 버전이면 설정 → 업데이트에 뜬다 (시작 8초 뒤 한 번 자동 확인)
```

## 처음 한 번: 저장소와 시크릿

```bash
gh repo create yujia-cha/deskboard --public --source=. --remote=origin --push
```

(이미 원격이 있으면 `git push -u origin main` 만.)

**public 이어야 한다.** 앱은 `releases/latest/download/latest.json` 을 **인증 없이** 받는데,
private 저장소에서는 그 URL 이 404 다. 앱에 PAT 를 넣어 풀 수도 있지만 그건 토큰을 배포본에
같이 뿌리는 것과 같다. 소스를 비공개로 두고 싶으면 릴리스만 올릴 public 저장소를 따로 두고
`release.yml` 이 그쪽에 릴리스를 만들게 해야 한다.

저장소에 비밀값은 들어가지 않는다 — 서명 개인 키는 `%USERPROFILE%\.tauri\` 에,
GitHub PAT 는 DPAPI 로 `github.dat` 에, Spotify·Claude 토큰은 `%APPDATA%` 에 있다.

그다음 **Settings → Secrets and variables → Actions** 에 둘을 넣는다.
이게 없으면 워크플로는 돌지만 서명이 없어 자동 업데이트가 동작하지 않는다.

| 시크릿 | 값 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | `%USERPROFILE%\.tauri\deskboard.key` 파일 **내용 전체** |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 키에 비밀번호를 걸었을 때만. **안 걸었으면 만들지 않는다** |

지금 키는 비밀번호 없이 만들었으므로 **첫 번째 하나만** 넣으면 된다. GitHub 은 빈 시크릿을
허용하지 않는데, 없는 시크릿은 워크플로에서 빈 문자열로 평가되어 환경 변수가 빈 값으로
전달된다 — 비밀번호 없는 키에 필요한 것이 정확히 그 값이다.

```powershell
Get-Content $env:USERPROFILE\.tauri\deskboard.key -Raw | Set-Clipboard
```

붙여 넣은 값이 **`dW50cnVzdGVk` 로 시작하는 한 줄**인지 눈으로 확인한다. 키 파일은 348바이트
한 줄짜리 base64 다(줄바꿈·따옴표·공백이 섞이면 안 된다). 워크플로의 "서명 키 시크릿 점검"
스텝이 같은 것을 빌드 **전에** 보고 몇 초 만에 멈춰 준다 — 예전에는 20분을 컴파일한 뒤
마지막 서명 단계에서 `failed to decode base64 secret key` 로 죽었다.

- 공개 키는 `src-tauri/tauri.conf.json` 의 `plugins.updater.pubkey` 에 이미 들어 있다.
- **개인 키를 잃으면 자동 업데이트가 끊긴다.** 새 키로 바꾸면 이미 깔린 앱들은 새 릴리스를
  거부하므로, 사용자가 설치 파일을 직접 받아 한 번 덮어써야 한다. 키 파일은 따로 백업해 둔다.
- 개인 키는 저장소에 올리지 않는다 (`.gitignore` 와 무관하게 `%USERPROFILE%` 에 있다).

## 릴리스 한 번

### 1. 버전

세 곳이 **같아야** 한다 — 다르면 설치 파일 이름과 제어판 표시가 어긋나고,
업데이터가 "새 버전"을 잘못 판단한다.

| 파일 | 키 |
|---|---|
| `package.json` | `version` |
| `src-tauri/Cargo.toml` | `[package] version` |
| `src-tauri/tauri.conf.json` | `version` |

`Cargo.toml` 을 고쳤으면 `cargo check` 를 한 번 돌려 `Cargo.lock` 까지 같이 갱신한다.

### 2. 로컬 검사

```bash
npm run typecheck
npm test
cd src-tauri && cargo test && cargo clippy --all-targets
```

`clippy` 는 아직 경고가 남아 있다(새 버전에서 생긴 스타일 린트 — `map_or` 단순화 등).
**새로 늘지 않았는지만** 본다. `-D warnings` 로 세우는 건 그것들을 정리한 뒤에.

### 3. 태그를 민다

```bash
git tag v0.3.0 && git push origin v0.3.0
```

Actions 가 끝나면 릴리스에 `deskboard_0.3.0_x64-setup.exe` · `.sig` · `latest.json` 이 붙는다.
실패하면 릴리스가 만들어지지 않으므로 사용자에게 반쪽짜리가 나가지 않는다.

태그를 다시 만들 때는 (`git tag -d` → 다시 푸시) **릴리스도 함께 지워야** 한다 —
같은 태그의 릴리스가 남아 있으면 action 이 거기에 덧붙인다.

### 4. 로컬에서만 만들고 싶을 때

```bash
npm run tauri build
```

결과: `src-tauri/target/release/bundle/nsis/deskboard_<버전>_x64-setup.exe`

서명까지 하려면 환경 변수를 준다 (안 주면 `.sig` 가 없어 업데이트용으로는 못 쓴다):

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $env:USERPROFILE\.tauri\deskboard.key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
npm run tauri build
```

- **덮어 설치**는 `src-tauri/nsis-hooks.nsh` 가 `taskkill /F /T /IM deskboard.exe` 를 돌린다.
  이게 없으면 상주 중인 앱이 파일을 잠그고 있어 옛 버전 제거가 실패한다
  ("기존 버전을 제거할 수 없습니다"). 자동 시작 때문에 재부팅한 PC 에서는 거의 항상 걸린다.

  **주의 — 0.3.0 을 처음 덮어 설치할 때는 아직 안 통한다.** 덮어 설치는 새 설치본이
  **옛 버전의 언인스톨러를 먼저 실행**하는 구조라(생성된 `installer.nsi` 의 재설치 페이지),
  실제로 앱을 끝내 주는 것은 `NSIS_HOOK_PREUNINSTALL` — 즉 **이미 설치돼 있던 쪽**의 훅이다.
  0.2.0 의 언인스톨러에는 그게 없다. 그래서 0.2.0 → 0.3.0 한 번만 트레이에서 종료한 뒤
  설치하고, **0.3.0 이후부터는 앱이 떠 있어도 그냥 덮어 설치된다.**
- `installMode: currentUser` — 관리자 권한 없이 계정에만 설치된다.
  자동 시작이 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 을 쓰므로 이쪽이 맞다.
- 업데이트 설치는 `installMode: passive` — 진행 막대만 뜨고 사용자가 누를 것이 없다.
- 코드 서명(SmartScreen)은 하지 않는다 → 처음 실행 시 경고. [추가 정보] → [실행].
  업데이터의 서명은 이것과 다른 것이다 (배포본이 우리 것인지 앱이 확인하는 용도).

## 새 PC 에서의 점검 목록

설치 직후 **로그인·설정 없이** 확인할 것 (외부 계정이 없어도 깨지지 않아야 한다):

- [ ] 첫 실행에 기본 배치가 뜨고, 바탕화면 아이콘을 가리지 않으며 클릭이 통과한다
- [ ] `Win+D` 를 눌러도 대시보드가 남는다 (여러 번)
- [ ] 트레이 메뉴 — 편집 잠금/해제 · 설정 · 표시/숨기기 · 종료
- [ ] 편집 모드: 헤더 드래그, 빈 곳 드래그로 여러 개 선택 → 함께 이동, `Esc` 로 선택 해제 → 잠금
- [ ] 편집 모드에서 **네 변·네 모서리 어디를 끌어도** 크기가 바뀐다 (왼쪽/위는 반대편이 고정)
- [ ] 배경 블러가 켜진 채(기본값) 카드 뒤에 배경화면이 비치고, 유휴 CPU 가 눈에 띄게 오르지 않는다
- [ ] 시계를 한 번 누른 뒤 다른 앱에서 한/영 전환이 된다 (IME 포커스 반환)
- [ ] 로그아웃 상태의 위젯(Spotify·GitHub·Claude)이 **로그인 안내만** 보이고 오류로 도배되지 않는다
- [ ] 네트워크를 끊어도 위젯이 "연결하지 못했습니다" 수준에서 멈추고 앱이 죽지 않는다
- [ ] 재부팅 후 자동 시작 (설정 → "Windows 시작 시 실행" 에 표시되는 등록 경로가 설치 경로다)
- [ ] 설정 → 업데이트에 현재 버전이 뜨고, [업데이트 확인] 이 오류 없이 "최신" 이라고 답한다
- [ ] **앱이 켜져 있는 상태에서** 설치 파일을 다시 실행해도 "기존 버전을 제거할 수 없습니다" 가
      뜨지 않고 덮어 설치된다 (NSIS 훅)
- [ ] 제거 후 `%APPDATA%\com.user.deskboard\` 가 남는지 확인 (설정은 일부러 남긴다)

환경이 다른 PC 에서 특히 볼 것:

- [ ] **디스플레이 배율 125%/150%** — 위젯이 잘리거나 좌표가 밀리지 않는지
- [ ] **다중 모니터** — 설정에서 모니터를 바꾸면 그 모니터의 작업영역을 덮는지
- [ ] **NVIDIA GPU 가 없는 PC** — 시스템 위젯이 GPU 칸만 비우고 나머지는 정상인지
- [ ] **작업표시줄 위치/자동 숨김** 변경 시 작업영역을 다시 맞추는지 (2초 안)

## 업데이트가 안 올 때

| 증상 | 볼 곳 |
|---|---|
| "최신" 이라고만 나온다 | 릴리스가 draft 인지, 태그가 `v` 로 시작하는지, 버전 세 곳이 같은지 |
| 받다가 실패한다 | `latest.json` 의 서명이 지금 `pubkey` 로 풀리는지 (키를 바꿨나) |
| 개발 실행에서 안 보인다 | 정상이다 — `lib.rs` 가 release 빌드에서만 플러그인을 건다 |
| 설치가 "기존 버전을 제거할 수 없습니다" 로 멈춘다 | `nsis-hooks.nsh` 가 번들에 들어갔는지 (`tauri.conf.json` 의 `installerHooks`). 급하면 작업 관리자에서 `deskboard.exe` 를 끝내고 다시 실행 |

## 설정을 옮길 때

`%APPDATA%\com.user.deskboard\` 를 통째로 복사하면 배치·메모·일정·기록이 따라온다.
다만 **DPAPI 로 암호화한 값(`github.dat`)은 그 PC 의 그 계정에서만** 풀리므로 새 PC 에서 다시 입력해야 한다.
해상도가 작은 PC 로 옮기면 화면 밖으로 나간 위젯이 생길 수 있다 — 설정 → 레이아웃 초기화.
폴더 위젯의 "기존 폴더 연결" 은 절대 경로라 새 PC 에 그 폴더가 없으면 위젯이 그 사실을 알려 준다.
