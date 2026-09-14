import { create } from "zustand";
import { LazyStore } from "@tauri-apps/plugin-store";
import { invoke } from "@tauri-apps/api/core";
import { widgetById } from "../widgets/registry";
import { defaultsOf, type WidgetSettings } from "../widgets/types";

export type ThemeMode = "translucent" | "solid";

export interface WidgetInstance {
  id: string;        // 인스턴스 id (uuid)
  widgetId: string;  // WidgetDefinition.id
  x: number; y: number; w: number; h: number; // px (창 좌표)
  settings: WidgetSettings;
}

interface Persisted {
  themeMode: ThemeMode;
  accent: string;
  gridSnap: number;
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
  setLocked(v: boolean): void;
  openSettings(instanceId?: string | null): void;
  closeSettings(): void;
  addWidget(widgetId: string): void;
  removeWidget(instanceId: string): void;
  moveResize(instanceId: string, rect: Partial<Pick<WidgetInstance, "x" | "y" | "w" | "h">>): void;
  updateWidgetSettings(instanceId: string, patch: WidgetSettings): void;
}

const store = new LazyStore("settings.json");
const KEY = "v1";

function applyTheme(mode: ThemeMode, accent: string) {
  document.documentElement.dataset.themeMode = mode;
  document.documentElement.style.setProperty("--accent", accent);
  invoke("set_theme_mode", { mode }).catch(console.warn);
}

let saveTimer: number | undefined;
function persist(get: () => State) {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    const s = get();
    const data: Persisted = {
      themeMode: s.themeMode, accent: s.accent, gridSnap: s.gridSnap, instances: s.instances,
    };
    await store.set(KEY, data);
    await store.save();
  }, 300);
}

function defaultInstances(): WidgetInstance[] {
  return [
    { id: crypto.randomUUID(), widgetId: "clock", x: 24, y: 24, w: 300, h: 140, settings: defaultsOf(widgetById("clock")?.settingsSchema) },
    { id: crypto.randomUUID(), widgetId: "sysmon", x: 24, y: 184, w: 360, h: 240, settings: defaultsOf(widgetById("sysmon")?.settingsSchema) },
    { id: crypto.randomUUID(), widgetId: "claude-usage", x: 344, y: 24, w: 340, h: 260, settings: defaultsOf(widgetById("claude-usage")?.settingsSchema) },
  ];
}

export const useSettings = create<State>((set, get) => ({
  themeMode: "translucent",
  accent: "#7c9cff",
  gridSnap: 8,
  instances: [],
  loaded: false,
  locked: true,
  settingsOpen: false,
  selected: null,

  async load() {
    const saved = await store.get<Persisted>(KEY);
    const s: Persisted = saved ?? {
      themeMode: "translucent", accent: "#7c9cff", gridSnap: 8, instances: defaultInstances(),
    };
    // 레지스트리에서 사라진 위젯은 버리고, 스키마 기본값은 채운다.
    s.instances = s.instances
      .filter((i) => widgetById(i.widgetId))
      .map((i) => ({ ...i, settings: { ...defaultsOf(widgetById(i.widgetId)!.settingsSchema), ...i.settings } }));
    set({ ...s, loaded: true });
    applyTheme(s.themeMode, s.accent);
    if (!saved) persist(get);
  },
  setThemeMode(themeMode) { set({ themeMode }); applyTheme(themeMode, get().accent); persist(get); },
  toggleTheme() { get().setThemeMode(get().themeMode === "solid" ? "translucent" : "solid"); },
  setAccent(accent) { set({ accent }); applyTheme(get().themeMode, accent); persist(get); },
  setLocked(locked) { set({ locked }); },
  openSettings(instanceId = null) { set({ settingsOpen: true, selected: instanceId }); },
  closeSettings() { set({ settingsOpen: false, selected: null }); },
  addWidget(widgetId) {
    const def = widgetById(widgetId);
    if (!def) return;
    const n = get().instances.length;
    const inst: WidgetInstance = {
      id: crypto.randomUUID(), widgetId,
      x: 24 + (n % 4) * 40, y: 24 + (n % 4) * 40,
      w: def.defaultSize.w, h: def.defaultSize.h,
      settings: defaultsOf(def.settingsSchema),
    };
    set({ instances: [...get().instances, inst], locked: false });
    persist(get);
  },
  removeWidget(instanceId) {
    set({ instances: get().instances.filter((i) => i.id !== instanceId), selected: null });
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
