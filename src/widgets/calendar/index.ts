import type { WidgetDefinition } from "../types";
import { Calendar, type CalendarSettings } from "./Calendar";

export const calendarWidget: WidgetDefinition<CalendarSettings> = {
  id: "calendar",
  title: "캘린더",
  icon: "📅",
  component: Calendar,
  defaultSize: { w: 320, h: 400 },
  minSize: { w: 230, h: 230 },
  settingsSchema: [
    { key: "weekStartsMonday", label: "월요일 시작", type: "boolean", default: false },
  ],
};
