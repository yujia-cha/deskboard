# 위젯 만들기

위젯 = **폴더 하나** (`widget.json` + 진입 파일). 앱에 컴파일해 넣지 않는다 — 시작할 때 백엔드가 폴더를 읽고
프론트 로더가 평가한다. 내장 위젯과 사용자 위젯은 **같은 템플릿·같은 로더·같은 SDK** 를 쓴다.

| | 어디에 | 비고 |
|---|---|---|
| 내장 | 저장소 `widgets/<id>/` | release 는 `bundle.resources`(`../widgets/` → `widgets/`)로 설치본에 들어간다. **debug 는 저장소 폴더를 직접 읽어서** 저장하면 핫 리로드 |
| 사용자 | `%APPDATA%/com.user.deskboard/widgets/<id>/` | 앱을 다시 빌드할 필요 없음. 설정 패널의 "폴더 열기" |

(설치본의 `%APPDATA%` 는 MSIX 가상화될 수 있다 — 실제 경로는 설정 패널의 "폴더 열기"가 연다.)

규칙:
- **폴더 이름 = 위젯 id** (`^[a-z0-9][a-z0-9_-]{0,47}$`). 인스턴스 저장값의 키이므로 바꾸면 기존 카드가 `MissingWidget` 이 된다.
- 같은 이름이 두 루트에 있으면 **사용자 폴더가 내장본을 덮어쓴다** (설정 패널 "복사해서 고치기" = `widgets_eject`).
- 이름이 `_` 또는 `.` 로 시작하는 폴더는 무시 = 비활성. 덮어쓴 걸 끄려면 `_<id>` 로 바꾸면 내장본으로 돌아간다
  (오류 카드의 "내장본으로 되돌리기" = `widgets_disable`).
- `*.test.*` 는 번들·배포에서 빠진다. 폴더 깊이 4, 파일당 512KB, 합계 4MB 까지.
- 오류가 난 폴더도 목록에서 빠지지 않고 오류와 함께 남는다 — 왜 안 뜨는지 보여야 고칠 수 있다.

## 모듈 위젯 (JSX/TSX) 템플릿

```
widgets/hello/
├─ widget.json
├─ index.tsx        default export 컴포넌트 (+ 선택: export function migrate)
├─ Hello.tsx · Hello.css · format.ts · format.test.ts …
```

```json
{
  "$schema": "../widget.schema.json",
  "title": "인사", "icon": "👋", "description": "…",
  "entry": "index.tsx",
  "defaultSize": { "w": 240, "h": 120 },
  "minSize": { "w": 160, "h": 80 },
  "provider": "hello",
  "settingsSchema": [
    { "key": "name", "label": "이름", "type": "text", "default": "world" },
    { "key": "loud", "label": "크게", "type": "boolean", "default": false }
  ]
}
```

```tsx
// index.tsx
export { Hello as default } from "./Hello";
```
```tsx
// Hello.tsx
import { useWidget, type WidgetProps } from "deskboard";
import "./Hello.css";
export function Hello({ settings, size }: WidgetProps) {
  const text = `Hello, ${settings.name}`;
  return <div style={{ fontSize: Math.min(size.h / 3, 40) }}>{settings.loud ? text.toUpperCase() : text}</div>;
}
```

- `entry` 를 생략하면 `index.tsx → .jsx → .ts → .js → .html` 중 처음 있는 것. 확장자가 `.html` 이면 iframe 위젯.
- 설정 필드: `boolean` · `number`(min/max/step) · `text` · `select` · `path`(`pick: image|directory`) · `note`(값 없는 설명 줄),
  공통 `showIf: {key, equals}`. **잘못된 줄은 그 줄만 버리고 경고**한다 — 한 줄 틀렸다고 위젯 전체를 죽이지 않는다.
- `singleton` 은 쓸 수 없다 (셸의 설정 위젯 전용).
- **import 할 수 있는 것은 아래뿐이다** (그 밖은 로더가 throw):
  `react` · `react/jsx-runtime` · `react-dom` · `deskboard`(SDK) · `date-fns` · `date-fns/locale` ·
  같은 폴더의 상대 경로 파일(`.ts/.tsx/.js/.jsx/.json`, `.css`). 이 규칙이 예전의 "core/ 와 components/ 만 import" 를 대체한다.
  `@tauri-apps/*` 도 직접 못 쓴다 — SDK 로 한다. 다른 위젯 폴더도 import 하지 않는다.
- CSS 는 `<style data-widget=id>` 로 주입되고 위젯끼리 섞인다 → 클래스 앞에 위젯 이름을 붙인다. 색·타이포는 `theme.css` 토큰만 쓴다.
- 새 import 를 허용하려면 `src/core/loader/host.ts` 와 사용자 README(`src-tauri/resources/userland/widgets/README.md`)를 같이 고친다.

## 로더가 하는 일 (디버깅할 때)

백엔드 `widgets_bundle(id)` 가 텍스트 파일 맵을 준다 → `sucrase`(지연 로드 청크; jsx·typescript·imports → CJS)로 변환 →
`new Function("require","module","exports", code)` 로 평가, `require` 는 위 허용 목록만 푼다.
- 변환 결과는 **소스 해시로 IndexedDB 에 캐시** — 두 번째 시작부터 변환을 건너뛴다 (`compile.ts` 의 `COMPILER_VERSION` 을 올리면 전체 무효).
- sucrase 는 줄을 보존하므로 오류의 `파일:줄` 이 원본과 같다. DevTools 에는 `dbw://<id>/<file>` 로 보인다.
- `React` 는 **호스트 것 하나뿐**이어야 hooks 가 동작한다 (그래서 `react` 를 번들하지 않고 `host.ts` 가 넘긴다).
- 폴더가 바뀌면 `widgets://changed` → 지문(version)이 바뀐 위젯만 다시 불러와 다시 마운트한다.
- 렌더 중 throw 는 `WidgetHost` 의 error boundary 가 그 카드만 오류 카드로 바꾼다 (설정을 바꾸거나 파일을 고치면 재시도).
  **무한 루프는 격리할 수 없다** — 메인 스레드가 멈춘다. 폴더 이름 앞에 `_` 를 붙여 끈다.
- 정의가 없는 인스턴스(폴더 삭제·개명)는 지워지지 않고 `MissingWidget` 카드로 남는다 — 폴더가 돌아오면 같은 설정으로 다시 뜬다.

## SDK — `import … from "deskboard"`

구현은 `src/sdk/index.ts`. tsconfig `paths` 와 vite alias 가 같은 파일을 가리켜 `widgets/**` 도 typecheck·vitest 가 푼다.

| 묶음 | 이름 |
|---|---|
| 현재 위젯 | `useWidget()` → `{ widgetId, instanceId, settings, size, editing }` · `useEditing()` · `useInstanceAccent(instanceId)` |
| 설정 | `updateWidgetSettings(instanceId, patch)` · `openSettings(instanceId?)` |
| 백엔드 | `call`(= `invoke`) · `invokeInOrder` · `useEvent(name, fn)` · `useProviderEvent(name)` · `useProviderData(event, initCmd)` |
| 셸 연동 | `setOverlayRect(id, rect\|null)` · `useDismiss` · `useFileDrop` · `openUrl(url)`(http/https 만) · `openDialog(opts)` · `asset(widgetId, "img.png")` |
| 컴포넌트 | `Gauge` · `Sparkline` · `Donut` · `ContextMenu` |
| 사용자 데이터 | `useCommand()` · `runCommand(instanceId)` · `useStorage(initial)` |
| 타입 | `WidgetProps` · `WidgetSettings` · `SettingField` · `Rect` · `Slice` · `ContextMenuItem` · `CommandOutput` |

- 컴포넌트는 `{ instanceId, settings, size, editing }` 를 받는다. `settings` 는 스키마 기본값 위에 저장값을 얹은 것.
- **`invokeInOrder`**: 백엔드를 켜고 끄는 토글(`*_set_active`)은 이걸로 보낸다. `invoke` 는 도착 순서를 보장하지 않는데
  StrictMode 가 `true → false → true` 를 연달아 보내면 `false` 가 마지막에 닿아 기능이 꺼진 채 남는다.
- **`setOverlayRect`**: 카드 밖으로 펼치는 팝업(`createPortal`)의 히트 영역을 등록한다. 안 하면 클릭이 바탕화면으로 샌다.
- **`asset`** 은 `http://dbw.localhost/<id>/<file>` 을 만든다. `convertFileSrc` 는 경로의 `/` 까지 `%2F` 로 인코딩해서 쓰지 않는다.

## migrate — 옛 저장값 옮기기

`index.tsx` 에서 `export function migrate(saved, instanceId)` 를 내보내면 저장된 인스턴스 설정을 불러올 때 거친다
(레지스트리가 먼저 모듈을 불러 둔 뒤 `settings.load()` 가 부른다 — 그래서 시작 순서가 `config.init + registry.init → settings.load` 다).
사용자가 **직접 고르지 않은 옛 기본값만** 새 값으로 바꾼다. 예: `widgets/clock/index.tsx`. 분리 파일이면 `migrate.ts` 를 재수출한다
(`widgets/folder`, `widgets/github`) — 테스트는 `migrate.test.ts` 로.

## command — 명령 실행 (stdout → 위젯)

```json
"command": { "run": "Get-Date -Format o", "shell": "powershell", "interval": 60, "timeout": 10, "parse": "text", "pauseWhenHidden": true }
```
위젯: `const out = useCommand();` → `{ ok, text, data, error, exitCode, at, ms } | null`. 지금 한 번: `runCommand(instanceId)`.

- 필드·기본값은 사용자 README 표가 정본: `run`(문자열 또는 argv 배열) · `shell` `powershell|pwsh|cmd|none`(`none` 은 argv 필수) ·
  `interval` 60초(0 = 시작 시 한 번 + 수동) · `timeout` 10초(최대 60) · `parse` `text|json|lines` · `pauseWhenHidden` true.
- **시작·정지는 셸이 한다** (`WidgetHost` 가 `invokeInOrder("widget_command_start/stop")`). 위젯은 읽기만 한다 — 위젯이 직접 켜면 StrictMode 에서 꺼진 채 남는다.
- **명령 문자열은 스캔한 manifest 에서만** 읽는다. 프론트는 위젯 id 와 설정값만 보낸다. 설정값은 **환경 변수로만** 전달
  (`DESKBOARD_INSTANCE`·`DESKBOARD_WIDGET_DIR`·`DESKBOARD_SETTINGS`·`DESKBOARD_SETTING_<KEY>`) — 명령 문자열에 끼우지 않는다(인젝션 방지).
- 인스턴스별 직렬(겹치지 않음) + 전역 동시 4개. 출력 상한 stdout 256KB / stderr 16KB. `CREATE_NO_WINDOW`, cwd = 위젯 폴더.
- **한글(CP949) 함정**: PowerShell 은 `[Console]::OutputEncoding=UTF8` 를 앞에 붙여 해결. `cmd` 는 `chcp 65001` 을 붙이지만
  **내장 `echo` 는 무시하고 ANSI 로 낸다**(실측) — 한글이면 `type 파일` 처럼 바이트를 그대로 흘리거나 PowerShell 을 쓴다.
  `.ps1` 을 직접 가리키면 `-File` 로 돌아 앞의 인코딩 설정이 안 붙는다 → 스크립트 안에서 `[Console]::OutputEncoding = [Text.Encoding]::UTF8`.
- **시간 초과 때 죽이는 것은 직계 자식 하나뿐이다.** 그 아래 손자 프로세스는 남을 수 있다 (프로세스 트리 kill 은 TODO — Job Object).

## storage — 인스턴스 저장소

`const [v, setV, loaded] = useStorage(initial)` — `useState` 처럼 쓴다. `userdata/<instanceId>.json`, 256KB, 300ms 모아서 저장
(떠날 때 남은 것은 즉시 flush), tmp 에 쓴 뒤 rename. 비밀값은 여기 넣지 않는다 (평문) — `providers/secrets`(DPAPI) 를 쓴다.

## HTML(iframe) 위젯

진입 파일이 `index.html` 이면 `FrameWidget` 이 격리된 iframe 으로 띄운다. 아무 HTML·JS·CSS 를 써도 되고 같은 폴더 파일은 상대 경로로 부른다.
`widget.json` 에 `command`·`subscribe` 를 선언할 수 있다. 예제: 데이터 폴더 `widgets/_example-cpu` (`_` 를 떼면 켜진다).

- `sandbox="allow-scripts allow-forms"` — **same-origin 없음**. Tauri IPC 에 못 닿으므로 `core/frameBridge.ts` 의 허용 목록이 **권한의 전부**다.
- 파일은 커스텀 스킴 `dbw` 로 서빙한다 (`userland/scheme.rs`): `http://dbw.localhost/<id>/<file>`. `..`·`%2e%2e`·절대 경로·symlink 탈출은 403.
  `bridge.js` 는 스킴이 `<head>` 맨 앞에 **자동으로 넣는다**(`/_sdk/bridge.js`) — 작성자가 넣지 않는다.
- `window.deskboard`: `onProps` · `onTheme`(CSS 변수는 자동 적용) · `onCommand` · `runCommand` · `storage.get/set` · `subscribe` · `updateSettings` · `openUrl` · `ready`.
  메시지는 `e.source` 가 그 iframe 인지와 메서드 화이트리스트로 검증한다 (`parseFrameMessage`). `openUrl` 은 http/https 만.
- 받을 수 있는 이벤트는 `PUBLIC_EVENTS` ∩ `widget.json` 의 `subscribe`: `sysmon://update` · `claude_usage://limits` · `claude_usage://update` ·
  `calendar://changed` · `notes://changed` · `activity://changed`. 구독만으로 비용이 생기지 않는 것만 연다 (켜야 도는 provider 는 제외).
- 테마가 바뀌면(팔레트·강조색·`user.css`) 토큰을 다시 보낸다. 편집 모드에서는 `pointer-events:none` 이라 카드를 끌 수 있다.
- **IME**: iframe 안 입력란은 셸이 못 본다 → 브리지가 `focus{editable}` 을 보내면 `iframe.dataset.editable` 을 세우고, `App.tsx` 의 `editable()` 이 이를 입력 요소로 인정한다.
  빠뜨리면 "칠 것이 없는 위젯"으로 판단해 포커스를 돌려줘 한글이 안 쳐진다.
- ready 가 5초 안에 안 오면 "응답이 없습니다 / 다시 불러오기".

## 백엔드 Provider (데이터가 OS·파일·외부 API 에서 올 때)

Provider 는 앱에 **컴파일돼 있다** — 위젯 폴더만으로는 못 만든다 (사용자 위젯은 `command`/`storage` 를 쓴다).
```
src-tauri/src/providers/<id>/mod.rs
```
```rust
impl Provider for HelloProvider {
    fn id(&self) -> &'static str { "hello" }
    fn start(&self, app: AppHandle) {
        // 백그라운드 태스크를 띄우고 즉시 반환. 주기 데이터는 이벤트로 푸시한다.
        std::thread::spawn(move || loop {
            let _ = app.emit("hello://update", serde_json::json!({ "n": 42 }));
            std::thread::sleep(std::time::Duration::from_secs(5));
        });
    }
}
#[tauri::command]
pub fn hello_do_thing(arg: String) -> Result<String, String> { Ok(arg) }
```
1. `providers/mod.rs` 의 `all()` 에 `Box::new(hello::HelloProvider)` 추가
2. `lib.rs` 의 `generate_handler![...]` 에 커맨드 추가
3. 위젯에서 `useProviderEvent` / `useProviderData` / `call("hello_do_thing", {arg})`. `widget.json` 의 `provider` 는 정보용.

관례: 이벤트 이름 `<id>://<what>` · 실패는 `Result<_, String>` 사용자 메시지 · 외부 자원은 트레이트 뒤에(`SensorSource` …) ·
폴링은 위젯이 떠 있을 때만(`*_set_active` + `invokeInOrder`) · 폴링 주기는 `userland::config::interval(key, default, min)` 로 읽어
`config.jsonc` 의 `intervals` 로 고칠 수 있게 한다 (`docs/CONFIG.md`).

## 테스트

- 위젯 폴더의 순수 함수는 옆에 `*.test.ts` (vitest). 폴더 안에서 `import … from "deskboard"` 가 풀리도록 vite alias 가 걸려 있다.
- **`src/core/loader/widgets.test.ts`** 가 저장소 `widgets/*` 를 전부 **실제 로더로** 불러 본다 — id 규칙, 크기, 스키마, 쓸 수 없는 import,
  default export 누락, `migrate` 타입을 잡는다. 새 내장 위젯은 따로 등록할 것 없이 여기에 자동으로 걸린다.
- vitest 는 `src-tauri/**` 를 제외한다 — tauri 빌드가 리소스(`widgets/`)를 `target/` 아래로 복사하므로 그 사본까지 돌면 중복이다.
- Rust: 모듈 안 `#[cfg(test)]` (파서·집계·저장소, 외부 자원은 Fake). `userland/` 는 scan·scheme 경로 탈출·runner(FakeExec)·storage·config 를 덮는다.

## 보안

- **모듈 위젯은 메인 웹뷰에서 앱 전체 권한으로 돈다** — 파일을 읽고, 명령을 돌리고, Tauri 커맨드(저장된 토큰 포함)를 부를 수 있다.
  그래서 README·사용자 README·설정 패널이 "직접 쓴 코드만 넣으라"고 경고한다. 로드 대상은 두 루트뿐이고 네트워크로 설치하는 경로는 없다.
- 남의 코드를 돌려야 하면 **iframe 위젯**이 선택지다. 위 허용 목록이 실제 경계다.
- 스킴은 위젯 폴더 밖을 서빙하지 않는다. 명령은 manifest 에 적힌 것만 실행된다.
