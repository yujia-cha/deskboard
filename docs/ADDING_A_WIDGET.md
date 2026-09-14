# 위젯 추가하기

위젯 = **프론트 컴포넌트 1개** (+ 필요하면 **백엔드 Provider 1개**). 둘 다 다른 위젯을 건드리지 않는다.

## 1. 프론트 (필수)

```
src/widgets/<id>/
├─ index.ts        WidgetDefinition (id, 제목, 기본/최소 크기, 설정 스키마)
├─ <Name>.tsx      컴포넌트 — props: { instanceId, settings, size, editing }
└─ <Name>.css
```

```ts
// src/widgets/hello/index.ts
import type { WidgetDefinition } from "../types";
import { Hello, type HelloSettings } from "./Hello";

export const helloWidget: WidgetDefinition<HelloSettings> = {
  id: "hello",
  title: "인사",
  icon: "👋",
  component: Hello,
  defaultSize: { w: 240, h: 120 },
  minSize: { w: 160, h: 80 },
  chromeless: false,               // true 면 헤더 숨김 (시계처럼)
  settingsSchema: [                // 선언만 하면 설정 패널이 자동 생성된다
    { key: "name", label: "이름", type: "text", default: "world" },
    { key: "loud", label: "크게", type: "boolean", default: false },
  ],
};
```

```tsx
// src/widgets/hello/Hello.tsx
import type { WidgetProps } from "../types";
export interface HelloSettings extends Record<string, unknown> { name: string; loud: boolean }
export function Hello({ settings, size }: WidgetProps<HelloSettings>) {
  const text = `Hello, ${settings.name}`;
  return <div style={{ fontSize: Math.min(size.h / 3, 40) }}>{settings.loud ? text.toUpperCase() : text}</div>;
}
```

마지막으로 `src/widgets/registry.ts` 의 `WIDGETS` 배열에 `helloWidget` 을 추가한다. 끝.

규칙:
- 위젯은 `core/`(ipc, settings), `components/`(Gauge, Sparkline 등) 만 import 한다. 다른 위젯 폴더는 import 하지 않는다.
- 색상은 CSS 변수(`--accent`, `--text`, `--text-dim`, `--surface`, `--ok`, `--warn`, `--danger`)만 쓴다. 반투명/단색 모드가 자동 반영된다.
- `size` 로 반응형 처리한다 (작으면 요소를 숨기는 식).

## 2. 백엔드 Provider (데이터가 OS·파일·외부 API 에서 올 때)

```
src-tauri/src/providers/<id>/mod.rs
```

```rust
use super::Provider;
use tauri::{AppHandle, Emitter};

pub struct HelloProvider;

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

// 프론트가 호출하는 조작은 커맨드로
#[tauri::command]
pub fn hello_do_thing(arg: String) -> Result<String, String> { Ok(arg) }
```

1. `providers/mod.rs` 의 `all()` 에 `Box::new(hello::HelloProvider)` 추가
2. `lib.rs` 의 `generate_handler![...]` 에 커맨드 추가
3. 프론트에서:
   - 이벤트 구독: `useProviderEvent<T>("hello://update")`
   - 초기값 + 이벤트: `useProviderData<T>("hello://update", "hello_initial_command")`
   - 커맨드 호출: `invoke("hello_do_thing", { arg })`

관례:
- 이벤트 이름은 `<id>://<what>` (`sysmon://update`, `calendar://changed`).
- 실패는 `Result<_, String>` 으로 사람이 읽을 메시지를 돌려주고, 위젯이 표시한다.
- 외부 서비스는 트레이트 뒤에 둔다 (`SensorSource`, `CalendarSource`) — 구현체 교체가 다른 코드에 영향을 주지 않게.
- 폴링은 위젯이 떠 있을 때만 (Spotify 의 `spotify_set_active` 참고).

## 3. 테스트

- Rust: 모듈 안 `#[cfg(test)]` — 파서/집계/저장소 로직 (외부 자원 없이 결정론적으로)
- 프론트: `*.test.ts` (vitest) — 순수 함수, 레지스트리 무결성
