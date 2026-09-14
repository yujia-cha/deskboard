import type { WidgetDefinition } from "../types";
import { ClaudeUsage, type ClaudeUsageSettings } from "./ClaudeUsage";

export const claudeUsageWidget: WidgetDefinition<ClaudeUsageSettings> = {
  id: "claude-usage",
  title: "Claude 한도",
  icon: "✨",
  component: ClaudeUsage,
  defaultSize: { w: 340, h: 150 },
  minSize: { w: 200, h: 100 },
  settingsSchema: [
    { key: "showOpus", label: "주간 Opus 한도도 표시", type: "boolean", default: false },
    { key: "showLocalCost", label: "로컬 추정 비용 표시 (오늘/7일)", type: "boolean", default: false },
    { key: "krwRate", label: "원화 환율 (0=숨김)", type: "number", default: 1380, min: 0, step: 10 },
  ],
};
