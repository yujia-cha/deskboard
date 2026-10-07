# deskboard 사용자 위젯

이 폴더(`widgets`)에 **폴더를 하나 만들면 위젯 하나**가 됩니다. 앱을 다시 빌드할 필요가 없고,
파일을 저장하면 바로 다시 불러옵니다. 이 README 와 `widget.schema.json` 은 앱이 시작할 때마다 다시 써지니
직접 고치지 마세요.

> **보안 경고** — JSX/JS 모듈 위젯은 대시보드 안에서 **앱과 같은 권한**으로 실행됩니다. 파일을 읽고,
> 명령을 돌리고, 저장된 토큰에 접근할 수 있다는 뜻입니다. **직접 쓴 코드, 또는 읽어서 이해한 코드만** 넣으세요.
> 인터넷에서 받은 위젯을 그대로 넣지 마세요. 남이 만든 코드를 돌려야 한다면 격리되는 **HTML 위젯**(아래)이 더 안전합니다.
> 무한 루프 같은 코드는 격리할 수 없어 대시보드가 멈출 수 있습니다. 그럴 땐 폴더 이름 앞에 `_` 를 붙여 끄세요.

## 폴더 구조

```
widgets/
└─ my-widget/          ← 폴더 이름 = 위젯 id
   ├─ widget.json      ← 이름·크기·설정·명령 (필수)
   ├─ index.jsx        ← 컴포넌트 (default export)
   └─ 그 밖의 *.js *.ts *.css *.json …
```

- **id 규칙**: 소문자·숫자·`_`·`-`, 영문/숫자로 시작, 48자 이하 (`^[a-z0-9][a-z0-9_-]{0,47}$`).
  규칙에 어긋나는 폴더는 목록에 오류로 표시됩니다.
- **`_` 또는 `.` 로 시작하는 폴더는 무시**합니다. 위젯을 끄고 싶으면 폴더 이름 앞에 `_` 를 붙이세요.
  `_example-disk` 는 예제입니다 — **`_` 를 떼고 `example-disk` 로 이름을 바꾸면 켜집니다.**
- **내장 위젯과 같은 이름**의 폴더를 만들면 내장본을 **덮어씁니다.** 이름을 `_` 로 바꾸면 내장본으로 돌아갑니다.
  (설정 패널의 "복사해서 고치기"는 내장 위젯을 이 폴더로 복사해 줍니다.)
- `*.test.*` 파일은 불러오지 않습니다.

## widget.json

주석과 마지막 쉼표를 써도 됩니다. 맨 위에 `"$schema": "../widget.schema.json"` 을 적으면 VS Code 가 자동완성을 해 줍니다.

| 필드 | 설명 |
|---|---|
| `title` | 이름 (기본: 폴더 이름) |
| `icon`, `description` | 아이콘, 설명 |
| `entry` | 진입 파일. 기본: `index.tsx` → `index.jsx` → `index.ts` → `index.js` → `index.html` 중 처음 있는 것. `.html` 은 iframe 위젯, 나머지는 모듈 위젯 |
| `defaultSize` | `{ "w": 240, "h": 140 }` 기본값. 추가할 때의 크기 |
| `minSize` | `{ "w": 120, "h": 80 }` 기본값. `defaultSize` 보다 크면 줄입니다 |
| `settingsSchema` | 설정 패널에 자동으로 그려질 항목 배열 |
| `provider` | 정보용 — 쓰는 내장 Provider 이름 |
| `command` | 주기적으로 실행할 명령 (아래) |
| `subscribe` | iframe 위젯이 받을 내장 이벤트 이름 |

알 수 없는 필드는 경고로 표시됩니다. `singleton` 은 지원하지 않고 무시합니다.

### settingsSchema

항목마다 `key`, `label`, `type`, 그리고 `showIf`(`{ "key": "...", "equals": 값 }` — 다른 설정값이 같을 때만 보임)를 쓸 수 있습니다.

| type | 추가 필드 |
|---|---|
| `boolean` | `default` |
| `number` | `default`, `min`, `max`, `step` |
| `text` | `default`, `placeholder` |
| `select` | `default`, `options: [{ value, label }]` |
| `path` | `default`, `pick: "image" \| "directory"` |
| `note` | 값 없는 설명 줄 — `label` 이 본문 |

## index.jsx

```jsx
import { useCommand, useStorage, Gauge } from "deskboard";

export default function MyWidget({ instanceId, settings, size, editing }) {
  const out = useCommand();            // widget.json 의 command 결과
  const [count, setCount] = useStorage(0);   // 인스턴스별 저장소 (재시작해도 남음)
  return <div onClick={() => setCount(count + 1)}>{count}</div>;
}
```

- default export 컴포넌트가 `{ instanceId, settings, size, editing }` 를 받습니다.
  `size` 는 카드 크기(px), `editing` 은 편집 모드 여부입니다.
- `import ... from "deskboard"` 로 앱이 제공하는 기능(`useCommand`, `useStorage`, `Gauge` 등)을 씁니다.
  `"react"`, `"react-dom"`, `"date-fns"`, 같은 폴더의 `"./파일"`(.js/.jsx/.ts/.tsx/.json/.css)도 import 할 수 있고,
  그 밖의 패키지는 쓸 수 없습니다.
- 색·글자 크기는 하드코딩하지 말고 CSS 변수(`var(--text)`, `var(--accent)`, `var(--danger)` …)를 쓰세요.
  토큰 목록은 `user.sample.css` 에 있습니다.
- CSS 는 위젯끼리 섞이므로 클래스 이름 앞에 위젯 이름을 붙이세요 (`.my-widget-title`).

### "deskboard" 에서 쓸 수 있는 것

| 이름 | 설명 |
|---|---|
| `useWidget()` | `{ widgetId, instanceId, settings, size, editing }` |
| `useCommand()` | `command` 의 마지막 결과 `{ ok, text, data, error, exitCode, at, ms }` (아직 없으면 `null`) |
| `runCommand(instanceId)` | 주기를 기다리지 않고 지금 실행 |
| `useStorage(초기값)` | `[값, 바꾸기, 읽었는가]` — 인스턴스별 저장소 |
| `updateWidgetSettings(instanceId, { key: 값 })` | 설정을 바꾸고 저장 |
| `openSettings(instanceId)` | 이 위젯의 설정 패널 열기 |
| `useInstanceAccent(instanceId)` · `useEditing()` | 인스턴스 강조색 · 편집 모드 여부 |
| `openUrl(url)` · `openDialog(옵션)` | 브라우저로 열기(http/https) · 파일/폴더 고르기 |
| `asset(widgetId, "img.png")` | 위젯 폴더 안 파일의 주소 (`<img src>` 등) |
| `call(커맨드, 인자)` · `invokeInOrder` | 앱 백엔드 커맨드 호출 (내장 위젯이 쓰는 것과 같음) |
| `useProviderEvent(이름)` · `useProviderData(이벤트, 커맨드)` · `useEvent(이름, 함수)` | 백엔드 이벤트 받기 (`sysmon://update` 등) |
| `setOverlayRect(id, rect)` · `useDismiss` · `useFileDrop` | 카드 밖으로 펼치는 팝업·닫기·파일 끌어놓기 |
| `Gauge` · `Sparkline` · `Donut` · `ContextMenu` | 내장 위젯이 쓰는 컴포넌트 |

내장 위젯의 소스가 가장 좋은 예제입니다 — 설정 패널에서 내장 위젯을 고르고 **"복사해서 고치기"** 를 누르면 이 폴더로 복사됩니다.

## command — 명령 실행

```json
"command": {
  "run": "Get-Date -Format o",
  "shell": "powershell",
  "interval": 60,
  "timeout": 10,
  "parse": "text",
  "pauseWhenHidden": true
}
```

| 필드 | 기본값 | 설명 |
|---|---|---|
| `run` | (필수) | 문자열(셸이 해석) 또는 문자열 배열(argv) |
| `shell` | `powershell` | `powershell` · `pwsh` · `cmd` · `none`. `none` 은 셸 없이 직접 실행하므로 `run` 이 배열이어야 합니다 (`["git","status","-s"]`) |
| `interval` | 60 | 초 단위 주기, 최소 1. **0 이면 시작할 때 한 번 + 수동 새로고침만** |
| `timeout` | 10 | 초, 최대 60. 넘으면 강제 종료하고 "시간 초과" 오류 |
| `parse` | `text` | `text`: 그대로 · `json`: JSON 으로 해석(`data`) · `lines`: 비어 있지 않은 줄의 배열(`data`) |
| `pauseWhenHidden` | true | 대시보드가 가려져 있으면(트레이로 숨김, 전체화면 앱) 실행을 건너뜁니다 |

- 실행은 **직렬**입니다 — 한 인스턴스의 명령은 겹쳐 돌지 않고, 앱 전체에서도 동시에 4개까지만 돕니다.
- 작업 폴더는 위젯 폴더입니다. 창은 뜨지 않고, 출력은 UTF-8 로 읽습니다(PowerShell/cmd 는 자동 설정).
  `.ps1` 파일을 직접 가리키면(`"run": "./get.ps1"`) `-File` 로 실행하는데, 이때는 스크립트 안에서
  `[Console]::OutputEncoding = [Text.Encoding]::UTF8` 를 직접 설정하세요.
- 출력 상한은 stdout 256KB, stderr 16KB 입니다. 종료 코드가 0 이 아니면 stderr(없으면 "종료 코드 N")가 오류가 됩니다.
- **설정값은 환경 변수로만 전달됩니다** (명령 문자열에 끼워 넣지 않습니다):
  - `DESKBOARD_INSTANCE` — 인스턴스 id
  - `DESKBOARD_WIDGET_DIR` — 위젯 폴더 경로
  - `DESKBOARD_SETTINGS` — 설정 전체(JSON 문자열)
  - `DESKBOARD_SETTING_<KEY>` — 문자열·숫자·불리언 설정 하나씩 (KEY 는 대문자, 영숫자 외에는 `_`)
- 명령은 `widget.json` 에 적힌 것만 실행됩니다. 위젯 코드가 임의의 명령 문자열을 만들어 실행할 수는 없습니다.

## 저장소

`useStorage` 로 위젯 인스턴스마다 JSON 값을 저장할 수 있습니다 (최대 256KB).
데이터 폴더의 `userdata/<instanceId>.json` 에 저장됩니다.

## HTML(iframe) 위젯

`index.html` 이 진입 파일이면 격리된 iframe 으로 띄웁니다. 앱 권한이 없고 아래 `window.deskboard` 로
열어 준 기능만 쓸 수 있어, 남이 만든 코드를 돌릴 때 더 안전합니다. 아무 HTML·JS·CSS 를 써도 되고,
같은 폴더의 파일은 상대 경로(`<link href="style.css">`, `<script src="app.js">`)로 부릅니다.
예제는 `_example-cpu` 입니다 — `_` 를 떼면 켜집니다.

브리지 스크립트는 앱이 자동으로 넣어 줍니다 (직접 넣을 필요 없음):

```js
deskboard.onProps(({ settings, size, editing }) => { … });   // 처음에도 한 번
deskboard.onTheme((theme) => { … });                          // CSS 변수는 자동 적용
deskboard.onCommand((out) => { … });                          // command 결과
deskboard.runCommand();
await deskboard.storage.get();  deskboard.storage.set({ … });
deskboard.subscribe("sysmon://update", (payload) => { … });   // widget.json 의 subscribe 에 적은 것만
deskboard.updateSettings({ key: 값 });
deskboard.openUrl("https://…");                               // <a href> 클릭도 자동으로 브라우저에서 열림
await deskboard.ready;                                        // 첫 props 를 받은 뒤
```

- 받을 수 있는 이벤트: `sysmon://update`, `claude_usage://limits`, `claude_usage://update`, `calendar://changed`,
  `notes://changed`, `activity://changed`. `widget.json` 의 `"subscribe"` 에 먼저 적어야 합니다.
- 배경은 투명합니다 (카드 표면이 비침). 테마 CSS 변수(`--text`, `--accent`, `--font` …)를 그대로 쓸 수 있습니다.
- 스크립트 오류는 카드에 표시되고, 5초 안에 브리지가 응답하지 않으면 "다시 불러오기" 가 뜹니다.

## 문제가 생기면

- 위젯이 목록에 오류와 함께 나오면 설정 패널의 위젯 항목에 `widget.json` 의 `줄:칸` 오류가 표시됩니다.
- 위젯 카드가 오류로 바뀌면 다시 시도하거나, 덮어쓴 위젯이라면 폴더 이름을 `_이름` 으로 바꿔 내장본으로 돌아가세요.
