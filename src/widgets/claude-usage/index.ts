import type { WidgetDefinition } from "../types";
import { ClaudeUsage, type ClaudeUsageSettings } from "./ClaudeUsage";

export const claudeUsageWidget: WidgetDefinition<ClaudeUsageSettings> = {
  id: "claude-usage",
  title: "Claude 사용량",
  icon: "✨",
  component: ClaudeUsage,
  defaultSize: { w: 340, h: 260 },
  minSize: { w: 240, h: 90 },
  settingsSchema: [
    { key: "period", label: "기간", type: "select", default: "today",
      options: [{ value: "today", label: "오늘" }, { value: "week", label: "최근 7일" }, { value: "month", label: "최근 30일" }, { value: "session", label: "현재 세션" }] },
    { key: "showChart", label: "7일 비용 그래프", type: "boolean", default: true },
    { key: "showModels", label: "모델별 비용", type: "boolean", default: true },
    { key: "krwRate", label: "원화 환율 (0=숨김)", type: "number", default: 1380, min: 0, step: 10 },
  ],
};
