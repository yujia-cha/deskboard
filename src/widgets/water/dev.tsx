/**
 * 물통 위젯만 띄우는 개발 페이지 (`dev.html`). 쿼리로 설정을 바꾼다:
 *   ?view=iso|isoLite|flat  &scene=<scene.ts 키>  &fill=45  &preset=water|honey|…  &big=1
 * 프레임 시간(ms)을 오른쪽에 찍는다 — rAF 가 멈추면 "sleeping" 이 뜬다.
 */
import ReactDOM from "react-dom/client";
import "../../core/theme.css";
import { Water } from "./Water";
import { waterWidget } from "./index";
import { defaultsOf } from "../types";
import type { WaterSettings } from "./params";

const q = new URLSearchParams(location.search);

// ?drive=1 — 창이 가려져 rAF 가 멈추는 환경(자동화)에서도 돌게 setTimeout 으로 흉내 내고, hidden 을 거짓으로 둔다.
if (q.get("drive")) {
  const raf = (cb: FrameRequestCallback) => window.setTimeout(() => cb(performance.now()), 16);
  window.requestAnimationFrame = raf as typeof window.requestAnimationFrame;
  window.cancelAnimationFrame = (id: number) => window.clearTimeout(id);
  Object.defineProperty(document, "hidden", { get: () => false, configurable: true });
}
const settings = { ...defaultsOf(waterWidget.settingsSchema) } as WaterSettings;
for (const [k, v] of q) {
  if (k === "big") continue;
  const cur = (settings as Record<string, unknown>)[k];
  (settings as Record<string, unknown>)[k] = typeof cur === "number" ? Number(v) : typeof cur === "boolean" ? v === "true" : v;
}

const root = document.getElementById("root")!;
root.className = "devcard" + (q.get("big") ? " big" : "");
const size = q.get("big") ? 480 : 260;
// theme.css 가 #root 를 창 크기로 늘려 둔다 — 카드 크기로 고정
root.style.cssText = `width:${size}px;height:${size}px;min-height:0;position:relative;flex:none`;
ReactDOM.createRoot(root).render(
  <Water instanceId="dev" settings={settings} size={{ w: size, h: size }} editing={false} />,
);

// 프레임 간격 통계 — 1초마다 갱신. rAF 가 1초 넘게 안 오면 잠든 것이다.
const info = document.getElementById("info")!;
let frames: number[] = [];
let last = performance.now();
let lastFrameAt = last;
function tick(now: number) {
  frames.push(now - last);
  last = now;
  lastFrameAt = now;
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);
setInterval(() => {
  const sorted = [...frames].sort((a, b) => a - b);
  const p = (f: number) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(f * sorted.length))].toFixed(1) : "-");
  info.textContent = `${JSON.stringify({ view: settings.view, scene: settings.scene, fill: settings.fill, preset: settings.preset })}\n`
    + `page rAF: ${frames.length} fps · frame p50 ${p(0.5)} ms · p95 ${p(0.95)} ms\n`
    + `(위젯의 rAF 는 물이 멈추면 따로 잠든다 — 페이지 rAF 와 다르다)`;
  frames = [];
}, 1000);
void lastFrameAt;

// 자동화용 — 콘솔/드라이버에서 `__water.shot()` 등으로 쓴다.
const canvasOf = () => document.querySelector<HTMLCanvasElement>(".water-canvas")!;
function pointer(type: string, x: number, y: number, id = 7) {
  const c = canvasOf();
  const r = c.getBoundingClientRect();
  c.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: id, button: 0, buttons: type === "pointerup" ? 0 : 1, clientX: r.left + x, clientY: r.top + y, pointerType: "mouse" }));
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
(window as unknown as { __water: unknown }).__water = {
  /** 캔버스 그림 — 작게 줄인 JPEG data URL (자동화에서 토큰을 아낀다) */
  shot: (scale = 0.5, quality = 0.6) => {
    const c = canvasOf();
    const o = document.createElement("canvas");
    o.width = Math.round(c.width * scale); o.height = Math.round(c.height * scale);
    const g = o.getContext("2d")!;
    g.fillStyle = "#26262e"; g.fillRect(0, 0, o.width, o.height);
    g.drawImage(c, 0, 0, o.width, o.height);
    return o.toDataURL("image/jpeg", quality);
  },
  /** 등각 버퍼를 글자로 — 자동화에서 그림 대신 본다. '#' 바디, '=' 윗면, ':' 오른면, '.' 왼면 */
  ascii: (step = 2) => {
    const e = (window as unknown as { __waterEngine?: { isoBuf: { w: number; h: number; rgba: Uint8ClampedArray; pick: Uint8Array } | null } }).__waterEngine;
    const buf = e?.isoBuf;
    if (!buf) return "";
    const rows: string[] = [];
    for (let y = 0; y < buf.h; y += step) {
      let r = "";
      for (let x = 0; x < buf.w; x += step) {
        const i = y * buf.w + x;
        const m = buf.pick[i];
        if (m === 0) { r += " "; continue; }
        if (m >= 2) { r += "#"; continue; }
        const l = buf.rgba[i * 4] + buf.rgba[i * 4 + 1] + buf.rgba[i * 4 + 2];
        r += l > 560 ? "=" : l > 430 ? ":" : ".";
      }
      rows.push(r.replace(/\s+$/, ""));
    }
    return rows.join("\n");
  },
  /** 엔진 상태 요약 + 수면 높이 통계 */
  stats: () => {
    const e = (window as unknown as { __waterEngine?: { debugState(): Record<string, unknown> } }).__waterEngine;
    if (!e) return null;
    const d = e.debugState() as { field?: { res: number; mat: Uint8Array } } & Record<string, unknown>;
    const f = d.field;
    let surf: Record<string, number> | null = null;
    if (f) {
      const hs: number[] = [];
      for (let ix = 0; ix < f.res; ix++) for (let iz = 0; iz < f.res; iz++) {
        let iy = 0;
        for (; iy < f.res; iy++) if (f.mat[(ix * f.res + iy) * f.res + iz] === 1) break;
        hs.push(iy);
      }
      const mean = hs.reduce((a, b) => a + b, 0) / hs.length;
      const within1 = hs.filter((v) => Math.abs(v - mean) <= 1).length / hs.length;
      surf = { mean, min: Math.min(...hs), max: Math.max(...hs), within1 };
    }
    const { field: _f, ...rest } = d;
    return { ...rest, surf };
  },
  /** (x1,y1) → (x2,y2) 를 ms 동안 끈다 (캔버스 px) */
  drag: async (x1: number, y1: number, x2: number, y2: number, ms = 800, steps = 24) => {
    pointer("pointerdown", x1, y1);
    for (let i = 1; i <= steps; i++) {
      await sleep(ms / steps);
      const t = i / steps;
      pointer("pointermove", x1 + (x2 - x1) * t, y1 + (y2 - y1) * t);
    }
    pointer("pointerup", x2, y2);
  },
  wheel: (dy: number) => canvasOf().dispatchEvent(new WheelEvent("wheel", { deltaY: dy, bubbles: true })),
  dbl: () => canvasOf().dispatchEvent(new MouseEvent("dblclick", { bubbles: true })),
  sleep,
};
