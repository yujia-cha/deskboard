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
    /** 슬라이더. 움직이는 즉시 저장·반영되므로 물리 값처럼 "만지며 맞추는" 값에 쓴다. */
    | { type: "range"; default: number; min: number; max: number; step: number; unit?: string }
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

/** 위젯 폴더가 어느 루트에서 왔는가 — 앱과 함께 깔린 것(builtin) / 사용자가 만든 것(user). */
export type WidgetRoot = "builtin" | "user" | "shell";

export interface WidgetDefinition<S extends WidgetSettings = WidgetSettings> {
  /** 고유 id = 위젯 폴더 이름. 인스턴스 설정 키로도 쓰인다. */
  id: string;
  title: string;
  icon?: string;
  description?: string;
  component: ComponentType<WidgetProps<S>>;
  defaultSize: { w: number; h: number };
  minSize: { w: number; h: number };
  /** 선언하면 설정 패널이 자동 생성된다. */
  settingsSchema?: SettingField[];
  /** 항상 정확히 하나만 존재하고 제거할 수 없다 (셸의 설정 위젯 전용). */
  singleton?: boolean;
  /** 옛 저장값을 새 형태로 옮긴다. 위젯 모듈이 `export function migrate` 로 내보낸다. */
  migrate?: (saved: WidgetSettings, instanceId: string) => WidgetSettings;
  root: WidgetRoot;
  /** module = 셸 안에서 도는 React 컴포넌트, iframe = 격리된 HTML 페이지. 셸 위젯은 없음. */
  kind?: "module" | "iframe";
  /** 폴더 내용의 지문 — 바뀌면 위젯을 다시 마운트한다. */
  version?: string;
  /** `widget.json` 에 `command` 가 있다 — 셸이 인스턴스마다 실행을 켜고 끈다. */
  hasCommand?: boolean;
  /** iframe 위젯이 받을 수 있는 백엔드 이벤트 (허용 목록과 교집합). */
  subscribe?: string[];
  /** 사용자 폴더가 같은 이름의 내장 위젯을 덮어쓰고 있다. */
  overridesBuiltin?: boolean;
  /** 위젯 추가 목록에서 숨긴다 (`config.jsonc` 의 `widgets.<id>.hidden`). */
  hidden?: boolean;
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

/**
 * 설정 스키마를 검증한다. 잘못된 줄은 버리고 이유를 돌려준다 — 사용자가 손으로 쓴
 * `widget.json` 이 한 줄 틀렸다고 위젯 전체를 못 쓰게 만들지 않는다.
 */
export function validateSchema(raw: unknown): { schema: SettingField[]; warnings: string[] } {
  const warnings: string[] = [];
  if (raw == null) return { schema: [], warnings };
  if (!Array.isArray(raw)) return { schema: [], warnings: ["settingsSchema 는 배열이어야 합니다"] };
  const seen = new Set<string>();
  const schema: SettingField[] = [];
  raw.forEach((f: Record<string, unknown>, i) => {
    const where = `settingsSchema[${i}]`;
    if (!f || typeof f !== "object") { warnings.push(`${where}: 객체가 아닙니다`); return; }
    const key = f.key, type = f.type;
    if (typeof key !== "string" || !key) { warnings.push(`${where}: key 가 없습니다`); return; }
    if (seen.has(key)) { warnings.push(`${where}: key "${key}" 가 중복됩니다`); return; }
    if (typeof f.label !== "string") { warnings.push(`${where} (${key}): label 이 없습니다`); return; }
    const bad = (msg: string) => warnings.push(`${where} (${key}): ${msg}`);
    switch (type) {
      case "note": break;
      case "boolean": if (typeof f.default !== "boolean") return bad("default 는 true/false 여야 합니다"); break;
      case "number": if (typeof f.default !== "number") return bad("default 는 숫자여야 합니다"); break;
      case "range": {
        const n = (v: unknown) => typeof v === "number" && Number.isFinite(v);
        if (!n(f.default) || !n(f.min) || !n(f.max) || !n(f.step)) return bad("default·min·max·step 이 모두 숫자여야 합니다");
        const [d, lo, hi] = [f.default as number, f.min as number, f.max as number];
        if (lo > hi || d < lo || d > hi) return bad("default 는 min 과 max 사이여야 합니다");
        if (f.unit !== undefined && typeof f.unit !== "string") return bad("unit 은 문자열이어야 합니다");
        break;
      }
      case "text": if (typeof f.default !== "string") return bad("default 는 문자열이어야 합니다"); break;
      case "path":
        if (typeof f.default !== "string") return bad("default 는 문자열이어야 합니다");
        if (f.pick !== "image" && f.pick !== "directory") return bad('pick 은 "image" 또는 "directory" 여야 합니다');
        break;
      case "select": {
        const opts = f.options;
        if (!Array.isArray(opts) || !opts.every((o) => o && typeof o.value === "string" && typeof o.label === "string"))
          return bad("options 는 {value, label} 배열이어야 합니다");
        if (!opts.some((o) => o.value === f.default)) return bad("default 가 options 안에 없습니다");
        break;
      }
      default: return bad(`알 수 없는 type "${String(type)}"`);
    }
    seen.add(key);
    schema.push(f as unknown as SettingField);
  });
  return { schema, warnings };
}
