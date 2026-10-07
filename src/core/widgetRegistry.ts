import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createElement, type ComponentType } from "react";
import { useConfig, type WidgetOverride } from "./config";
import { validateSchema, type SettingField, type WidgetDefinition, type WidgetProps } from "./widgetTypes";
import { evaluateBundle, removeWidgetCss, type Bundle } from "./loader/modules";
import { hostModules } from "./loader/host";
import { SettingsLauncher } from "../shell/SettingsLauncher";
import { FrameWidget } from "../components/FrameWidget";

/**
 * 위젯 레지스트리 — **위젯 = 폴더 하나.**
 *
 * 백엔드(`userland/scan.rs`)가 두 루트를 훑는다: 앱과 함께 깔린 내장 위젯과
 * `%APPDATA%/com.user.deskboard/widgets` 의 사용자 위젯. 같은 이름이면 사용자 것이 이긴다.
 * 여기서는 그 목록을 받아 각 폴더의 코드를 불러(`loader/`) 정의로 만들고,
 * `config.jsonc` 의 `widgets.<id>` 덮어쓰기를 얹는다.
 *
 * 파일이 바뀌면 `widgets://changed` 가 오고, 지문(version)이 바뀐 위젯만 다시 불러온다.
 * 셸에 박혀 있는 위젯은 설정(톱니) 하나뿐이다 — 설정 진입점은 최소 동작에 속한다.
 */

/** 백엔드 `widgets_list` 의 한 줄. */
export interface WidgetInfo {
  id: string;
  root: "builtin" | "user";
  kind: "module" | "iframe" | null;
  entry: string | null;
  manifest: {
    title: string;
    icon?: string;
    description?: string;
    defaultSize: { w: number; h: number };
    minSize: { w: number; h: number };
    settingsSchema?: unknown;
    provider?: string;
    subscribe?: string[];
    hasCommand?: boolean;
  } | null;
  error: string | null;
  warnings: string[];
  version: string;
  overridesBuiltin: boolean;
  dir: string;
}

export interface RegistryEntry {
  id: string;
  info: WidgetInfo | null;
  status: "loading" | "ready" | "error";
  /** 덮어쓰기 전의 정의. 화면에 쓰는 것은 `defs` 쪽이다. */
  base: WidgetDefinition | null;
  error: string | null;
  warnings: string[];
}

interface RegistryState {
  entries: Record<string, RegistryEntry>;
  /** config 덮어쓰기까지 얹은 최종 정의. 준비된 위젯만 있다. */
  defs: Record<string, WidgetDefinition>;
  init(): Promise<void>;
}

const SHELL_WIDGETS: WidgetDefinition[] = [
  {
    id: "settings",
    title: "설정",
    icon: "⚙",
    component: SettingsLauncher,
    defaultSize: { w: 56, h: 56 },
    minSize: { w: 40, h: 40 },
    singleton: true,
    root: "shell",
  },
];

/** 위젯 모듈의 default export 가 React 컴포넌트로 쓸 수 있는가 (함수 또는 memo/forwardRef 객체). */
const isComponent = (v: unknown) =>
  typeof v === "function" || (typeof v === "object" && v !== null && "$$typeof" in v);

/** `config.jsonc` 의 `widgets.<id>` 를 정의에 얹는다. 맞지 않는 값은 버리고 이유를 돌려준다. */
export function applyOverrides(def: WidgetDefinition, o: WidgetOverride | undefined): { def: WidgetDefinition; warnings: string[] } {
  if (!o || typeof o !== "object") return { def, warnings: [] };
  const warnings: string[] = [];
  const where = `config.jsonc widgets.${def.id}`;
  const size = (v: unknown) =>
    !!v && typeof v === "object" && Number((v as { w: unknown }).w) > 0 && Number((v as { h: unknown }).h) > 0;
  const out: WidgetDefinition = { ...def };
  if (o.title !== undefined) { if (typeof o.title === "string" && o.title) out.title = o.title; else warnings.push(`${where}.title 은 문자열이어야 합니다`); }
  if (o.icon !== undefined) { if (typeof o.icon === "string") out.icon = o.icon; else warnings.push(`${where}.icon 은 문자열이어야 합니다`); }
  if (o.hidden !== undefined) { if (typeof o.hidden === "boolean") out.hidden = o.hidden; else warnings.push(`${where}.hidden 은 true/false 여야 합니다`); }
  if (o.defaultSize !== undefined) { if (size(o.defaultSize)) out.defaultSize = { w: o.defaultSize.w, h: o.defaultSize.h }; else warnings.push(`${where}.defaultSize 는 { "w": 양수, "h": 양수 } 여야 합니다`); }
  if (o.minSize !== undefined) { if (size(o.minSize)) out.minSize = { w: o.minSize.w, h: o.minSize.h }; else warnings.push(`${where}.minSize 는 { "w": 양수, "h": 양수 } 여야 합니다`); }
  if (out.minSize.w > out.defaultSize.w || out.minSize.h > out.defaultSize.h) {
    out.minSize = { w: Math.min(out.minSize.w, out.defaultSize.w), h: Math.min(out.minSize.h, out.defaultSize.h) };
    warnings.push(`${where}: minSize 가 defaultSize 보다 커서 맞췄습니다`);
  }
  if (o.settings !== undefined) {
    if (!o.settings || typeof o.settings !== "object") warnings.push(`${where}.settings 는 객체여야 합니다`);
    else {
      const schema = (def.settingsSchema ?? []).map((f) => ({ ...f })) as SettingField[];
      for (const [key, value] of Object.entries(o.settings)) {
        const f = schema.find((x) => x.key === key);
        if (!f || f.type === "note") { warnings.push(`${where}.settings.${key}: 이 위젯에 없는 설정입니다`); continue; }
        const ok =
          (f.type === "boolean" && typeof value === "boolean") ||
          (f.type === "number" && typeof value === "number" && Number.isFinite(value)) ||
          ((f.type === "text" || f.type === "path") && typeof value === "string") ||
          (f.type === "select" && f.options.some((opt) => opt.value === value));
        if (!ok) { warnings.push(`${where}.settings.${key}: 값의 형식이 맞지 않습니다`); continue; }
        (f as { default: unknown }).default = value;
      }
      out.settingsSchema = schema;
    }
  }
  return { def: out, warnings };
}

function buildDefs(entries: Record<string, RegistryEntry>): Record<string, WidgetDefinition> {
  const overrides = useConfig.getState().config.widgets;
  const defs: Record<string, WidgetDefinition> = {};
  for (const d of SHELL_WIDGETS) defs[d.id] = d;
  for (const e of Object.values(entries)) {
    if (e.status !== "ready" || !e.base) continue;
    defs[e.id] = applyOverrides(e.base, overrides[e.id]).def;
  }
  return defs;
}

/** 위젯 폴더 하나를 정의로 만든다. 실패는 throw — 호출한 쪽이 오류 항목으로 남긴다. */
async function loadDefinition(info: WidgetInfo): Promise<{ def: WidgetDefinition; warnings: string[] }> {
  if (info.error) throw new Error(info.error);
  // 셸 위젯(설정 톱니)은 덮어쓸 수 없다 — 덮이면 설정으로 들어갈 길이 사라진다.
  if (SHELL_WIDGETS.some((d) => d.id === info.id))
    throw new Error(`"${info.id}" 는 대시보드가 쓰는 이름이라 위젯 이름으로 쓸 수 없습니다 — 폴더 이름을 바꿔 주세요`);
  const m = info.manifest;
  if (!m || !info.kind) throw new Error("widget.json 을 읽지 못했습니다");
  const { schema, warnings } = validateSchema(m.settingsSchema);
  const base = {
    id: info.id,
    title: m.title,
    icon: m.icon,
    description: m.description,
    defaultSize: m.defaultSize,
    minSize: m.minSize,
    settingsSchema: schema,
    root: info.root,
    kind: info.kind,
    version: info.version,
    hasCommand: !!m.hasCommand,
    subscribe: m.subscribe ?? [],
    overridesBuiltin: info.overridesBuiltin,
  };
  if (info.kind === "iframe") {
    const component: ComponentType<WidgetProps> = (props) =>
      createElement(FrameWidget, { ...props, widgetId: info.id, entry: info.entry ?? "index.html", version: info.version, subscribe: base.subscribe });
    return { def: { ...base, component }, warnings };
  }
  const bundle = await invoke<Bundle>("widgets_bundle", { id: info.id });
  const exports = await evaluateBundle(info.id, bundle, hostModules());
  const component = exports.default;
  if (!isComponent(component))
    throw new Error(`${bundle.entry}: 컴포넌트를 default export 해야 합니다 (export default function MyWidget(props) { … })`);
  const migrate = typeof exports.migrate === "function" ? (exports.migrate as WidgetDefinition["migrate"]) : undefined;
  return { def: { ...base, component: component as ComponentType<WidgetProps>, migrate }, warnings };
}

/** 같은 위젯을 연달아 다시 불러올 때 늦게 끝난 옛 결과가 새 것을 덮지 않게 한다. */
const loadSeq = new Map<string, number>();

export const useRegistry = create<RegistryState>((set, get) => {
  const commit = (entries: Record<string, RegistryEntry>) => set({ entries, defs: buildDefs(entries) });

  const loadOne = async (info: WidgetInfo) => {
    const seq = (loadSeq.get(info.id) ?? 0) + 1;
    loadSeq.set(info.id, seq);
    let next: RegistryEntry;
    try {
      const { def, warnings } = await loadDefinition(info);
      next = { id: info.id, info, status: "ready", base: def, error: null, warnings: [...info.warnings, ...warnings] };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.warn(`위젯 ${info.id}:`, msg);
      next = { id: info.id, info, status: "error", base: null, error: msg, warnings: info.warnings };
    }
    if (loadSeq.get(info.id) !== seq) return;
    commit({ ...get().entries, [info.id]: next });
  };

  /** 새 목록을 반영한다 — 지문이 같은 위젯은 그대로 두고, 바뀐 것만 다시 불러온다. */
  const sync = async (list: WidgetInfo[]) => {
    const prev = get().entries;
    const next: Record<string, RegistryEntry> = {};
    const toLoad: WidgetInfo[] = [];
    for (const info of list) {
      const old = prev[info.id];
      if (old?.info && old.info.version === info.version && old.info.dir === info.dir && old.info.error === info.error) {
        next[info.id] = { ...old, info };
        continue;
      }
      next[info.id] = old ? { ...old, info } : { id: info.id, info, status: "loading", base: null, error: null, warnings: [] };
      toLoad.push(info);
    }
    for (const id of Object.keys(prev)) if (!(id in next)) { removeWidgetCss(id); loadSeq.delete(id); }
    commit(next);
    await Promise.all(toLoad.map(loadOne));
  };

  // config 덮어쓰기가 바뀌면 정의만 다시 계산한다 (코드를 다시 불러올 필요는 없다).
  useConfig.subscribe((s, p) => { if (s.config.widgets !== p.config.widgets) set({ defs: buildDefs(get().entries) }); });

  return {
    entries: {},
    defs: buildDefs({}),

    /** 절대 throw 하지 않는다 — 위젯을 하나도 못 불러도 셸(설정 위젯)은 뜬다. */
    async init() {
      listen<WidgetInfo[]>("widgets://changed", (e) => { sync(e.payload).catch(console.warn); }).catch(() => {});
      try {
        await sync(await invoke<WidgetInfo[]>("widgets_list"));
      } catch (e) {
        console.warn("widgets_list", e);
      }
    },
  };
});

/** 정의 조회 (비반응형 — 이벤트 핸들러·스토어 안에서 쓴다). */
export const widgetById = (id: string): WidgetDefinition | undefined => useRegistry.getState().defs[id];

/** 정의 조회 (반응형 — 컴포넌트에서 쓴다. 위젯을 다시 불러오면 새 정의로 다시 그린다). */
export const useWidgetDef = (id: string): WidgetDefinition | undefined => useRegistry((s) => s.defs[id]);

export const allWidgets = (): WidgetDefinition[] => Object.values(useRegistry.getState().defs);
