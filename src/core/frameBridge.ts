/**
 * HTML(iframe) 위젯과 셸 사이의 메시지 약속.
 *
 * iframe 은 `sandbox="allow-scripts allow-forms"`(same-origin 없음)로 뜬다 — Tauri IPC 에 직접
 * 닿지 못하므로 **여기 허용 목록이 그 위젯이 할 수 있는 일의 전부**다. 위젯 쪽 구현은
 * `src-tauri/resources/userland/sdk/bridge.js` (스킴이 index.html 에 자동으로 넣는다).
 */

export const NS = "deskboard";
export const VERSION = 1;

/** iframe 이 구독할 수 있는 백엔드 이벤트 — 늘 돌고 있어 구독만으로 비용이 생기지 않는 것. */
export const PUBLIC_EVENTS = [
  "sysmon://update",
  "claude_usage://limits",
  "claude_usage://update",
  "calendar://changed",
  "notes://changed",
  "activity://changed",
] as const;

/** iframe → 셸 호출. */
export type FrameMethod =
  | "storage.get"
  | "storage.set"
  | "command.runNow"
  | "command.last"
  | "events.subscribe"
  | "settings.update"
  | "openUrl";

const METHODS = new Set<string>(["storage.get", "storage.set", "command.runNow", "command.last", "events.subscribe", "settings.update", "openUrl"]);

export type FrameMessage =
  | { ns: typeof NS; v: number; type: "ready" }
  | { ns: typeof NS; v: number; type: "call"; id: number; method: FrameMethod; args: unknown[] }
  | { ns: typeof NS; v: number; type: "error"; message: string }
  | { ns: typeof NS; v: number; type: "focus"; editable: boolean };

/**
 * iframe 에서 온 메시지를 검증한다. 모양이 틀리거나 허용 목록 밖이면 null.
 * `subscribe` 는 그 위젯의 widget.json 이 선언한 것 ∩ `PUBLIC_EVENTS`.
 */
export function parseFrameMessage(data: unknown, subscribe: readonly string[]): FrameMessage | null {
  if (!data || typeof data !== "object") return null;
  const m = data as Record<string, unknown>;
  if (m.ns !== NS || m.v !== VERSION) return null;
  switch (m.type) {
    case "ready": return { ns: NS, v: VERSION, type: "ready" };
    case "error": return typeof m.message === "string" ? { ns: NS, v: VERSION, type: "error", message: m.message.slice(0, 2000) } : null;
    case "focus": return { ns: NS, v: VERSION, type: "focus", editable: m.editable === true };
    case "call": {
      if (typeof m.id !== "number" || typeof m.method !== "string" || !METHODS.has(m.method)) return null;
      const args = Array.isArray(m.args) ? m.args : [];
      if (m.method === "events.subscribe") {
        const name = args[0];
        if (typeof name !== "string" || !allowedEvent(name, subscribe)) return null;
      }
      if (m.method === "openUrl" && (typeof args[0] !== "string" || !/^https?:\/\//i.test(args[0]))) return null;
      if (m.method === "settings.update" && (!args[0] || typeof args[0] !== "object" || Array.isArray(args[0]))) return null;
      return { ns: NS, v: VERSION, type: "call", id: m.id, method: m.method as FrameMethod, args };
    }
    default: return null;
  }
}

export function allowedEvent(name: string, subscribe: readonly string[]): boolean {
  return subscribe.includes(name) && (PUBLIC_EVENTS as readonly string[]).includes(name);
}

/** iframe 에 넘겨줄 테마 토큰 — theme.css 의 변수 중 위젯이 쓸 만한 것. */
export const THEME_TOKENS = [
  "--accent", "--text", "--text-dim", "--surface", "--surface-strong", "--border",
  "--ok", "--warn", "--danger", "--fill", "--fill-hover", "--fill-active", "--track",
  "--on-light", "--on-text", "--shadow-pop", "--menu-bg", "--radius",
  "--font", "--mono", "--gap", "--lh-tight",
  "--fs-value", "--fs-title", "--fs-sub", "--fs-label", "--fs-meta", "--fs-micro",
  "--fw-value", "--fw-title", "--fw-bold", "--fw-label",
  "--ease", "--dur", "--dur-fast",
];

export function readTheme(el: HTMLElement = document.documentElement) {
  const cs = getComputedStyle(el);
  const vars: Record<string, string> = {};
  for (const t of THEME_TOKENS) vars[t] = cs.getPropertyValue(t).trim();
  const root = document.documentElement.dataset;
  return { vars, dataset: { palette: root.palette ?? "dark", themeMode: root.themeMode ?? "translucent", cardStyle: root.cardStyle ?? "glass" } };
}

/**
 * 위젯 폴더 안 파일의 주소. Windows(WebView2)는 커스텀 스킴을 `http://<scheme>.localhost/` 로 보여 준다.
 * `convertFileSrc` 는 경로 전체를 한 덩어리로 인코딩해 `/` 까지 `%2F` 가 되므로 쓰지 않는다 —
 * 그러면 HTML 안의 상대 경로(`style.css`)가 풀리지 않는다.
 */
export function dbwUrl(widgetId: string, relPath: string): string {
  const path = [widgetId, ...relPath.replace(/^\.?\//, "").split("/")].map(encodeURIComponent).join("/");
  const windows = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
  return windows ? `http://dbw.localhost/${path}` : `dbw://localhost/${path}`;
}
