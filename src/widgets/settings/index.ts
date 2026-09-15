import type { WidgetDefinition } from "../types";
import { SettingsLauncher } from "./SettingsLauncher";

export const settingsWidget: WidgetDefinition = {
  id: "settings",
  title: "설정",
  icon: "⚙",
  component: SettingsLauncher,
  defaultSize: { w: 56, h: 56 },
  minSize: { w: 40, h: 40 },
  singleton: true,
};
