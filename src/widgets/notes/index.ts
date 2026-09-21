import type { WidgetDefinition } from "../types";
import { Notes, type NotesSettings } from "./Notes";

export const notesWidget: WidgetDefinition<NotesSettings> = {
  id: "notes",
  title: "메모 · 할 일",
  icon: "📝",
  component: Notes,
  defaultSize: { w: 280, h: 240 },
  minSize: { w: 180, h: 100 },
  settingsSchema: [
    // 목록은 위젯 인스턴스마다 독립이다 — 여러 개 띄워 용도별로 나눠 쓸 수 있다.
    { key: "title", label: "목록 이름", type: "text", default: "할 일", placeholder: "업무, 장보기…" },
    { key: "showDone", label: "완료한 항목도 표시", type: "boolean", default: true },
    { key: "strikeDone", label: "완료한 항목에 취소선", type: "boolean", default: true },
  ],
};
