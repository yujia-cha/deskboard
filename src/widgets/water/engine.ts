/**
 * 물 위젯의 실행부 — 캔버스, 고정 스텝 루프, 입력, 잠들기, 테마 색.
 *
 * 순수한 부분은 모두 다른 파일에 있다 (`sim`/`sim3` 유체, `container` 통의 움직임, `params` 물리 값,
 * `render`/`voxels`/`iso` 그림, `scene` 장면). 여기는 그것들을 DOM 과 시간에 잇는다.
 *
 * 보기(`view`)가 셋이다:
 *  - `iso`: 3D 유체(`FlipSim3`)를 복셀로 찍어 등각 픽셀아트로. 가장자리 세로 드래그·휠 = 기울이기.
 *  - `isoLite`: 2D 유체를 깊이로 늘여 같은 등각 그림으로 — 3D 가 버거운 기계의 폴백.
 *  - `flat`: 예전 2D 측면 보기. 가장자리 드래그·휠 = 회전.
 *
 * **멈춰 있는 물은 비용이 0 이다.** 통을 건드리지 않고 물이 실제로 거의 움직이지 않는 상태가
 * 1초 이어지면 rAF 를 멈추고 마지막 그림을 그대로 둔다. 포인터·휠·설정 변경이 깨운다.
 * 창이 숨겨지면(`document.hidden`) 역시 멈춘다.
 */
import { FlipSim, BOX, resolutionFor } from "./sim";
import { FlipSim3, resolutionFor3, type Forces3, type SimParams3 } from "./sim3";
import { buildBodies, excludeParticles, markSolid, paintBodies, stepBodies, type Body, type BodySet } from "./bodies";
import {
  angleDelta, boundsFor, boxSettled, grabMode, homeAngle, inertialForces, inertialForces3, MAX_PITCH, newBox, stepBox,
  type Bounds, type BoxState, type BoxTargets,
} from "./container";
import {
  CALM_AFTER, CALM_GATE, CALM_RAMP, calmed, physicsOf, simParams3Of, simParamsOf,
  type Physics, type View, type WaterSettings,
} from "./params";
import { newRaster, rasterize, type Raster, type Rgb } from "./render";
import { BODY0, extrude2d, newField, splat3, type VoxelField } from "./voxels";
import { isoLayout, newIsoBuffer, paintIso, pickAt, project, U, type IsoBuffer, type IsoLayout } from "./iso";
import { sceneOf, type Scene } from "./scene";
import { sdfOf, type Sdf, type ShapeName } from "./shapes";
import { SCREEN_RIGHT } from "./container";
import type { SimParams } from "./sim";

/** 2D 시뮬레이션 한 스텝 (s). 3D 는 `FlipSim3.dt`(1/60). 시간 배율은 dt 가 아니라 스텝 **수**로 맞춘다. */
const DT = 1 / 120;
/** 한 프레임에 따라잡는 최대 스텝 — 넘치면 밀린 시간은 버린다 */
const MAX_STEPS = 8;
/** 이보다 덜 움직이면(실제 변위, m/s) 고인 물로 본다 */
const SLEEP_MOTION = 0.03;
/** 고인 상태가 이만큼 이어지면 잠든다 (s) */
const SLEEP_AFTER = 1;
/** 안전망 — 통을 건드리지 않은 지 이만큼 지나면 물이 어떻든 잠든다 (s) */
const FORCE_SLEEP_IDLE = 12;
/** 휠 한 칸 = 회전 15°, 기울이기 5° */
const WHEEL_STEP = Math.PI / 12;
const WHEEL_PITCH = Math.PI / 36;
/** 등각 그림이 카드에서 차지하는 비율 */
const ISO_FIT = 0.92;

type Drag =
  | { mode: "move"; id: number; offX: number; offY: number }
  | { mode: "rotate"; id: number; lastA: number; lastY: number }
  | { mode: "body"; id: number; body: Body; lastX: number; lastY: number };

export class WaterEngine {
  private ctx: CanvasRenderingContext2D;
  private off: HTMLCanvasElement;
  private offCtx: CanvasRenderingContext2D;
  private img: ImageData | null = null;

  // 2D
  private raster: Raster | null = null;
  private sim: FlipSim | null = null;
  // 3D / 등각
  private sim3: FlipSim3 | null = null;
  private field: VoxelField | null = null;
  private layout: IsoLayout | null = null;
  private isoBuf: IsoBuffer | null = null;
  private bodySet: BodySet = { bodies: [], matColors: [], matBody: [] };
  private sdf: Sdf = sdfOf("box");
  /** 등각 버퍼 px → 화면 px 정수 배 */
  private K = 1;
  private simKey = "";

  private settings: WaterSettings;
  private view: View = "iso";
  private scene: Scene;
  private physics: Physics;
  private params: SimParams;
  private params3: SimParams3;

  /** 레이아웃 px (zoom 을 뺀 값) */
  private w = 0;
  private h = 0;
  private scale = 1;
  /** 통(또는 등각 그림)의 화면 크기 px */
  private boxPx = 0;
  private bounds: Bounds = { minX: 0, maxX: 0, minY: 0, maxY: 0 };
  private home = { x: 0, y: 0 };
  private box: BoxState = newBox(0, 0);
  private targets: BoxTargets = { pos: null, ang: 0, pitch: 0 };
  private placed = false;

  private drag: Drag | null = null;
  private hover: "move" | "rotate" | null = null;

  private raf = 0;
  private last = 0;
  private acc = 0;
  private still = 0;
  private quiet = 0;
  private idle = 0;

  private water: Rgb = { r: 124, g: 156, b: 255 };
  private bodyColors: Rgb[] = [];
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
    this.scene = sceneOf(settings.scene);
    this.view = viewOf(settings);
    this.physics = physicsOf(this.scene.fluid ? { ...settings, ...this.scene.fluid } : settings);
    this.params = simParamsOf(this.physics, settings.quality);
    this.params3 = simParams3Of(this.physics, settings.quality);

    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(canvas);
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
    this.scene = sceneOf(s.scene);
    this.view = viewOf(s);
    this.physics = physicsOf(this.scene.fluid ? { ...s, ...this.scene.fluid } : s);
    this.params = simParamsOf(this.physics, s.quality);
    this.params3 = simParams3Of(this.physics, s.quality);
    if (s.color !== prev.color || s.scene !== prev.scene) this.readColors();
    if (s.returnHome && !prev.returnHome && !this.drag) this.goHome();
    this.resize();
    this.disturb();
  }

  // --- 크기·색 -------------------------------------------------------------------

  private resize() {
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    if (w <= 0 || h <= 0) return;
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const bw = Math.max(1, Math.round(rect.width * dpr)), bh = Math.max(1, Math.round(rect.height * dpr));
    if (this.canvas.width !== bw || this.canvas.height !== bh) {
      this.canvas.width = bw;
      this.canvas.height = bh;
    }
    this.scale = bw / w;

    const s = this.settings;
    const oldHome = this.home;
    this.w = w;
    this.h = h;
    this.home = { x: w / 2, y: h / 2 };
    const fill = Math.max(0.1, Math.min(0.8, Number(s.fill) / 100 || 0.45));
    const pixelSize = Math.max(1, Math.min(4, Math.round(Number(s.pixelSize) || 2)));

    if (this.view === "flat") {
      const shape = s.shape === "round" ? "round" : "square";
      this.boxPx = Math.min(w, h) * (shape === "round" ? 0.72 : 0.62);
      const res = resolutionFor(this.boxPx, pixelSize, fill, shape);
      const key = `flat|${res}|${fill}|${shape}`;
      if (key !== this.simKey) {
        this.simKey = key;
        this.sim = new FlipSim({ res, fill, shape });
        this.sim3 = null; this.field = null; this.layout = null; this.isoBuf = null;
        this.raster = newRaster(res);
        this.off.width = res;
        this.off.height = res;
        this.img = this.offCtx.createImageData(res, res);
        this.disturb();
      }
    } else {
      // 등각 — 복셀 한 변은 4U 버퍼 px, 화면에서는 그 K 배. 요청한 입자 크기에서 시작해 예산 안에서 res 를 정한다.
      const avail = Math.min(w, h) * ISO_FIT;
      const byPixel = Math.floor(avail / (4 * U * pixelSize));
      const res = Math.max(8, Math.min(resolutionFor3(avail, 1, fill), Math.max(10, byPixel)));
      this.K = Math.max(1, Math.floor(avail / (4 * U * res)));
      this.boxPx = 4 * U * res * this.K;
      const shapeName: ShapeName = typeof this.scene.shape === "string" ? this.scene.shape : "box";
      const sdf = sdfOf(this.scene.shape);
      const key = `${this.view}|${res}|${fill}|${s.scene}|${this.scene.shape === sdf ? "fn" : shapeName}`;
      if (key !== this.simKey) {
        this.simKey = key;
        this.sdf = sdf;
        if (this.view === "iso") {
          this.sim3 = new FlipSim3({ res, fill, sdf, seed: this.scene.seed });
          this.sim3.field = this.scene.field ?? null;
          this.bodySet = buildBodies(this.scene.bodies, this.sim3.voxel);
          this.sim = null; this.raster = null;
        } else {
          this.bodySet = buildBodies([], BOX / res);
          const shape = shapeName === "cylinder" || shapeName === "bowl" || shapeName === "sphere" ? "round" : "square";
          this.sim = new FlipSim({ res, fill, shape });
          this.raster = newRaster(res);
          this.sim3 = null;
        }
        this.field = newField(res);
        this.layout = isoLayout(res);
        this.isoBuf = newIsoBuffer(this.layout);
        this.off.width = this.layout.bufW;
        this.off.height = this.layout.bufH;
        this.img = this.offCtx.createImageData(this.layout.bufW, this.layout.bufH);
        this.readColors();
        this.disturb();
      }
    }
    this.bounds = boundsFor(w, h, this.boxPx);

    if (!this.placed) {
      this.box = newBox(this.home.x, this.home.y);
      this.targets = { pos: { ...this.home }, ang: 0, pitch: 0 };
      this.placed = true;
    } else {
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
    const rgb = (css: string): Rgb | null => {
      const m = css.match(/[\d.]+/g);
      return m && m.length >= 3 ? { r: +m[0], g: +m[1], b: +m[2] } : null;
    };
    const user = (this.settings.color ?? "").trim();
    const water = rgb((user && resolve(user)) || resolve("var(--accent)"));
    if (water) this.water = water;
    this.bodyColors = this.bodySet.matColors.map((c) => rgb(resolve(c)) ?? this.water);
    this.fillColor = resolve("var(--fill)") || "transparent";
    this.borderColor = resolve("var(--border)") || "transparent";
    this.hintColor = resolve("var(--text-dim)") || "transparent";
  }

  // --- 루프 ---------------------------------------------------------------------

  private disturb() {
    this.quiet = 0;
    this.idle = 0;
    this.still = 0;
    this.wake();
  }

  private wake() {
    if (this.raf || document.hidden || !(this.sim || this.sim3)) return;
    this.last = performance.now();
    this.acc = 0;
    this.raf = requestAnimationFrame(this.frame);
  }

  /** 지금 도는 유체의 실제 움직임 (m/s) — 바디가 움직이면 그것도 움직임이다 */
  private motion() {
    let m = this.sim3 ? this.sim3.motion() : this.sim ? this.sim.motion() : 0;
    for (const b of this.bodySet.bodies) m = Math.max(m, Math.hypot(b.vel[0], b.vel[1], b.vel[2]));
    return m;
  }

  private frame = (now: number) => {
    this.raf = 0;
    const sim = this.sim, sim3 = this.sim3;
    if (!sim && !sim3) return;
    const dt = sim3 ? sim3.dt : DT;
    const real = Math.min(0.05, Math.max(0, (now - this.last) / 1000));
    this.last = now;
    const ts = Math.max(0.25, Math.min(2, this.physics.timeScale / 100));
    this.acc += real * ts;
    const boxDt = dt / ts;
    const forceOpts = {
      gravity: this.physics.gravity,
      gravityDirDeg: this.physics.gravityDir,
      inertia: this.physics.inertia / 100,
      boxPx: this.boxPx,
    };
    let steps = 0;
    while (this.acc >= dt && steps < MAX_STEPS) {
      stepBox(this.box, this.targets, boxDt, this.bounds);
      const settled = !this.drag && boxSettled(this.box, this.targets);
      this.idle = settled ? this.idle + boxDt : 0;
      if (!settled) this.quiet = 0;
      else if (this.motion() < CALM_GATE) this.quiet += boxDt;
      const k = (this.quiet - CALM_AFTER) / CALM_RAMP;
      if (sim3) {
        const f: Forces3 = inertialForces3(this.box, forceOpts);
        markSolid(sim3, this.bodySet);
        sim3.step(f, calmed(this.params3, k));
        if (this.bodySet.bodies.length && this.field) {
          stepBodies(this.bodySet, sim3, this.field, f, dt, this.sdf);
          excludeParticles(sim3, this.bodySet);
        }
      } else if (sim) {
        sim.step(dt, inertialForces(this.box, forceOpts), calmed(this.params, k));
      }
      this.acc -= dt;
      steps++;
    }
    if (steps === MAX_STEPS) this.acc = 0;
    this.draw();

    const resting = !this.drag && boxSettled(this.box, this.targets) && this.motion() < SLEEP_MOTION;
    this.still = resting ? this.still + real : 0;
    const asleep = this.still >= SLEEP_AFTER || this.idle >= FORCE_SLEEP_IDLE;
    if (!asleep && !document.hidden) this.raf = requestAnimationFrame(this.frame);
  };

  // --- 그리기 -------------------------------------------------------------------

  private draw() {
    const { ctx } = this;
    ctx.setTransform(this.scale, 0, 0, this.scale, 0, 0);
    ctx.clearRect(0, 0, this.w, this.h);
    if (this.view === "flat") this.drawFlat();
    else this.drawIso();
  }

  private drawFlat() {
    const { ctx, sim, raster, img } = this;
    if (!sim || !raster || !img) return;
    rasterize(sim, raster, this.water);
    img.data.set(raster.rgba);
    this.offCtx.putImageData(img, 0, 0);

    const S = this.boxPx;
    const half = S / 2;
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
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(this.off, -half, -half, S, S);
    ctx.restore();
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = this.borderColor;
    ctx.stroke();
    ctx.restore();

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

  private drawIso() {
    const { ctx, field, layout, isoBuf, img } = this;
    if (!field || !layout || !isoBuf || !img) return;
    if (this.sim3) { splat3(this.sim3, field); paintBodies(this.bodySet, field); }
    else if (this.sim && this.raster) { rasterize(this.sim, this.raster, this.water); extrude2d(this.raster, field); }
    paintIso(field, layout, isoBuf, { water: this.water, bodies: this.bodyColors });
    img.data.set(isoBuf.rgba);
    this.offCtx.putImageData(img, 0, 0);

    const K = this.K;
    const left = this.box.x - (layout.bufW * K) / 2;
    const top = this.box.y - (layout.bufH * K) / 2;
    const P = (x: number, y: number, z: number) => { const p = project(layout, x, y, z); return [left + p.sx * K, top + p.sy * K] as const; };
    const res = layout.res;
    const shape: ShapeName = typeof this.scene.shape === "string" ? this.scene.shape : "box";
    const line = (a: readonly [number, number], b: readonly [number, number]) => { ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); };
    const ring = (y: number, r: number) => {
      for (let i = 0; i <= 32; i++) {
        const t = (i / 32) * Math.PI * 2;
        const p = P(res / 2 + r * Math.cos(t), y, res / 2 + r * Math.sin(t));
        if (i === 0) ctx.moveTo(p[0], p[1]); else ctx.lineTo(p[0], p[1]);
      }
    };

    ctx.save();
    ctx.lineWidth = 1;
    ctx.lineJoin = "round";
    // 바닥과 뒤쪽 모서리 — 물 뒤에
    ctx.beginPath();
    if (shape === "box") {
      const a = P(0, res, 0), b = P(res, res, 0), c = P(res, res, res), d = P(0, res, res);
      ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.lineTo(c[0], c[1]); ctx.lineTo(d[0], d[1]); ctx.closePath();
    } else if (shape === "cylinder") ring(res, res / 2);
    else if (shape === "bowl") ring(res / 2, res / 2);
    else ring(res / 2, res / 2);
    ctx.fillStyle = this.fillColor;
    ctx.fill();
    ctx.globalAlpha = 0.45;
    ctx.strokeStyle = this.borderColor;
    ctx.beginPath();
    if (shape === "box") {
      line(P(0, 0, 0), P(0, res, 0));
      line(P(0, res, 0), P(res, res, 0));
      line(P(0, res, 0), P(0, res, res));
    } else if (shape === "cylinder") {
      ring(res, res / 2);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;

    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(this.off, left, top, layout.bufW * K, layout.bufH * K);

    // 앞쪽 모서리와 윗 테두리 — 물 위에 (유리 통)
    ctx.beginPath();
    if (shape === "box") {
      line(P(res, 0, 0), P(res, res, 0));
      line(P(0, 0, res), P(0, res, res));
      line(P(res, 0, res), P(res, res, res));
      line(P(res, res, 0), P(res, res, res));
      line(P(0, res, res), P(res, res, res));
      line(P(0, 0, 0), P(res, 0, 0)); line(P(res, 0, 0), P(res, 0, res));
      line(P(res, 0, res), P(0, 0, res)); line(P(0, 0, res), P(0, 0, 0));
    } else if (shape === "cylinder") {
      ring(0, res / 2);
      // 실루엣 세로선 — 화면에서 가장 바깥인 두 점(x−z 가 극값인 t = −45°, 135°)
      for (const t of [-Math.PI / 4, (3 * Math.PI) / 4]) {
        const x = res / 2 + (res / 2) * Math.cos(t), z = res / 2 + (res / 2) * Math.sin(t);
        line(P(x, 0, z), P(x, res, z));
      }
    } else {
      // 그릇·구 — 테두리 타원 + 아래쪽 실루엣 반타원
      const yRim = shape === "bowl" ? res / 2 : 0;
      ring(yRim, res / 2);
      const c = P(res / 2, res / 2, res / 2);
      const half = P(res / 2 + (res / 2) * Math.SQRT1_2, res / 2, res / 2 - (res / 2) * Math.SQRT1_2);
      const rx = Math.abs(half[0] - c[0]);
      const ry = (res / 2) * 2 * U * K;
      ctx.moveTo(c[0] - rx, c[1]);
      ctx.ellipse(c[0], c[1], rx, ry, 0, Math.PI, 2 * Math.PI, true);
    }
    ctx.strokeStyle = this.borderColor;
    ctx.stroke();
    ctx.restore();

    if (this.hover === "rotate" && !this.drag) {
      ctx.save();
      ctx.setLineDash([3, 4]);
      ctx.lineWidth = 1;
      ctx.strokeStyle = this.hintColor;
      ctx.beginPath();
      ctx.arc(this.box.x, this.box.y, this.boxPx * 0.5 + 4, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
    }
  }

  // --- 입력 ---------------------------------------------------------------------

  private on(t: EventTarget, type: string, f: (e: Event) => void, opts?: AddEventListenerOptions) {
    t.addEventListener(type, f, opts);
    this.unlisten.push(() => t.removeEventListener(type, f, opts));
  }

  private local(e: { clientX: number; clientY: number }) {
    const rect = this.canvas.getBoundingClientRect();
    return {
      x: ((e.clientX - rect.left) * this.w) / Math.max(1, rect.width),
      y: ((e.clientY - rect.top) * this.h) / Math.max(1, rect.height),
    };
  }

  private modeAt(p: { x: number; y: number }) {
    const dx = p.x - this.box.x, dy = p.y - this.box.y;
    if (this.view !== "flat") return grabMode(dx, dy, this.boxPx, "round");
    const c = Math.cos(-this.box.th), s = Math.sin(-this.box.th);
    return grabMode(c * dx - s * dy, s * dx + c * dy, this.boxPx, this.sim?.shape ?? "square");
  }

  /** 등각 그림에서 포인터 밑의 바디 */
  private bodyAt(p: { x: number; y: number }): Body | null {
    const { layout, isoBuf } = this;
    if (!layout || !isoBuf || !this.sim3) return null;
    const K = this.K;
    const bx = (p.x - (this.box.x - (layout.bufW * K) / 2)) / K;
    const by = (p.y - (this.box.y - (layout.bufH * K) / 2)) / K;
    const m = pickAt(isoBuf, bx, by);
    if (m < BODY0) return null;
    return this.bodySet.bodies[this.bodySet.matBody[m - BODY0]] ?? null;
  }

  private onDown(e: PointerEvent) {
    if (e.button !== 0 || this.drag) return;
    const p = this.local(e);
    const body = this.bodyAt(p);
    this.canvas.setPointerCapture(e.pointerId);
    if (body) {
      body.grab = [...body.pos];
      this.drag = { mode: "body", id: e.pointerId, body, lastX: p.x, lastY: p.y };
      this.canvas.style.cursor = "grabbing";
      this.disturb();
      return;
    }
    const mode = this.modeAt(p);
    if (mode === "move") {
      this.drag = { mode, id: e.pointerId, offX: p.x - this.box.x, offY: p.y - this.box.y };
      this.targets.pos = { x: this.box.x, y: this.box.y };
    } else {
      this.drag = { mode, id: e.pointerId, lastA: Math.atan2(p.y - this.box.y, p.x - this.box.x), lastY: p.y };
      if (this.view === "flat") this.targets.ang = this.box.th;
      else this.targets.pitch = this.box.ph;
    }
    this.canvas.style.cursor = "grabbing";
    this.disturb();
  }

  private onMove(e: PointerEvent) {
    const p = this.local(e);
    const d = this.drag;
    if (!d) { this.setHover(this.bodyAt(p) ? "move" : this.modeAt(p)); return; }
    if (e.pointerId !== d.id) return;
    if (d.mode === "body") {
      // 화면 가로 = 통의 (x−z)/√2 방향, 화면 세로 = y (아래로 = 물속으로)
      const voxel = BOX / (this.layout?.res ?? 1);
      const horiz = ((p.x - d.lastX) / (2 * Math.SQRT2 * U * this.K)) * voxel;
      const vert = ((p.y - d.lastY) / (2 * U * this.K)) * voxel;
      d.lastX = p.x; d.lastY = p.y;
      const g = d.body.grab ?? [...d.body.pos];
      g[0] = Math.max(0, Math.min(BOX, g[0] + horiz * SCREEN_RIGHT[0]));
      g[1] = Math.max(0, Math.min(BOX, g[1] + vert));
      g[2] = Math.max(0, Math.min(BOX, g[2] + horiz * SCREEN_RIGHT[2]));
      d.body.grab = g;
    } else if (d.mode === "move") {
      this.targets.pos = { x: p.x - d.offX, y: p.y - d.offY };
    } else if (this.view === "flat") {
      const a = Math.atan2(p.y - this.box.y, p.x - this.box.x);
      this.targets.ang = (this.targets.ang ?? this.box.th) + angleDelta(a, d.lastA);
      d.lastA = a;
    } else {
      // 등각: 세로로 끈 만큼 기울인다 — 통 높이만큼 끌면 90°
      const t = (this.targets.pitch ?? this.box.ph) + ((p.y - d.lastY) / Math.max(1, this.boxPx)) * (Math.PI / 2);
      this.targets.pitch = Math.max(-MAX_PITCH, Math.min(MAX_PITCH, t));
      d.lastY = p.y;
    }
    this.disturb();
  }

  private onUp(e: PointerEvent) {
    const d = this.drag;
    if (!d || e.pointerId !== d.id) return;
    this.drag = null;
    if (this.canvas.hasPointerCapture(e.pointerId)) this.canvas.releasePointerCapture(e.pointerId);
    if (d.mode === "body") {
      d.body.grab = null;
      this.canvas.style.cursor = "grab";
      this.disturb();
      return;
    }
    if (this.settings.returnHome) {
      this.targets = { pos: { ...this.home }, ang: homeAngle(this.box.th), pitch: 0 };
    } else if (d.mode === "move") {
      this.targets.pos = null;
    } else if (this.view === "flat") {
      this.targets.ang = null;
    } else {
      this.targets.pitch = null;
    }
    this.canvas.style.cursor = "grab";
    this.disturb();
  }

  private onWheel(e: WheelEvent) {
    if (!e.deltaY) return;
    if (this.view === "flat") {
      this.targets.ang = (this.targets.ang ?? this.box.th) + Math.sign(e.deltaY) * WHEEL_STEP;
    } else {
      const t = (this.targets.pitch ?? this.box.ph) + Math.sign(e.deltaY) * WHEEL_PITCH;
      this.targets.pitch = Math.max(-MAX_PITCH, Math.min(MAX_PITCH, t));
    }
    this.disturb();
  }

  private goHome() {
    this.targets = { pos: { ...this.home }, ang: homeAngle(this.box.th), pitch: 0 };
    this.disturb();
  }

  private setHover(mode: "move" | "rotate" | null) {
    if (mode === this.hover) return;
    this.hover = mode;
    if (!this.drag) this.canvas.style.cursor = mode ? "grab" : "";
    if (!this.raf) this.draw();
  }
}

function viewOf(s: WaterSettings): View {
  return s.view === "flat" || s.view === "isoLite" ? s.view : "iso";
}
