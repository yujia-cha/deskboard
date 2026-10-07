import * as React from "react";
import * as jsxRuntime from "react/jsx-runtime";
import * as ReactDOM from "react-dom";
import * as dateFns from "date-fns";
import * as dateFnsLocale from "date-fns/locale";
import * as sdk from "../../sdk";

/**
 * 위젯 코드가 `import` 할 수 있는 호스트 모듈 (그 밖에는 위젯 폴더 안의 파일뿐).
 * 여기에 더하면 사용자 위젯 README(`src-tauri/resources/userland/widgets/README.md`)도 고칠 것.
 *
 * `__esModule` 을 붙여 두면 sucrase 의 `_interopRequireDefault` 가 감싸지 않는다 —
 * `import React from "react"` 의 default 가 React 자신을 가리키게 한다.
 */
const esm = (ns: Record<string, unknown>) => ({ ...ns, default: (ns as { default?: unknown }).default ?? ns, __esModule: true });

let modules: Record<string, unknown> | null = null;

/**
 * 처음 쓸 때 만든다 — `sdk` 는 `settings` → 레지스트리 → 여기로 이어지는 순환 import 안에 있어서,
 * 모듈 평가 시점에 펼치면 아직 초기화되지 않은 바인딩을 건드린다.
 */
export function hostModules(): Record<string, unknown> {
  return (modules ??= {
    react: esm(React as unknown as Record<string, unknown>),
    "react/jsx-runtime": esm(jsxRuntime as unknown as Record<string, unknown>),
    // 카드 밖으로 펼치는 팝업(createPortal)용
    "react-dom": esm(ReactDOM as unknown as Record<string, unknown>),
    deskboard: esm(sdk as unknown as Record<string, unknown>),
    "date-fns": esm(dateFns as unknown as Record<string, unknown>),
    "date-fns/locale": esm(dateFnsLocale as unknown as Record<string, unknown>),
  });
}
