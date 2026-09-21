import type { ComponentType } from "react";

/**
 * 모든 필드가 함께 갖는 것.
 *
 * `showIf` 는 **같은 위젯의 다른 설정값**을 보고 이 줄을 보일지 정한다. 서로 배타적인 모드를
 * 하나의 폼에 섞어 두면(예: 폴더의 "새로 만들기" 와 "기존 폴더 연결") 어느 칸이 지금 의미가
 * 있는지 알 수 없다 — 안 쓰는 칸은 감춘다.
 */
interface FieldBase {
  key: string;
  label: string;
  showIf?: { key: string; equals: string | number | boolean };
}

export type SettingField = FieldBase &
  (
    | { type: "boolean"; default: boolean }
    | { type: "number"; default: number; min?: number; max?: number; step?: number }
    | { type: "text"; default: string; placeholder?: string }
    | { type: "select"; default: string; options: { value: string; label: string }[] }
    | { type: "path"; pick: "image" | "directory"; default: string }
    /** 값이 없는 설명 줄 — `label` 이 곧 본문이다. 모드가 무엇을 하는지 적는 데 쓴다. */
    | { type: "note" }
  );

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
  // 설명 줄(note)은 값이 아니다 — 저장값에 undefined 키를 만들지 않는다.
  for (const f of schema ?? []) if (f.type !== "note") out[f.key] = f.default;
  return out;
}

/** `showIf` 를 현재 설정값으로 평가한다. 조건이 없으면 항상 보인다. */
export function fieldVisible(f: SettingField, settings: WidgetSettings): boolean {
  return !f.showIf || settings[f.showIf.key] === f.showIf.equals;
}
