import type { ComponentType } from "react";

export type SettingField =
  | { key: string; label: string; type: "boolean"; default: boolean }
  | { key: string; label: string; type: "number"; default: number; min?: number; max?: number; step?: number }
  | { key: string; label: string; type: "text"; default: string; placeholder?: string }
  | { key: string; label: string; type: "select"; default: string; options: { value: string; label: string }[] }
  | { key: string; label: string; type: "path"; pick: "image" | "directory"; default: string };

export type WidgetSettings = Record<string, unknown>;

export interface WidgetProps<S extends WidgetSettings = WidgetSettings> {
  instanceId: string;
  settings: S;
  /** 위젯 크기(px). 반응형 레이아웃에 쓴다. */
  size: { w: number; h: number };
  /** 편집 모드 여부 */
  editing: boolean;
}

export interface WidgetDefinition<S extends WidgetSettings = WidgetSettings> {
  /** 고유 id. 인스턴스 설정 키와 이벤트 접두사로도 쓰인다. */
  id: string;
  title: string;
  icon?: string;
  component: ComponentType<WidgetProps<S>>;
  defaultSize: { w: number; h: number };
  minSize: { w: number; h: number };
  /** 선언하면 설정 패널이 자동 생성된다. */
  settingsSchema?: SettingField[];
  /** 항상 정확히 하나만 존재하고 제거할 수 없다 (설정 위젯). */
  singleton?: boolean;
}

export function defaultsOf(schema?: SettingField[]): WidgetSettings {
  const out: WidgetSettings = {};
  for (const f of schema ?? []) out[f.key] = f.default;
  return out;
}
