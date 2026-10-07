/**
 * 물 위젯의 실행부 — 캔버스, 고정 스텝 루프, 입력, 잠들기, 테마 색.
 *
 * 순수한 부분은 모두 다른 파일에 있다 (`sim` 유체, `container` 통의 움직임, `params` 물리 값,
 * `render` 픽셀화). 여기는 그것들을 DOM 과 시간에 잇는다.
 *
 * **멈춰 있는 물은 비용이 0 이다.** 통을 건드리지 않고 물이 실제로 거의 움직이지 않는 상태가
 * 1초 이어지면 rAF 를 멈추고 마지막 그림을 그대로 둔다. 포인터·휠·설정 변경이 깨운다.
 * 창이 숨겨지면(`document.hidden`) 역시 멈춘다.
 */
import { FlipSim, BOX, resolutionFor } from "./sim";
import {
  angleDelta, boundsFor, boxSettled, grabMode, homeAngle, inertialForces, newBox, stepBox,
  type Bounds, type BoxState, type BoxTargets,
} from "./container";
import { CALM_AFTER, CALM_GATE, CALM_RAMP, calmed, physicsOf, simParamsOf, type Physics, type WaterSettings } from "./params";
import { newRaster, rasterize, type Raster, type Rgb } from "./render";
import type { SimParams } from "./sim";

/** 시뮬레이션 한 스텝 (s). 시간 배율은 dt 를 늘리지 않고 스텝 **수**로 맞춘다 — dt 를 키우면 잡음이 는다. */
const DT = 1 / 120;
/** 한 프레임에 따라잡는 최대 스텝 (시간 배율 200% 에서 60fps = 4) — 넘치면 밀린 시간은 버린다 */
const MAX_STEPS = 8;
/** 이보다 덜 움직이면(실제 변위, m/s) 고인 물로 본다 — 약 1~2 px/s */
const SLEEP_MOTION = 0.03;
/** 고인 상태가 이만큼 이어지면 잠든다 (s) */
const SLEEP_AFTER = 1;
/**
 * 안전망 — 통을 건드리지 않은 지 이만큼 지나면 물이 어떻든 잠든다 (s).
 * 실측으로는 모든 프리셋이 4~10초 안에 잠들지만, 어떤 조합이 끝내 떨면 60fps 를 영영 돌게 된다.
 * 그 무렵의 떨림은 1~2 px 라 멈춘 그림과 구별되지 않는다.
 */
const FORCE_SLEEP_IDLE = 12;
/** 휠 한 칸 = 15° */
const WHEEL_STEP = Math.PI / 12;

type Drag =
  | { mode: "move"; id: number; offX: number; offY: number }
  | { mode: "rotate"; id: number; lastA: number };

export class WaterEngine {
  private ctx: CanvasRenderingContext2D;
  private off: HTMLCanvasElement;
  private offCtx: CanvasRenderingContext2D;
  private img: ImageData | null = null;
  private raster: Raster | null = null;
  private sim: FlipSim | null = null;
  private simKey = "";

  private settings: WaterSettings;
  private physics: Physics;
  private params: SimParams;

  /** 레이아웃 px (zoom 을 뺀 값) */
  private w = 0;
  private h = 0;
  private scale = 1;
  private boxPx = 0;
  private bounds: Bounds = { minX: 0, maxX: 0, minY: 0, maxY: 0 };
  private home = { x: 0, y: 0 };
  private box: BoxState = newBox(0, 0);
  private targets: BoxTargets = { pos: null, ang: 0 };
  private placed = false;

  private drag: Drag | null = null;
  private hover: "move" | "rotate" | null = null;

  private raf = 0;
  private last = 0;
  private acc = 0;
  /** 고인 상태가 이어진 시간 (s) — SLEEP_AFTER 를 넘으면 잠든다 */
  private still = 0;
  /** 통을 건드리지 않았고 물도 거의 멈춘 채 흐른 시간 (s) — 잔물결 가라앉히기 */
  private quiet = 0;
  /** 통을 마지막으로 건드린 뒤 흐른 시간 (s) — 안전망 */
  private idle = 0;

  private water: Rgb = { r: 124, g: 156, b: 255 };
  private fillColor = "transparent";
  private borderColor = "transparent";
  private hintColor = "transparent";

  private ro: ResizeObserver;
  private mo: MutationObserver;
  private unlisten: (() => void)[] = [];

  constructor(private canvas: HTMLCanvasElement, private probe: HTMLElement, settings: WaterSettings) {
    this.ctx = canvas.getContext("2d")!;
    this.off = document.createElement("canvas");
    this.offCtx = this.off.getContext("2d")!;
    this.settings = settings;
    this.physics = physicsOf(settings);
    this.params = simParamsOf(this.physics, settings.quality);

    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(canvas);
    // 테마가 바뀌면 색을 다시 읽는다 — 팔레트는 <html> 의 속성, 강조색은 :root 와
    // 위젯 카드(인스턴스 강조색)의 style 에 있다.
    this.mo = new MutationObserver(() => { this.readColors(); this.draw(); });
    this.mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-palette", "data-theme-mode", "style"] });
    const card = canvas.closest(".widget");
    if (card) this.mo.observe(card, { attributes: true, attributeFilter: ["style"] });

    this.on(canvas, "pointerdown", (e) => this.onDown(e as PointerEvent));
    this.on(canvas, "pointermove", (e) => this.onMove(e as PointerEvent));
    this.on(canvas, "pointerup", (e) => this.onUp(e as PointerEvent));
    this.on(canvas, "pointercancel", (e) => this.onUp(e as PointerEvent));
    this.on(canvas, "lostpointercapture", (e) => this.onUp(e as PointerEvent));
    this.on(canvas, "pointerleave", () => this.setHover(null));
    this.on(canvas, "wheel", (e) => this.onWheel(e as WheelEvent), { passive: true });
    this.on(canvas, "dblclick", () => this.goHome());
    this.on(document, "visibilitychange", () => { if (!document.hidden) this.wake(); });

    this.readColors();
    this.resize();
  }

  dispose() {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.ro.disconnect();
    this.mo.disconnect();
    this.unlisten.forEach((f) => f());
  }

  setSettings(s: WaterSettings) {
    const prev = this.settings;
    this.settings = s;
    this.physics = physicsOf(s);
    this.params = simParamsOf(this.physics, s.quality);
    if (s.color !== prev.color) this.readColors();
    if (s.returnHome && !prev.returnHome && !this.drag) this.goHome();
    // 물 양·입자 크기·모양이 바뀌었으면 다시 붓는다 (resize 가 키를 비교한다).
    this.resize();
    // 물성이 바뀌었으면 바로 보이도록 출렁이게 둔다 — 가라앉히기를 처음부터 다시.
    this.disturb();
  }

  // --- 크기·색 -------------------------------------------------------------------

  private resize() {
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    if (w <= 0 || h <= 0) return;
    // 백킹 크기는 화면 픽셀로 — 프레임의 CSS zoom 과 DPI 가 모두 들어 있는 값이다.
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const bw = Math.max(1, Math.round(rect.width * dpr)), bh = Math.max(1, Math.round(rect.height * dpr));
    if (this.canvas.width !== bw || this.canvas.height !== bh) {
      this.canvas.width = bw;
      this.canvas.height = bh;
    }
    this.scale = bw / w;

    const s = this.settings;
    const shape = s.shape === "round" ? "round" : "square";
    const oldHome = this.home;
    this.w = w;
    this.h = h;
    // 사각 통은 돌려도 대각선이 카드를 크게 벗어나지 않게 조금 작게 잡는다.
    this.boxPx = Math.min(w, h) * (shape === "round" ? 0.72 : 0.62);
    this.bounds = boundsFor(w, h, this.boxPx);
    this.home = { x: w / 2, y: h / 2 };

    const fill = Math.max(0.1, Math.min(0.8, Number(s.fill) / 100 || 0.45));
    const res = resolutionFor(this.boxPx, Number(s.pixelSize) || 2, fill, shape);
    const key = `${res}|${fill}|${shape}`;
    if (key !== this.simKey) {
      this.simKey = key;
      this.sim = new FlipSim({ res, fill, shape });
      this.raster = newRaster(res);
      this.off.width = res;
      this.off.height = res;
      this.img = this.offCtx.createImageData(res, res);
      this.disturb();
    }

    if (!this.placed) {
      this.box = newBox(this.home.x, this.home.y);
      this.targets = { pos: { ...this.home }, ang: 0 };
      this.placed = true;
    } else {
      // 원위치를 향하던 중이었으면 새 원위치로. 상자도 새 경계 안으로 들인다.
      const t = this.targets.pos;
      if (t && t.x === oldHome.x && t.y === oldHome.y) this.targets.pos = { ...this.home };
      this.box.x = Math.max(this.bounds.minX, Math.min(this.bounds.maxX, this.box.x));
      this.box.y = Math.max(this.bounds.minY, Math.min(this.bounds.maxY, this.box.y));
    }
    this.draw();
  }

  /** CSS 토큰을 실제 색으로 푼다. `--border` 처럼 calc() 가 든 값도 color 속성을 거치면 풀린다. */
  private readColors() {
    const resolve = (css: string) => {
      this.probe.style.color = "";
      this.probe.style.color = css;
      return this.probe.style.color ? getComputedStyle(this.probe).color : "";
    };
    const user = (this.settings.color ?? "").trim();
    const water = (user && resolve(user)) || resolve("var(--accent)");
    const m = water.match(/[\d.]+/g);
    if (m && m.length >= 3) this.water = { r: +m[0], g: +m[1], b: +m[2] };
    this.fillColor = resolve("var(--fill)") || "transparent";
    this.borderColor = resolve("var(--border)") || "transparent";
    this.hintColor = resolve("var(--text-dim)") || "transparent";
  }

  // --- 루프 ---------------------------------------------------------------------

  /** 통을 건드렸다 — 잔물결 가라앉히기를 처음부터, 그리고 깨운다. */
  private disturb() {
    this.quiet = 0;
    this.idle = 0;
    this.still = 0;
    this.wake();
  }

  private wake() {
    if (this.raf || document.hidden || !this.sim) return;
    this.last = performance.now();
    this.acc = 0;
    this.raf = requestAnimationFrame(this.frame);
  }

  private frame = (now: number) => {
    this.raf = 0;
    const sim = this.sim;
    if (!sim) return;
    const real = Math.min(0.05, Math.max(0, (now - this.last) / 1000));
    this.last = now;
    const ts = Math.max(0.25, Math.min(2, this.physics.timeScale / 100));
    this.acc += real * ts;
    const boxDt = DT / ts; // 시뮬레이션 한 스텝 동안 흐르는 실제 시간
    let steps = 0;
    while (this.acc >= DT && steps < MAX_STEPS) {
      stepBox(this.box, this.targets, boxDt, this.bounds);
      const settled = !this.drag && boxSettled(this.box, this.targets);
      this.idle = settled ? this.idle + boxDt : 0;
      // 물이 아직 흐르는 동안은 가라앉히기 시계를 멈춰 둔다(0 으로 되돌리지는 않는다).
      if (!settled) this.quiet = 0;
      else if (sim.motion() < CALM_GATE) this.quiet += boxDt;
      const forces = inertialForces(this.box, {
        gravity: this.physics.gravity,
        gravityDirDeg: this.physics.gravityDir,
        inertia: this.physics.inertia / 100,
        boxPx: this.boxPx,
      });
      sim.step(DT, forces, calmed(this.params, (this.quiet - CALM_AFTER) / CALM_RAMP));
      this.acc -= DT;
      steps++;
    }
    if (steps === MAX_STEPS) this.acc = 0; // 밀린 시간은 버린다 — 따라잡으려다 더 느려지지 않게
    this.draw();

    const resting = !this.drag && boxSettled(this.box, this.targets) && sim.motion() < SLEEP_MOTION;
    this.still = resting ? this.still + real : 0;
    const asleep = this.still >= SLEEP_AFTER || this.idle >= FORCE_SLEEP_IDLE;
    if (!asleep && !document.hidden) this.raf = requestAnimationFrame(this.frame);
  };

  // --- 그리기 -------------------------------------------------------------------

  private draw() {
    const { ctx, sim, raster, img } = this;
    ctx.setTransform(this.scale, 0, 0, this.scale, 0, 0);
    ctx.clearRect(0, 0, this.w, this.h);
    if (!sim || !raster || !img) return;
    rasterize(sim, raster, this.water);
    img.data.set(raster.rgba);
    this.offCtx.putImageData(img, 0, 0);

    const S = this.boxPx;
    const half = S / 2;
    // 벽 칸 두께만큼 안쪽에 통을 그린다 — 물이 닿는 면과 그려진 벽이 맞아야 틈이 안 보인다.
    const inner = half - (sim.h / BOX) * S;
    ctx.save();
    ctx.translate(this.box.x, this.box.y);
    ctx.rotate(this.box.th);
    ctx.beginPath();
    if (sim.shape === "round") ctx.arc(0, 0, inner, 0, Math.PI * 2);
    else ctx.roundRect(-inner, -inner, inner * 2, inner * 2, Math.max(2, inner * 0.08));
    ctx.fillStyle = this.fillColor;
    ctx.fill();
    ctx.save();
    ctx.clip();
    ctx.imageSmoothingEnabled = false; // 보간 없이 확대 — 픽셀이 그대로 보여야 한다
    ctx.drawImage(this.off, -half, -half, S, S);
    ctx.restore();
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = this.borderColor;
    ctx.stroke();
    ctx.restore();

    // 회전 띠에 올렸을 때만 — 여기를 잡으면 돈다는 표시
    if (this.hover === "rotate" && !this.drag) {
      ctx.save();
      ctx.setLineDash([3, 4]);
      ctx.lineWidth = 1;
      ctx.strokeStyle = this.hintColor;
      ctx.beginPath();
      ctx.arc(this.box.x, this.box.y, (sim.shape === "round" ? inner : inner * Math.SQRT2) + 4, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
    }
  }

  // --- 입력 ---------------------------------------------------------------------

  private on(t: EventTarget, type: string, f: (e: Event) => void, opts?: AddEventListenerOptions) {
    t.addEventListener(type, f, opts);
    this.unlisten.push(() => t.removeEventListener(type, f, opts));
  }

  /** 포인터 → 레이아웃 px. CSS zoom 이 걸려 있어도 화면 크기와의 비율로 바꾸면 맞는다. */
  private local(e: { clientX: number; clientY: number }) {
    const rect = this.canvas.getBoundingClientRect();
    return {
      x: ((e.clientX - rect.left) * this.w) / Math.max(1, rect.width),
      y: ((e.clientY - rect.top) * this.h) / Math.max(1, rect.height),
    };
  }

  /** 레이아웃 px → 통 중심 기준·회전을 뺀 좌표 */
  private modeAt(p: { x: number; y: number }) {
    const dx = p.x - this.box.x, dy = p.y - this.box.y;
    const c = Math.cos(-this.box.th), s = Math.sin(-this.box.th);
    return grabMode(c * dx - s * dy, s * dx + c * dy, this.boxPx, this.sim?.shape ?? "square");
  }

  private onDown(e: PointerEvent) {
    if (e.button !== 0 || this.drag) return;
    const p = this.local(e);
    const mode = this.modeAt(p);
    this.canvas.setPointerCapture(e.pointerId);
    if (mode === "move") {
      this.drag = { mode, id: e.pointerId, offX: p.x - this.box.x, offY: p.y - this.box.y };
      this.targets.pos = { x: this.box.x, y: this.box.y };
    } else {
      this.drag = { mode, id: e.pointerId, lastA: Math.atan2(p.y - this.box.y, p.x - this.box.x) };
      this.targets.ang = this.box.th;
    }
    this.canvas.style.cursor = "grabbing";
    this.disturb();
  }

  private onMove(e: PointerEvent) {
    const p = this.local(e);
    const d = this.drag;
    if (!d) { this.setHover(this.modeAt(p)); return; }
    if (e.pointerId !== d.id) return;
    if (d.mode === "move") {
      this.targets.pos = { x: p.x - d.offX, y: p.y - d.offY };
    } else {
      // atan2 는 ±π 에서 튄다 — 변화량만 누적해 몇 바퀴든 이어서 돌린다.
      const a = Math.atan2(p.y - this.box.y, p.x - this.box.x);
      this.targets.ang = (this.targets.ang ?? this.box.th) + angleDelta(a, d.lastA);
      d.lastA = a;
    }
    this.disturb();
  }

  private onUp(e: PointerEvent) {
    const d = this.drag;
    if (!d || e.pointerId !== d.id) return;
    this.drag = null;
    if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
    if (this.settings.returnHome) {
      this.targets = { pos: { ...this.home }, ang: homeAngle(this.box.th) };
    } else if (d.mode === "move") {
      this.targets.pos = null; // 던진 만큼 미끄러지다 멈춘다
    } else {
      this.targets.ang = null; // 돌린 만큼 돌다 멈춘다
    }
    this.canvas.style.cursor = "grab";
    this.disturb();
  }

  private onWheel(e: WheelEvent) {
    if (!e.deltaY) return;
    this.targets.ang = (this.targets.ang ?? this.box.th) + Math.sign(e.deltaY) * WHEEL_STEP;
    this.disturb();
  }

  private goHome() {
    this.targets = { pos: { ...this.home }, ang: homeAngle(this.box.th) };
    this.disturb();
  }

  private setHover(mode: "move" | "rotate" | null) {
    if (mode === this.hover) return;
    this.hover = mode;
    if (!this.drag) this.canvas.style.cursor = mode ? "grab" : "";
    if (!this.raf) this.draw(); // 잠든 동안에도 표시는 바로 바뀌어야 한다
  }
}
