import { create } from "zustand";
import { LazyStore } from "@tauri-apps/plugin-store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { enable as enableAutostart, disable as disableAutostart } from "@tauri-apps/plugin-autostart";
import { clampToBounds } from "./layout";
import { WIDGETS, widgetById } from "../widgets/registry";
import { defaultsOf, type WidgetSettings } from "../widgets/types";

export type ThemeMode = "translucent" | "solid";
/** 글자·표면 색 계열. "auto" 는 OS 테마를 따라간다. */
export type Palette = "dark" | "light" | "auto";
/** 카드의 테두리·그림자 프리셋. */
export type CardStyle = "glass" | "minimal" | "borderless" | "none";

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
  palette: Palette;
  cardStyle: CardStyle;
  /** 카드 표면의 불투명도 0~100. 낮출수록 뒤의 블러된 배경화면이 비친다. */
  surfaceOpacity: number;
  /**
   * 배경화면 블러 패스 수 1~6. **0 = 끔.** 기본 3 —
   * 캡처를 GDI 안에서 줄여 받고 안 바뀌면 아무 일도 하지 않게 되어(`providers/wallpaper`)
   * 상시로 켜 둘 만큼 싸졌다. 0 으로 내리면 백엔드 캡처 루프까지 멈춘다.
   */
  blurStrength: number;
  /** 카드 모서리 반경 px. */
  cornerRadius: number;
  /**
   * 카드 테두리 진하기 0~100. 기본 65 — 배경화면 위에서 카드 경계가 묻히지 않는 값이다.
   * 0 이면 거의 안 보이는 실선, 100 이면 편집 모드 링에 가까울 만큼 또렷하다.
   */
  borderStrength: number;
  /** 카드 테두리 두께 px 1~6. 진하기와 따로 둔다 — 두께와 진하기는 다른 축이다. */
  borderWidth: number;
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
  /** 편집 모드에서 고른 위젯들 (여러 개 동시 이동). 저장 안 함 — 잠그면 비운다. */
  selection: string[];
  /** 위젯 사각형 밖으로 펼쳐지는 팝업(폴더 등)의 히트 영역. 저장 안 함. */
  overlayRects: Record<string, Rect>;
  /** 배경화면을 어디서 얻었는지 — 설정 패널에 보여준다. 저장 안 함. */
  wallpaperSource: string | null;
  wallpaperError: string | null;

  load(): Promise<void>;
  setThemeMode(m: ThemeMode): void;
  toggleTheme(): void;
  setPalette(p: Palette): void;
  setCardStyle(c: CardStyle): void;
  setSurfaceOpacity(v: number): void;
  setBlurStrength(v: number): void;
  setCornerRadius(v: number): void;
  setBorderStrength(v: number): void;
  setBorderWidth(v: number): void;
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
  /** 여러 위젯을 한 번에 옮긴다 (마키 선택 후 드래그). */
  moveMany(moves: { id: string; x: number; y: number }[]): void;
  clampAll(bounds: { w: number; h: number }): void;
  setSelection(ids: string[]): void;
  toggleSelected(id: string): void;
  clearSelection(): void;
  updateWidgetSettings(instanceId: string, patch: WidgetSettings): void;
  setInstanceAccent(instanceId: string, accent: string | null): void;
  setOverlayRect(id: string, rect: Rect | null): void;
  setWallpaperStatus(source: string | null, error: string | null): void;
}

const store = new LazyStore("settings.json");
const KEY = "v1";

const prefersDark = () =>
  typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

/**
 * "auto" 를 실제 팔레트로 푼다.
 * `systemPrefersDark` 는 테스트에서 주입한다 — 생략하면 OS 설정을 읽고,
 * matchMedia 가 없는 환경(SSR·테스트)에서는 다크로 본다.
 */
export const resolvePalette = (
  p: Palette,
  systemPrefersDark: boolean = prefersDark()?.matches !== false,
): "dark" | "light" => (p === "auto" ? (systemPrefersDark ? "dark" : "light") : p);

type ThemeBits = Pick<
  Persisted,
  | "themeMode" | "palette" | "cardStyle" | "accent"
  | "surfaceOpacity" | "cornerRadius" | "borderStrength" | "borderWidth"
>;

function applyTheme(s: ThemeBits) {
  const d = document.documentElement;
  d.dataset.themeMode = s.themeMode;
  d.dataset.palette = resolvePalette(s.palette);
  d.dataset.cardStyle = s.cardStyle;
  d.style.setProperty("--accent", s.accent);
  d.style.setProperty("--surface-alpha", String(clamp(s.surfaceOpacity, 0, 100) / 100));
  d.style.setProperty("--radius", `${clamp(s.cornerRadius, 0, 40)}px`);
  d.style.setProperty("--border-k", String(clamp(s.borderStrength, 0, 100) / 100));
  d.style.setProperty("--border-w", `${clamp(s.borderWidth, 1, 6)}px`);
}

const clamp = (v: number, lo: number, hi: number) =>
  Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : lo;

// palette 가 "auto" 일 때 OS 테마 변경을 따라간다.
if (typeof window !== "undefined") {
  prefersDark()?.addEventListener("change", () => {
    const s = useSettings.getState();
    if (s.loaded && s.palette === "auto") applyTheme(s);
  });
}

let saveTimer: number | undefined;
let pending: (() => State) | null = null;
async function flush() {
  if (!pending) return;
  const s = pending();
  pending = null;
  const data: Persisted = {
    themeMode: s.themeMode, palette: s.palette, cardStyle: s.cardStyle,
    surfaceOpacity: s.surfaceOpacity, blurStrength: s.blurStrength, cornerRadius: s.cornerRadius,
    borderStrength: s.borderStrength, borderWidth: s.borderWidth,
    accent: s.accent, gridSnap: s.gridSnap, autoScale: s.autoScale,
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
export function migrate(widgetId: string, saved: WidgetSettings, instanceId = ""): WidgetSettings {
  const out = { ...saved };
  // 시계 v1 기본값(dots + long)은 블록 스타일 도입 전 값 → 새 기본으로
  if (widgetId === "clock" && !("blockColor" in out)) {
    if (out.digitStyle === "dots") out.digitStyle = "blocks";
    if (out.dateFormat === "long") out.dateFormat = "mono";
  }
  // 시계 블록 색 옛 하드코딩 기본값("#4fd1c5") → 빈 값(전역/위젯 강조색 사용)
  if (widgetId === "clock" && out.blockColor === "#4fd1c5") out.blockColor = "";
  // GitHub v1 의 "CI 상태"(showChecks)는 저장소 줄로 흡수됐다 — 꺼 뒀던 사람이
  // 갑자기 목록을 보게 되지 않도록 그 뜻을 새 키로 옮긴다.
  if (widgetId === "github" && "showChecks" in out && !("showRepos" in out)) out.showRepos = out.showChecks;
  // 폴더 v1 은 경로 칸 하나였다 ("비우면 자동 생성"). 이제 두 모드가 나뉘었으므로 저장된
  // 경로가 무엇이었는지로 가른다 — 전용 폴더의 경로는 `.../folders/<인스턴스 id>` 로 끝난다.
  // 그 경우 `dir` 은 지운다. 남겨 두면 다른 PC 에서 남의 계정 경로를 연결하려 든다.
  if (widgetId === "folder" && !("source" in out)) {
    const dir = String(out.dir ?? "").trim();
    const managed = !dir
      || (!!instanceId && dir.replace(/\\/g, "/").toLowerCase().endsWith(`/folders/${instanceId.toLowerCase()}`));
    out.source = managed ? "managed" : "link";
    if (managed) out.dir = "";
  }
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
  palette: "dark",
  cardStyle: "glass",
  surfaceOpacity: 62,
  blurStrength: 3,
  cornerRadius: 16,
  borderStrength: 65,
  borderWidth: 1,
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
  selection: [],
  overlayRects: {},
  wallpaperSource: null,
  wallpaperError: null,

  async load() {
    const saved = await store.get<Partial<Persisted>>(KEY);
    const s: Persisted = {
      themeMode: "translucent", palette: "dark", cardStyle: "glass",
      surfaceOpacity: 62, blurStrength: 3, cornerRadius: 16, borderStrength: 65, borderWidth: 1,
      accent: "#7c9cff", gridSnap: 8, autoScale: true, canvasMonitor: null, autostart: true,
      ...saved,
      instances: saved?.instances ?? defaultInstances(),
    };
    // 레지스트리에서 사라진 위젯은 버리고, 스키마 기본값은 채운다.
    const known = s.instances
      .filter((i) => widgetById(i.widgetId))
      .map((i) => ({ ...i, settings: { ...defaultsOf(widgetById(i.widgetId)!.settingsSchema), ...migrate(i.widgetId, i.settings, i.id) } }));
    const singletoned = ensureSingletons(known);
    const normalized = normalizeSettingsSize(singletoned);
    // 다른 해상도·배율·모니터에서 저장된 배치는 화면 밖에 남을 수 있다 — 보이는 자리로 접는다.
    //
    // **접은 결과를 그 자리에서 저장하지는 않는다.** 이 시점의 `window.innerWidth` 가 아직
    // 작업영역 크기가 아닐 수 있다 (백엔드 `fit_to_work_area` 가 실패했거나 늦은 경우 창은
    // `tauri.conf.json` 의 1100×700 이다). 그 크기로 접어 저장해 버리면 넓은 화면에서 짜 둔
    // 배치가 한 번에 뭉개지고 되돌릴 수 없다. 화면에 보이는 것만 고치고, 디스크에는
    // 사용자가 실제로 무언가를 옮겼을 때 함께 적힌다.
    set({ ...s, loaded: true });
    applyTheme(s);
    invoke("set_canvas_monitor", { name: s.canvasMonitor }).catch(console.warn);
    // 자동 시작 등록은 백엔드(autostart.rs::sync)가 시작 시 `autostart` 값에 맞춰 처리한다.
    const resized = normalized.some((i, idx) => i.w !== singletoned[idx].w || i.h !== singletoned[idx].h);
    if (!saved || s.instances.length !== known.length || resized) persist(get);
  },
  setThemeMode(themeMode) { set({ themeMode }); applyTheme(get()); persist(get); },
  toggleTheme() { get().setThemeMode(get().themeMode === "solid" ? "translucent" : "solid"); },
  setPalette(palette) { set({ palette }); applyTheme(get()); persist(get); },
  setCardStyle(cardStyle) { set({ cardStyle }); applyTheme(get()); persist(get); },
  setSurfaceOpacity(surfaceOpacity) { set({ surfaceOpacity }); applyTheme(get()); persist(get); },
  setBlurStrength(blurStrength) { set({ blurStrength }); persist(get); },
  setCornerRadius(cornerRadius) { set({ cornerRadius }); applyTheme(get()); persist(get); },
  setBorderStrength(borderStrength) { set({ borderStrength }); applyTheme(get()); persist(get); },
  setBorderWidth(borderWidth) { set({ borderWidth }); applyTheme(get()); persist(get); },
  setAccent(accent) { set({ accent }); applyTheme(get()); persist(get); },
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
  // 잠그면 선택은 의미가 없다 — 다음 편집이 항상 빈 선택에서 시작하도록 비운다.
  setLocked(locked) { set(locked ? { locked, selection: [] } : { locked }); },
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
    set({
      instances: get().instances.filter((i) => i.id !== instanceId),
      selected: null,
      selection: get().selection.filter((id) => id !== instanceId),
    });
    persist(get);
  },
  resetLayout() {
    set({ instances: defaultInstances(), selected: null, selection: [] });
    persist(get);
  },
  moveResize(instanceId, rect) {
    set({ instances: get().instances.map((i) => (i.id === instanceId ? { ...i, ...rect } : i)) });
    persist(get);
  },
  moveMany(moves) {
    if (moves.length === 0) return;
    const by = new Map(moves.map((m) => [m.id, m]));
    set({ instances: get().instances.map((i) => { const m = by.get(i.id); return m ? { ...i, x: m.x, y: m.y } : i; }) });
    persist(get);
  },
  /**
   * 창 크기가 바뀌었다 — 화면 밖으로 나간 위젯을 안으로 접는다 (모니터 변경·해상도 변경).
   *
   * `load` 와 같은 이유로 **저장하지 않는다**: 잠깐 작아진 창에 맞춰 저장해 버리면 원래
   * 배치로 돌아갈 길이 없어진다. 사용자가 다음에 무언가를 옮길 때 함께 적힌다.
   */
  clampAll(bounds) {
    const cur = get().instances;
    const next = clampToBounds(cur, bounds);
    if (next.every((i, idx) => i === cur[idx])) return;
    set({ instances: next });
  },
  setSelection(selection) { set({ selection }); },
  toggleSelected(id) {
    const cur = get().selection;
    set({ selection: cur.includes(id) ? cur.filter((s) => s !== id) : [...cur, id] });
  },
  clearSelection() { if (get().selection.length) set({ selection: [] }); },
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
  setWallpaperStatus(wallpaperSource, wallpaperError) { set({ wallpaperSource, wallpaperError }); },
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
