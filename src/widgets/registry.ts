/**
 * 위젯 레지스트리 — 새 위젯은 폴더를 만들고 여기 한 줄만 추가한다.
 * 위젯은 서로 import 하지 않고 `core/` 와 `components/` 만 의존한다.
 */
import type { WidgetDefinition } from "./types";
import { clockWidget } from "./clock";
import { sysmonWidget } from "./sysmon";
import { claudeUsageWidget } from "./claude-usage";
import { calendarWidget } from "./calendar";
import { spotifyWidget } from "./spotify";
import { folderWidget } from "./folder";
import { weatherWidget } from "./weather";
import { notesWidget } from "./notes";
import { playtimeWidget } from "./playtime";
import { githubWidget } from "./github";
import { settingsWidget } from "./settings";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const WIDGETS: WidgetDefinition<any>[] = [
  settingsWidget,
  clockWidget,
  sysmonWidget,
  claudeUsageWidget,
  calendarWidget,
  spotifyWidget,
  folderWidget,
  weatherWidget,
  notesWidget,
  playtimeWidget,
  githubWidget,
];

export const widgetById = (id: string) => WIDGETS.find((w) => w.id === id);
