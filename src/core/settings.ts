import { create } from "zustand";
import { LazyStore } from "@tauri-apps/plugin-store";
import { invoke } from "@tauri-apps/api/core";
import { widgetById } from "../widgets/registry";
import { defaultsOf, type WidgetSettings } from "../widgets/types";

export type ThemeMode = "translucent" | "solid";

export interface WidgetInstance {
  id: string;        // 인스턴스 id (uuid)
  widgetId: string;  // WidgetDefinition.id
  x: number; y: number; w: number; h: number; // px (창 좌표 = 작업영역 좌표)
  settings: WidgetSettings;
}

interface Persisted {
  themeMode: ThemeMode;
  accent: string;
  gridSnap: number;
  /** 위젯 내용을 크기에 맞춰 확대/축소 */
  autoScale: boolean;
  /** 캔버스로 쓸 모니터 장치명. null = 주 모니터 */
  canvasMonitor: string | null;
  instances: WidgetInstance[];
}

interface State extends Persisted {
  loaded: boolean;
  locked: boolean;           // 편집 잠금 (저장 안 함, 시작 시 항상 잠김)
  settingsOpen: boolean;
  selected: string | null;   // 설정 패널에서 보는 인스턴스

  load(): Promise<void>;
  setThemeMode(m: ThemeMode): void;
  toggleTheme(): void;
  setAccent(c: string): void;
  setAutoScale(v: boolean): void;
  setCanvasMonitor(name: string | null): void;
  setLocked(v: boolean): void;
  openSettings(instanceId?: string | null): void;
  closeSettings(): void;
  addWidget(widgetId: string): void;
  removeWidget(instanceId: string): void;
  resetLayout(): void;
  moveResize(instanceId: string, rect: Partial<Pick<WidgetInstance, "x" | "y" | "w" | "h">>): void;
  updateWidgetSettings(instanceId: string, patch: WidgetSettings): void;
}

const store = new LazyStore("settings.json");
const KEY = "v1";

function applyTheme(mode: ThemeMode, accent: string) {
  document.documentElement.dataset.themeMode = mode;
  document.documentElement.style.setProperty("--accent", accent);
}

let saveTimer: number | undefined;
function persist(get: () => State) {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    const s = get();
    const data: Persisted = {
      themeMode: s.themeMode, accent: s.accent, gridSnap: s.gridSnap, autoScale: s.autoScale,
      canvasMonitor: s.canvasMonitor, instances: s.instances,
    };
    await store.set(KEY, data);
    await store.save();
  }, 300);
}

type R = { x: number; y: number; w: number; h: number };
export const overlaps = (a: R, b: R) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/** 기존 위젯과 겹치지 않는 첫 자리 (좌→우, 상→하, 40px 스텝). 못 찾으면 (24,24). */
export function findFreeSlot(existing: R[], w: number, h: number, bounds = { w: window.innerWidth || 1920, h: window.innerHeight || 1040 }): { x: number; y: number } {
  const STEP = 40, M = 24;
  for (let y = M; y + h <= bounds.h - M; y += STEP)
    for (let x = M; x + w <= bounds.w - M; x += STEP)
      if (!existing.some((e) => overlaps({ x, y, w, h }, e))) return { x, y };
  return { x: M, y: M };
}

export function defaultInstances(): WidgetInstance[] {
  const mk = (widgetId: string, x: number, y: number, w: number, h: number): WidgetInstance =>
    ({ id: crypto.randomUUID(), widgetId, x, y, w, h, settings: defaultsOf(widgetById(widgetId)?.settingsSchema) });
  return [
    mk("clock", 24, 24, 300, 140),
    mk("sysmon", 24, 184, 360, 240),
    mk("claude-usage", 404, 24, 340, 150),
    mk("spotify", 404, 194, 340, 320),
    mk("calendar", 764, 24, 320, 400),
  ];
}

export const useSettings = create<State>((set, get) => ({
  themeMode: "translucent",
  accent: "#7c9cff",
  gridSnap: 8,
  autoScale: true,
  canvasMonitor: null,
  instances: [],
  loaded: false,
  locked: true,
  settingsOpen: false,
  selected: null,

  async load() {
    const saved = await store.get<Partial<Persisted>>(KEY);
    const s: Persisted = {
      themeMode: "translucent", accent: "#7c9cff", gridSnap: 8, autoScale: true, canvasMonitor: null,
      ...saved,
      instances: saved?.instances ?? defaultInstances(),
    };
    // 레지스트리에서 사라진 위젯은 버리고, 스키마 기본값은 채운다.
    s.instances = s.instances
      .filter((i) => widgetById(i.widgetId))
      .map((i) => ({ ...i, settings: { ...defaultsOf(widgetById(i.widgetId)!.settingsSchema), ...i.settings } }));
    set({ ...s, loaded: true });
    applyTheme(s.themeMode, s.accent);
    invoke("set_canvas_monitor", { name: s.canvasMonitor }).catch(console.warn);
    if (!saved) persist(get);
  },
  setThemeMode(themeMode) { set({ themeMode }); applyTheme(themeMode, get().accent); persist(get); },
  toggleTheme() { get().setThemeMode(get().themeMode === "solid" ? "translucent" : "solid"); },
  setAccent(accent) { set({ accent }); applyTheme(get().themeMode, accent); persist(get); },
  setAutoScale(autoScale) { set({ autoScale }); persist(get); },
  setCanvasMonitor(canvasMonitor) {
    set({ canvasMonitor });
    invoke("set_canvas_monitor", { name: canvasMonitor }).catch(console.warn);
    persist(get);
  },
  setLocked(locked) { set({ locked }); },
  openSettings(instanceId = null) { set({ settingsOpen: true, selected: instanceId }); },
  closeSettings() { set({ settingsOpen: false, selected: null }); },
  addWidget(widgetId) {
    const def = widgetById(widgetId);
    if (!def) return;
    const { w, h } = def.defaultSize;
    const { x, y } = findFreeSlot(get().instances, w, h);
    const inst: WidgetInstance = { id: crypto.randomUUID(), widgetId, x, y, w, h, settings: defaultsOf(def.settingsSchema) };
    set({ instances: [...get().instances, inst], locked: false });
    persist(get);
  },
  removeWidget(instanceId) {
    set({ instances: get().instances.filter((i) => i.id !== instanceId), selected: null });
    persist(get);
  },
  resetLayout() {
    set({ instances: defaultInstances(), selected: null });
    persist(get);
  },
  moveResize(instanceId, rect) {
    set({ instances: get().instances.map((i) => (i.id === instanceId ? { ...i, ...rect } : i)) });
    persist(get);
  },
  updateWidgetSettings(instanceId, patch) {
    set({ instances: get().instances.map((i) => (i.id === instanceId ? { ...i, settings: { ...i.settings, ...patch } } : i)) });
    persist(get);
  },
}));

/** 위젯 크기에 따른 내용 배율. autoScale 이 꺼져 있으면 1. */
export function contentScale(inst: WidgetInstance, autoScale: boolean): number {
  const def = widgetById(inst.widgetId);
  if (!autoScale || !def) return 1;
  const s = Math.min(inst.w / def.defaultSize.w, inst.h / def.defaultSize.h);
  return Math.max(0.6, Math.min(2.5, Math.round(s * 100) / 100));
}
