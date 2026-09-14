import type { WidgetDefinition } from "../types";
import { Clock, type ClockSettings } from "./Clock";

export const clockWidget: WidgetDefinition<ClockSettings> = {
  id: "clock",
  title: "시계",
  icon: "🕒",
  component: Clock,
  defaultSize: { w: 300, h: 140 },
  minSize: { w: 180, h: 90 },
  chromeless: true,
  settingsSchema: [
    { key: "digitStyle", label: "숫자 스타일", type: "select", default: "dots",
      options: [{ value: "system", label: "시스템 폰트" }, { value: "dots", label: "도트 매트릭스 (■)" }] },
    { key: "dotChar", label: "도트 문자 (■ 🟦 ⬜ ● 등)", type: "text", default: "■", placeholder: "■" },
    { key: "hour12", label: "12시간제", type: "boolean", default: false },
    { key: "seconds", label: "초 표시", type: "boolean", default: true },
    { key: "dateFormat", label: "날짜 형식", type: "select", default: "long",
      options: [{ value: "long", label: "2026년 9월 14일 월요일" }, { value: "short", label: "2026-09-14 (월)" }, { value: "none", label: "숨김" }] },
    { key: "timeZone", label: "시간대 (비우면 시스템)", type: "text", default: "", placeholder: "Asia/Seoul" },
  ],
};
