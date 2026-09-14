/**
 * 위젯 레지스트리 — 새 위젯은 폴더를 만들고 여기 한 줄만 추가한다.
 * 위젯은 서로 import 하지 않고 `core/` 와 `components/` 만 의존한다.
 */
import type { WidgetDefinition } from "./types";
import { clockWidget } from "./clock";
import { sysmonWidget } from "./sysmon";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const WIDGETS: WidgetDefinition<any>[] = [
  clockWidget,
  sysmonWidget,
];

export const widgetById = (id: string) => WIDGETS.find((w) => w.id === id);
