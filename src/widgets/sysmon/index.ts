import type { WidgetDefinition } from "../types";
import { SysMon, type SysMonSettings } from "./SysMon";

export const sysmonWidget: WidgetDefinition<SysMonSettings> = {
  id: "sysmon",
  title: "시스템",
  icon: "📊",
  component: SysMon,
  defaultSize: { w: 360, h: 270 },
  minSize: { w: 220, h: 150 },
  settingsSchema: [
    { key: "showGpu", label: "GPU 표시", type: "boolean", default: true },
    { key: "showCores", label: "코어별 막대", type: "boolean", default: true },
    { key: "showHistory", label: "60초 히스토리", type: "boolean", default: true },
    { key: "disks", label: "디스크 (쉼표로 여러 개)", type: "text", default: "C:", placeholder: "C:, D:" },
  ],
};
