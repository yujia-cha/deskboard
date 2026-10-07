import { createContext, useContext } from "react";
import type { WidgetSettings } from "../core/widgetTypes";

export interface WidgetContextValue {
  widgetId: string;
  instanceId: string;
  settings: WidgetSettings;
  size: { w: number; h: number };
  editing: boolean;
}

/** 호스트(`components/WidgetHost`)가 위젯 하나마다 채워 준다. */
export const WidgetContext = createContext<WidgetContextValue | null>(null);

export function useWidgetContext(): WidgetContextValue {
  const v = useContext(WidgetContext);
  if (!v) throw new Error("deskboard SDK 훅은 위젯 안에서만 쓸 수 있습니다");
  return v;
}
