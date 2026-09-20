import { create } from "zustand";
import { LazyStore } from "@tauri-apps/plugin-store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { enable as enableAutostart, disable as disableAutostart } from "@tauri-apps/plugin-autostart";
import { WIDGETS, widgetById } from "../widgets/registry";
import { defaultsOf, type WidgetSettings } from "../widgets/types";

export type ThemeMode = "translucent" | "solid";

export interface WidgetInstance {
  id: string;        // 인스턴스 id (uuid)
  widgetId: string;  // WidgetDefinition.id
  x: number; y: number; w: number; h: number; // px (창 좌표 = 작업영역 좌표)
  settings: WidgetSettings;
  /** 인스턴스별 강조색 덮어쓰기. 비어있으면 전역 accent 사용. */
  accent?: string;
}

interface Persisted {
  themeMode: ThemeMode;
  accent: string;
  gridSnap: number;
  /** 위젯 내용을 크기에 맞춰 확대/축소 */
  autoScale: boolean;
  /** 캔버스로 쓸 모니터 장치명. null = 주 모니터 */
  canvasMonitor: string | null;
  /** Windows 시작 시 실행. 기본 true — 사용자가 끄면 false 로 기억한다. */
  autostart: boolean;
  instances: WidgetInstance[];
}

interface State extends Persisted {
  loaded: boolean;
  locked: boolean;           // 편집 잠금 (저장 안 함, 시작 시 항상 잠김)
  settingsOpen: boolean;
  selected: string | null;   // 설정 패널에서 보는 인스턴스
  /** 위젯 사각형 밖으로 펼쳐지는 팝업(폴더 등)의 히트 영역. 저장 안 함. */
  overlayRects: Record<string, Rect>;

  load(): Promise<void>;
  setThemeMode(m: ThemeMode): void;
  toggleTheme(): void;
  setAccent(c: string): void;
  setAutoScale(v: boolean): void;
  setCanvasMonitor(name: string | null): void;
  setAutostart(v: boolean): Promise<void>;
  setLocked(v: boolean): void;
  openSettings(instanceId?: string | null): void;
  closeSettings(): void;
  addWidget(widgetId: string): void;
  removeWidget(instanceId: string): void;
  resetLayout(): void;
  moveResize(instanceId: string, rect: Partial<Pick<WidgetInstance, "x" | "y" | "w" | "h">>): void;
  updateWidgetSettings(instanceId: string, patch: WidgetSettings): void;
  setInstanceAccent(instanceId: string, accent: string | null): void;
  setOverlayRect(id: string, rect: Rect | null): void;
}

const store = new LazyStore("settings.json");
const KEY = "v1";

function applyTheme(mode: ThemeMode, accent: string) {
  document.documentElement.dataset.themeMode = mode;
  document.documentElement.style.setProperty("--accent", accent);
}

let saveTimer: number | undefined;
let pending: (() => State) | null = null;
async function flush() {
  if (!pending) return;
  const s = pending();
  pending = null;
  const data: Persisted = {
    themeMode: s.themeMode, accent: s.accent, gridSnap: s.gridSnap, autoScale: s.autoScale,
    canvasMonitor: s.canvasMonitor, autostart: s.autostart, instances: s.instances,
  };
  await store.set(KEY, data);
  await store.save();
}
/** 모든 변경은 디스크에 저장된다 (150ms 디바운스, 창이 닫히거나 숨겨질 때는 즉시). */
function persist(get: () => State) {
  pending = get;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => { flush().catch(console.warn); }, 150);
}
if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", () => { window.clearTimeout(saveTimer); flush().catch(console.warn); });
  // 트레이 "종료" — 백엔드가 400ms 뒤 프로세스를 끝내므로 그 전에 즉시 저장
  listen("ui://quit", () => { window.clearTimeout(saveTimer); flush().catch(console.warn); }).catch(() => {});
  document.addEventListener("visibilitychange", () => { if (document.hidden) { window.clearTimeout(saveTimer); flush().catch(console.warn); } });
}

type R = { x: number; y: number; w: number; h: number };
export type Rect = R;
export const overlaps = (a: R, b: R) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/** 기존 위젯과 겹치지 않는 첫 자리 (좌→우, 상→하, 40px 스텝). 못 찾으면 (24,24). */
export function findFreeSlot(existing: R[], w: number, h: number, bounds = { w: window.innerWidth || 1920, h: window.innerHeight || 1040 }): { x: number; y: number } {
  const STEP = 40, M = 24;
  for (let y = M; y + h <= bounds.h - M; y += STEP)
    for (let x = M; x + w <= bounds.w - M; x += STEP)
      if (!existing.some((e) => overlaps({ x, y, w, h }, e))) return { x, y };
  return { x: M, y: M };
}

/** 구버전 저장값 마이그레이션 (사용자가 직접 고르지 않은 옛 기본값만 바꾼다). */
export function migrate(widgetId: string, saved: WidgetSettings): WidgetSettings {
  const out = { ...saved };
  // 시계 v1 기본값(dots + long)은 블록 스타일 도입 전 값 → 새 기본으로
  if (widgetId === "clock" && !("blockColor" in out)) {
    if (out.digitStyle === "dots") out.digitStyle = "blocks";
    if (out.dateFormat === "long") out.dateFormat = "mono";
  }
  // 시계 블록 색 옛 하드코딩 기본값("#4fd1c5") → 빈 값(전역/위젯 강조색 사용)
  if (widgetId === "clock" && out.blockColor === "#4fd1c5") out.blockColor = "";
  return out;
}

/** 설정 위젯은 톱니바퀴만 있는 정사각형이어야 한다 — 옛 저장값(직사각형)을 기본 크기로 보정한다. */
export function normalizeSettingsSize(instances: WidgetInstance[]): WidgetInstance[] {
  const def = widgetById("settings");
  if (!def) return instances;
  return instances.map((i) => {
    if (i.widgetId !== "settings" || i.w === i.h) return i;
    return { ...i, w: def.defaultSize.w, h: def.defaultSize.h };
  });
}

/** singleton 위젯(설정)이 없으면 빈 자리에 하나 추가한다 — 제목줄이 없어 설정 진입점이 반드시 있어야 한다. */
export function ensureSingletons(instances: WidgetInstance[]): WidgetInstance[] {
  const out = [...instances];
  for (const def of WIDGETS) {
    if (!def.singleton || out.some((i) => i.widgetId === def.id)) continue;
    const { w, h } = def.defaultSize;
    const { x, y } = findFreeSlot(out, w, h);
    out.push({ id: crypto.randomUUID(), widgetId: def.id, x, y, w, h, settings: defaultsOf(def.settingsSchema) });
  }
  return out;
}

export function defaultInstances(): WidgetInstance[] {
  const mk = (widgetId: string, x: number, y: number, w: number, h: number): WidgetInstance =>
    ({ id: crypto.randomUUID(), widgetId, x, y, w, h, settings: defaultsOf(widgetById(widgetId)?.settingsSchema) });
  return [
    mk("settings", 1104, 24, 56, 56),
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
  autostart: true,
  instances: [],
  loaded: false,
  locked: true,
  settingsOpen: false,
  selected: null,
  overlayRects: {},

  async load() {
    const saved = await store.get<Partial<Persisted>>(KEY);
    const s: Persisted = {
      themeMode: "translucent", accent: "#7c9cff", gridSnap: 8, autoScale: true, canvasMonitor: null, autostart: true,
      ...saved,
      instances: saved?.instances ?? defaultInstances(),
    };
    // 레지스트리에서 사라진 위젯은 버리고, 스키마 기본값은 채운다.
    const known = s.instances
      .filter((i) => widgetById(i.widgetId))
      .map((i) => ({ ...i, settings: { ...defaultsOf(widgetById(i.widgetId)!.settingsSchema), ...migrate(i.widgetId, i.settings) } }));
    const singletoned = ensureSingletons(known);
    s.instances = normalizeSettingsSize(singletoned);
    set({ ...s, loaded: true });
    applyTheme(s.themeMode, s.accent);
    invoke("set_canvas_monitor", { name: s.canvasMonitor }).catch(console.warn);
    // 자동 시작 등록은 백엔드(autostart.rs::sync)가 시작 시 `autostart` 값에 맞춰 처리한다.
    const resized = s.instances.some((i, idx) => i.w !== singletoned[idx].w || i.h !== singletoned[idx].h);
    if (!saved || s.instances.length !== known.length || resized) persist(get);
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
  async setAutostart(autostart) {
    set({ autostart });
    persist(get);
    if (import.meta.env.DEV) return;
    await (autostart ? enableAutostart() : disableAutostart());
  },
  setLocked(locked) { set({ locked }); },
  openSettings(instanceId = null) { set({ settingsOpen: true, selected: instanceId }); },
  closeSettings() { set({ settingsOpen: false, selected: null }); },
  addWidget(widgetId) {
    const def = widgetById(widgetId);
    if (!def) return;
    if (def.singleton && get().instances.some((i) => i.widgetId === widgetId)) return;
    const { w, h } = def.defaultSize;
    const { x, y } = findFreeSlot(get().instances, w, h);
    const inst: WidgetInstance = { id: crypto.randomUUID(), widgetId, x, y, w, h, settings: defaultsOf(def.settingsSchema) };
    set({ instances: [...get().instances, inst], locked: false });
    persist(get);
  },
  removeWidget(instanceId) {
    const inst = get().instances.find((i) => i.id === instanceId);
    if (inst && widgetById(inst.widgetId)?.singleton) return;
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
  setInstanceAccent(instanceId, accent) {
    set({
      instances: get().instances.map((i) => {
        if (i.id !== instanceId) return i;
        if (!accent) { const { accent: _drop, ...rest } = i; return rest; }
        return { ...i, accent };
      }),
    });
    persist(get);
  },
  setOverlayRect(id, rect) {
    const next = { ...get().overlayRects };
    if (rect) next[id] = rect; else delete next[id];
    set({ overlayRects: next });
  },
}));

/** 위젯 크기에 따른 내용 배율. autoScale 이 꺼져 있으면 1. */
export function contentScale(inst: WidgetInstance, autoScale: boolean): number {
  const def = widgetById(inst.widgetId);
  if (!autoScale || !def) return 1;
  const s = Math.min(inst.w / def.defaultSize.w, inst.h / def.defaultSize.h);
  return Math.max(0.6, Math.min(2.5, Math.round(s * 100) / 100));
}
