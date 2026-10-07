import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * 사용자가 손으로 고치는 셸 설정 — `%APPDATA%/com.user.deskboard/` 의 세 파일.
 *
 * - `config.jsonc`  : 기본값·배치·튜닝 상수·내장 위젯 덮어쓰기
 * - `strings.jsonc` : 트레이·편집 바 문구 (트레이는 백엔드가 읽는다)
 * - `user.css`      : theme.css 위에 덮는 스타일
 *
 * 파일이 없으면 기본값, 깨져 있으면 **무시하고 이유를 보여 준다** — 손으로 쓴 파일 한 줄이
 * 대시보드를 못 띄우게 해서는 안 된다. 읽기·JSONC 파싱·파일 감시는 백엔드(`userland/config.rs`)가
 * 하고, 여기서는 구조를 기본값에 맞춰 검증하고 합친다.
 */

export interface SizePreset { label: string; k: number }
export interface LayoutSlot { widget: string; x: number; y: number; w: number; h: number }
export interface WidgetOverride {
  title?: string;
  icon?: string;
  defaultSize?: { w: number; h: number };
  minSize?: { w: number; h: number };
  settings?: Record<string, unknown>;
  hidden?: boolean;
}

export interface ShellConfig {
  defaults: {
    themeMode: "translucent" | "solid";
    palette: "dark" | "light" | "auto";
    cardStyle: "glass" | "minimal" | "borderless" | "none";
    surfaceOpacity: number;
    blurStrength: number;
    cornerRadius: number;
    borderStrength: number;
    borderWidth: number;
    accent: string;
    gridSnap: number;
    autoScale: boolean;
  };
  layout: {
    sizePresets: SizePreset[];
    contentScale: { min: number; max: number };
    freeSlot: { step: number; margin: number };
    default: LayoutSlot[];
  };
  widgets: Record<string, WidgetOverride>;
  /** 백엔드 폴링 주기(초). 프론트는 쓰지 않지만 알려진 키로 검증만 한다. */
  intervals: Record<string, number>;
}

export interface ShellStrings {
  editBar: { hint: string; multi: string; settings: string; lock: string };
  /** 백엔드가 읽는다 — 여기서는 모르는 키 경고만 피하려고 둔다. */
  tray: Record<string, string>;
}

export const DEFAULT_CONFIG: ShellConfig = {
  defaults: {
    themeMode: "translucent", palette: "dark", cardStyle: "glass",
    surfaceOpacity: 62, blurStrength: 3, cornerRadius: 16, borderStrength: 65, borderWidth: 1,
    accent: "#7c9cff", gridSnap: 8, autoScale: true,
  },
  layout: {
    sizePresets: [{ label: "작게", k: 0.75 }, { label: "기본", k: 1 }, { label: "크게", k: 1.4 }, { label: "아주 크게", k: 1.8 }],
    contentScale: { min: 0.6, max: 2.5 },
    freeSlot: { step: 40, margin: 24 },
    default: [
      { widget: "settings", x: 1104, y: 24, w: 56, h: 56 },
      { widget: "clock", x: 24, y: 24, w: 300, h: 140 },
      { widget: "sysmon", x: 24, y: 184, w: 360, h: 240 },
      { widget: "claude-usage", x: 404, y: 24, w: 340, h: 150 },
      { widget: "spotify", x: 404, y: 194, w: 340, h: 320 },
      { widget: "calendar", x: 764, y: 24, w: 320, h: 400 },
    ],
  },
  widgets: {},
  intervals: {},
};

export const DEFAULT_STRINGS: ShellStrings = {
  editBar: {
    hint: "편집 모드 — 헤더 드래그: 이동 · 빈 곳 드래그: 여러 개 선택 · Ctrl/Shift 클릭: 추가 · 테두리·모서리 드래그: 크기",
    multi: "{n}개 선택 — 하나를 끌면 함께 움직입니다 · Esc: 선택 해제",
    settings: "⚙ 설정",
    lock: "🔒 잠금",
  },
  tray: {},
};

/** 값의 범위. 벗어나면 잘라 넣고 경고한다. */
const RANGES: Record<string, [number, number]> = {
  "defaults.surfaceOpacity": [10, 100],
  "defaults.blurStrength": [0, 6],
  "defaults.cornerRadius": [0, 40],
  "defaults.borderStrength": [0, 100],
  "defaults.borderWidth": [1, 6],
  "defaults.gridSnap": [1, 64],
  "layout.contentScale.min": [0.1, 1],
  "layout.contentScale.max": [1, 6],
  "layout.freeSlot.step": [4, 400],
  "layout.freeSlot.margin": [0, 400],
};
/** 선택지가 정해진 문자열. */
const ENUMS: Record<string, string[]> = {
  "defaults.themeMode": ["translucent", "solid"],
  "defaults.palette": ["dark", "light", "auto"],
  "defaults.cardStyle": ["glass", "minimal", "borderless", "none"],
};
/** 키를 미리 알 수 없는 열린 맵 — 안의 키는 경고하지 않는다. */
const OPEN_MAPS = new Set(["widgets", "intervals", "tray"]);

const kind = (v: unknown) => (Array.isArray(v) ? "array" : v === null ? "null" : typeof v);
const isObj = (v: unknown): v is Record<string, unknown> => kind(v) === "object";

/**
 * 사용자 값을 기본값 위에 합친다.
 * - 객체는 깊게 합치고, 배열은 통째로 바꾼다 (배치 목록을 "일부만" 덮는다는 건 뜻이 없다)
 * - 타입이 다르면 기본값을 두고 경고, 모르는 키도 경고 (오타를 조용히 무시하지 않는다)
 * - 범위·선택지가 정해진 값은 맞춰 넣는다
 */
export function mergeConfig<T>(defaults: T, user: unknown, file = "config.jsonc"): { value: T; warnings: string[] } {
  const warnings: string[] = [];
  const walk = (def: unknown, usr: unknown, path: string): unknown => {
    if (usr === undefined) return def;
    if (isObj(def) && isObj(usr)) {
      const out: Record<string, unknown> = { ...def };
      const open = OPEN_MAPS.has(path);
      for (const [k, v] of Object.entries(usr)) {
        if (k === "$schema") continue;
        const p = path ? `${path}.${k}` : k;
        if (!(k in def)) {
          if (open) out[k] = v;
          else warnings.push(`${file}: 알 수 없는 키 "${p}" — 무시합니다`);
          continue;
        }
        out[k] = walk(def[k], v, p);
      }
      return out;
    }
    if (kind(def) !== kind(usr)) {
      warnings.push(`${file}: "${path}" 는 ${kind(def)} 이어야 합니다 (지금 ${kind(usr)}) — 기본값을 씁니다`);
      return def;
    }
    if (typeof usr === "number") {
      if (!Number.isFinite(usr)) return def;
      const r = RANGES[path];
      if (r && (usr < r[0] || usr > r[1])) {
        warnings.push(`${file}: "${path}" 는 ${r[0]}~${r[1]} 사이여야 합니다 — 잘라 넣습니다`);
        return Math.min(r[1], Math.max(r[0], usr));
      }
    }
    if (typeof usr === "string" && ENUMS[path] && !ENUMS[path].includes(usr)) {
      warnings.push(`${file}: "${path}" 는 ${ENUMS[path].join(" | ")} 중 하나여야 합니다 — 기본값을 씁니다`);
      return def;
    }
    return usr;
  };
  if (user != null && !isObj(user)) {
    return { value: defaults, warnings: [`${file}: 최상위는 { … } 객체여야 합니다 — 무시합니다`] };
  }
  const value = walk(defaults, user ?? undefined, "") as T;
  return { value, warnings: [...warnings, ...checkArrays(value, file)] };
}

/** 배열 안쪽은 `mergeConfig` 가 보지 않는다 — 알려진 배열만 모양을 확인해 잘못된 항목을 버린다. */
function checkArrays(value: unknown, file: string): string[] {
  const warnings: string[] = [];
  if (!isObj(value) || !isObj(value.layout)) return warnings;
  const layout = value.layout as Record<string, unknown>;
  const num = (v: unknown) => typeof v === "number" && Number.isFinite(v);
  if (Array.isArray(layout.sizePresets)) {
    layout.sizePresets = layout.sizePresets.filter((p, i) => {
      const ok = isObj(p) && typeof p.label === "string" && num(p.k) && (p.k as number) > 0;
      if (!ok) warnings.push(`${file}: layout.sizePresets[${i}] 는 { "label": 문자열, "k": 양수 } 여야 합니다 — 버립니다`);
      return ok;
    });
  }
  if (Array.isArray(layout.default)) {
    layout.default = layout.default.filter((s, i) => {
      const ok = isObj(s) && typeof s.widget === "string" && num(s.x) && num(s.y) && num(s.w) && num(s.h);
      if (!ok) warnings.push(`${file}: layout.default[${i}] 는 { widget, x, y, w, h } 여야 합니다 — 버립니다`);
      return ok;
    });
  }
  return warnings;
}

/** 백엔드 `config_get` / `config://changed` 의 모양. */
interface Snapshot {
  config: unknown | null;
  strings: unknown | null;
  css: string | null;
  errors: { file: string; message: string }[];
  dir: string;
}

export type FileStatus = { file: string; state: "missing" | "ok" | "error"; messages: string[] };

interface ConfigState {
  config: ShellConfig;
  strings: ShellStrings;
  /** 설정 패널의 "사용자 설정 파일" 섹션이 보여 준다. */
  files: FileStatus[];
  dir: string;
  init(): Promise<void>;
  apply(s: Snapshot): void;
}

const FILES = ["config.jsonc", "strings.jsonc", "user.css"] as const;

export const useConfig = create<ConfigState>((set, get) => ({
  config: DEFAULT_CONFIG,
  strings: DEFAULT_STRINGS,
  files: FILES.map((file) => ({ file, state: "missing", messages: [] })),
  dir: "",

  /** 절대 throw 하지 않는다 — 실패하면 기본값으로 뜬다. */
  async init() {
    try {
      get().apply(await invoke<Snapshot>("config_get"));
    } catch (e) {
      console.warn("config_get", e);
    }
    listen<Snapshot>("config://changed", (e) => get().apply(e.payload)).catch(() => {});
  },

  apply(s) {
    const c = mergeConfig(DEFAULT_CONFIG, s.config, "config.jsonc");
    const t = mergeConfig(DEFAULT_STRINGS, s.strings, "strings.jsonc");
    applyUserCss(s.css);
    const present = { "config.jsonc": s.config != null, "strings.jsonc": s.strings != null, "user.css": s.css != null };
    const files: FileStatus[] = FILES.map((file) => {
      const errors = s.errors.filter((e) => e.file === file).map((e) => e.message);
      const warnings = file === "config.jsonc" ? c.warnings : file === "strings.jsonc" ? t.warnings : [];
      if (errors.length) return { file, state: "error", messages: errors };
      return { file, state: present[file] ? "ok" : "missing", messages: warnings };
    });
    set({ config: c.value, strings: t.value, files, dir: s.dir });
  },
}));

/** 문구 하나. `{n}` 같은 자리표시자를 채운다. */
export function t(key: keyof ShellStrings["editBar"], vars: Record<string, string | number> = {}): string {
  const raw = useConfig.getState().strings.editBar[key] ?? DEFAULT_STRINGS.editBar[key];
  return raw.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
}

/** theme.css 보다 **뒤에** 붙여야 같은 우선순위에서 이긴다 — 그래서 head 의 맨 끝에 둔다. */
function applyUserCss(css: string | null) {
  if (typeof document === "undefined") return;
  let el = document.getElementById("deskboard-user-css") as HTMLStyleElement | null;
  if (!css) { el?.remove(); return; }
  if (!el) {
    el = document.createElement("style");
    el.id = "deskboard-user-css";
  }
  el.textContent = css;
  document.head.appendChild(el); // 이미 있어도 다시 붙여 맨 끝을 지킨다 (위젯 CSS 가 뒤에 붙었을 수 있다)
}
