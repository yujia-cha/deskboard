/**
 * 통에 담긴 물 — PIC/FLIP 하이브리드 2D 유체.
 *
 * 왜 FLIP 인가: 모래 게임식 셀룰러 오토마타는 픽셀 느낌은 좋지만 입자에 **속도가 없어**
 * 흔들어도 출렁이지 않는다. FLIP 은 입자가 위치·속도를 들고, 비압축성(압력)만 격자에서 푼다.
 * 그래서 수천 개 입자로도 출렁임·튀김·잔물결이 나온다.
 * 구조는 Matthias Müller 의 Ten Minute Physics "FLIP fluid" 2D 구현을 따른다.
 *
 * **통의 좌표계에서 시뮬레이션한다.** 통은 이 안에서 늘 [0, BOX]² 에 고정된 상자(또는 원)다 —
 * 벽 처리가 격자 경계로 끝난다. 통이 움직이거나 도는 것은 `Forces` 의 관성력(가상력)으로 들어온다.
 * 좌표는 화면과 같은 방향이다: x 오른쪽, y **아래**. 각도는 +x 에서 +y 쪽으로 돈다.
 *
 * DOM 에 의존하지 않는다 — 테스트(node)에서 그대로 돌고, 필요하면 Worker 로 옮길 수 있다.
 */

/** 통 한 변의 실제 크기(m). 중력을 m/s² 그대로 쓰기 위한 축척이다. */
export const BOX = 1.5;
/** 입자 수 상한 — 프레임 예산(스텝당 몇 ms)을 지킨다. */
export const MAX_PARTICLES = 6000;

export type Shape = "square" | "round";

export interface SimParams {
  /** 0 = PIC(끈적·조용) ~ 1 = FLIP(튐·활발) */
  flipRatio: number;
  /** 격자 속도 평활 세기 0~1 — 점성 */
  viscosity: number;
  viscosityPasses: number;
  /** 이웃 입자끼리 당기는 세기 (위치 비율) — 표면장력·응집 */
  cohesion: number;
  /** 벽에 닿을 때마다 접선 속도를 깎는 비율 0~1 */
  friction: number;
  /** 벽 법선 반발 계수 0~1 */
  restitution: number;
  /** 속도 감쇠 (1/s) — 출렁임이 몇 초 안에 잦아들게 한다. 없으면 FLIP 은 거의 영원히 출렁인다 */
  drag: number;
  pressureIters: number;
  separationIters: number;
}

/** 통 좌표계에서 본 외력. */
export interface Forces {
  /** 균일 가속 (중력 − 통의 병진 가속), m/s² */
  ax: number;
  ay: number;
  /** 통의 각속도 rad/s — 원심력·코리올리 */
  omega: number;
  /** 통의 각가속도 rad/s² — 오일러력 */
  alpha: number;
}

const FLUID = 0;
const AIR = 1;
const SOLID = 2;

/** 입자 반지름(픽셀 단위). 육각 배치 간격이 정확히 1 픽셀이 된다. */
const R_PX = 0.5;
/** 격자 한 칸 = 입자 반지름 / 0.3 (Müller 의 비율) */
const R_PER_CELL = 0.3;
/** 응집 인력이 닿는 거리 (입자 지름의 배수) */
const COHESION_RANGE = 1.5;

/** 육각 배치에서 픽셀 하나당 입자 수 (간격 1px) */
const PARTICLES_PER_PX2 = 2 / Math.sqrt(3);

/**
 * 통 한 변의 픽셀 수(=시뮬레이션 해상도)를 정한다.
 *
 * 화면의 픽셀 한 칸이 `pixelSize` 보다 작아지지 않게 하되, 입자 수가 상한을 넘으면
 * 해상도를 낮춘다 — 큰 위젯에서는 픽셀이 조금 굵어지고 프레임 예산은 그대로 지켜진다.
 */
export function resolutionFor(boxPx: number, pixelSize: number, fill: number, shape: Shape): number {
  const areaFrac = shape === "round" ? Math.PI / 4 : 1;
  const byPixel = Math.floor(boxPx / Math.max(1, pixelSize));
  const byBudget = Math.floor(Math.sqrt(MAX_PARTICLES / (PARTICLES_PER_PX2 * Math.max(0.05, fill) * areaFrac)));
  return Math.max(16, Math.min(byPixel, byBudget));
}

export class FlipSim {
  readonly res: number;
  readonly shape: Shape;
  /** 입자 반지름 (m) */
  readonly r: number;
  /** 격자 한 칸 (m), 격자 한 변의 칸 수 */
  readonly h: number;
  readonly n: number;

  // --- 격자 (MAC: u 는 칸의 왼쪽 면, v 는 위쪽 면) ---
  private u: Float32Array;
  private v: Float32Array;
  private du: Float32Array;
  private dv: Float32Array;
  private prevU: Float32Array;
  private prevV: Float32Array;
  private tmp: Float32Array;
  /** 1 = 유체가 들어갈 수 있음, 0 = 벽 */
  private s: Float32Array;
  private cellType: Uint8Array;
  private density: Float32Array;
  private restDensity = 0;

  // --- 입자 ---
  count = 0;
  readonly pos: Float32Array;
  readonly vel: Float32Array;
  /** 직전 스텝 시작 때의 위치 — 실제로 얼마나 움직였는지(`motion`) 재는 데 쓴다 */
  private lastPos: Float32Array;
  private lastDt = 0;

  // --- 이웃 찾기용 공간 해시 ---
  private pInv: number;
  private pn: number;
  private cellCount: Int32Array;
  private cellStart: Int32Array;
  private cellIds: Int32Array;
  /** 입자마다 이번 스텝의 해시 칸 */
  private cellOf: Int32Array;
  /** 이번 스텝의 유체 칸 목록 — 압력은 이 칸들만 푼다 */
  private fluidCells: Int32Array;

  constructor(opts: { res: number; fill: number; shape: Shape }) {
    this.res = opts.res;
    this.shape = opts.shape;
    this.r = (BOX / opts.res) * R_PX;
    this.n = Math.floor(BOX / (this.r / R_PER_CELL)) + 1;
    this.h = BOX / this.n;
    const cells = this.n * this.n;
    this.u = new Float32Array(cells);
    this.v = new Float32Array(cells);
    this.du = new Float32Array(cells);
    this.dv = new Float32Array(cells);
    this.prevU = new Float32Array(cells);
    this.prevV = new Float32Array(cells);
    this.tmp = new Float32Array(cells);
    this.s = new Float32Array(cells);
    this.cellType = new Uint8Array(cells);
    this.density = new Float32Array(cells);

    this.pos = new Float32Array(MAX_PARTICLES * 2);
    this.vel = new Float32Array(MAX_PARTICLES * 2);
    this.lastPos = new Float32Array(MAX_PARTICLES * 2);

    // 응집 인력이 닿는 거리까지 한 칸 안에 들어오게 해시 칸을 잡는다.
    this.pInv = 1 / (2 * this.r * COHESION_RANGE);
    this.pn = Math.floor(BOX * this.pInv) + 1;
    this.cellCount = new Int32Array(this.pn * this.pn);
    this.cellStart = new Int32Array(this.pn * this.pn + 1);
    this.cellIds = new Int32Array(MAX_PARTICLES);
    this.cellOf = new Int32Array(MAX_PARTICLES);
    this.fluidCells = new Int32Array(cells);

    this.buildWalls();
    this.reseed(opts.fill);
  }

  /** 원형 통에서 입자가 머무는 원의 반지름 (m) */
  private get innerRadius() {
    return BOX / 2 - this.h - this.r;
  }

  private buildWalls() {
    const { n, h } = this;
    const c = BOX / 2;
    // 원형 통의 벽 칸: 입자가 머무는 원(innerRadius)에서 0.3 칸 밖부터 벽이다.
    //  - 더 바깥(0.6h 이상)이면 계단 모양 격자와 매끈한 원 사이에 입자가 못 가는 "공기" 칸이
    //    남아 자유 표면처럼 굴고, 압력이 물을 계속 벽으로 밀어 고인 물이 벽을 따라 끝없이 돈다.
    //  - 0.5h 는 해상도에 따라 들쭉날쭉했다 (실측: res 40 에서 0.18 m/s, res 85 에서 0.01).
    //  - 0.2~0.4h 는 res 24~90 전부에서 사각 통과 같은 수준(≈0.02 m/s)으로 가라앉았다.
    const wallR = this.innerRadius + 0.3 * h;
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        const border = i === 0 || j === 0 || i === n - 1 || j === n - 1;
        const outside = this.shape === "round" && Math.hypot((i + 0.5) * h - c, (j + 0.5) * h - c) > wallR;
        this.s[i * n + j] = border || outside ? 0 : 1;
      }
    }
  }

  /** 물을 바닥(+y)부터 육각으로 다시 채운다. 속도는 0 — 거의 정지 상태로 시작해 곧 잠든다. */
  reseed(fill: number) {
    const { r } = this;
    const lo = this.h + r;
    const hi = BOX - this.h - r;
    const dx = 2 * r;
    const dy = (Math.sqrt(3) / 2) * dx;
    const c = BOX / 2;
    const rr = this.innerRadius;
    const pts: number[] = [];
    for (let row = 0, y = hi; y >= lo; row++, y -= dy) {
      for (let x = lo + (row % 2 ? r : 0); x <= hi; x += dx) {
        if (this.shape === "round" && Math.hypot(x - c, y - c) > rr) continue;
        pts.push(x, y);
      }
    }
    const total = pts.length / 2;
    this.count = Math.min(MAX_PARTICLES, Math.round(Math.max(0, Math.min(1, fill)) * total));
    this.pos.set(pts.slice(0, this.count * 2));
    this.vel.fill(0);
    this.u.fill(0);
    this.v.fill(0);
    this.restDensity = 0;
  }

  step(dt: number, f: Forces, p: SimParams) {
    this.lastPos.set(this.pos.subarray(0, this.count * 2));
    this.lastDt = dt;
    this.integrate(dt, f, p.drag);
    this.separate(p.separationIters, p.cohesion);
    this.collide(p.friction, p.restitution);
    this.toGrid();
    this.updateDensity();
    // FLIP 의 보정량에 점성과 압력이 **둘 다** 들어가도록 평활 전에 찍어 둔다.
    this.prevU.set(this.u);
    this.prevV.set(this.v);
    if (p.viscosity > 0) this.smooth(p.viscosity, p.viscosityPasses);
    this.solve(p.pressureIters);
    this.toParticles(p.flipRatio);
  }

  private integrate(dt: number, f: Forces, drag: number) {
    const keep = Math.exp(-drag * dt);
    const c = BOX / 2;
    const w = f.omega;
    const w2 = w * w;
    const { pos, vel } = this;
    for (let i = 0; i < this.count; i++) {
      const x = pos[2 * i], y = pos[2 * i + 1];
      const vx = vel[2 * i], vy = vel[2 * i + 1];
      const rx = x - c, ry = y - c;
      // 균일 가속 + 원심력(ω²r) + 오일러력(−α×r) + 코리올리(−2ω×v)
      const ax = f.ax + w2 * rx + f.alpha * ry + 2 * w * vy;
      const ay = f.ay + w2 * ry - f.alpha * rx - 2 * w * vx;
      const nvx = (vx + ax * dt) * keep, nvy = (vy + ay * dt) * keep;
      vel[2 * i] = nvx;
      vel[2 * i + 1] = nvy;
      pos[2 * i] = x + nvx * dt;
      pos[2 * i + 1] = y + nvy * dt;
    }
  }

  /** 겹친 입자를 밀어내고, 응집이 켜져 있으면 조금 떨어진 이웃을 당긴다. */
  private separate(iters: number, cohesion: number) {
    const { pos, pInv, pn, cellCount, cellStart, cellIds, cellOf } = this;
    const count = this.count;
    const last = pn - 1;
    // 칸은 스텝마다 한 번만 잡는다 — 반복 사이에 입자가 칸 하나만큼 움직이지는 않는다.
    cellCount.fill(0);
    for (let i = 0; i < count; i++) {
      let cx = Math.floor(pos[2 * i] * pInv), cy = Math.floor(pos[2 * i + 1] * pInv);
      cx = cx < 0 ? 0 : cx > last ? last : cx;
      cy = cy < 0 ? 0 : cy > last ? last : cy;
      const cell = cx * pn + cy;
      cellOf[i] = cell;
      cellCount[cell]++;
    }
    let first = 0;
    for (let i = 0; i < pn * pn; i++) {
      first += cellCount[i];
      cellStart[i] = first;
    }
    cellStart[pn * pn] = first;
    for (let i = 0; i < count; i++) {
      const cell = cellOf[i];
      cellStart[cell]--;
      cellIds[cellStart[cell]] = i;
    }

    const minDist = 2 * this.r;
    const minDist2 = minDist * minDist;
    const range = minDist * COHESION_RANGE;
    const range2 = cohesion > 0 ? range * range : minDist2;
    const pull = (cohesion * 0.5) / (range - minDist);
    for (let it = 0; it < iters; it++) {
      for (let i = 0; i < count; i++) {
        const cell = cellOf[i];
        const xi = (cell / pn) | 0, yi = cell - xi * pn;
        const x0 = xi > 0 ? xi - 1 : 0, x1 = xi < last ? xi + 1 : last;
        const y0 = yi > 0 ? yi - 1 : 0, y1 = yi < last ? yi + 1 : last;
        let px = pos[2 * i], py = pos[2 * i + 1];
        for (let cx = x0; cx <= x1; cx++) {
          const row = cx * pn;
          const kEnd = cellStart[row + y1 + 1];
          // 같은 열의 y0..y1 칸은 cellStart 에서 연속이다 — 한 번에 훑는다.
          for (let k = cellStart[row + y0]; k < kEnd; k++) {
            const id = cellIds[k];
            if (id <= i) continue; // 쌍마다 한 번만
            const dx = pos[2 * id] - px, dy = pos[2 * id + 1] - py;
            const d2 = dx * dx + dy * dy;
            if (d2 > range2 || d2 === 0) continue;
            const d = Math.sqrt(d2);
            // 겹침은 절반씩 밀어내고, 응집 거리 안이면 가운데서 가장 세게 당긴다(양 끝에서 0).
            const s = d < minDist ? (0.5 * (minDist - d)) / d : (-pull * (d - minDist) * (range - d)) / d;
            const mx = dx * s, my = dy * s;
            px -= mx;
            py -= my;
            pos[2 * id] += mx;
            pos[2 * id + 1] += my;
          }
        }
        pos[2 * i] = px;
        pos[2 * i + 1] = py;
      }
    }
  }

  private collide(friction: number, restitution: number) {
    const { pos, vel } = this;
    const lo = this.h + this.r;
    const hi = BOX - this.h - this.r;
    const c = BOX / 2;
    const rr = this.innerRadius;
    const keep = 1 - friction;
    for (let i = 0; i < this.count; i++) {
      let x = pos[2 * i], y = pos[2 * i + 1];
      let vx = vel[2 * i], vy = vel[2 * i + 1];
      if (!Number.isFinite(x) || !Number.isFinite(y)) { x = c; y = c; }
      if (!Number.isFinite(vx) || !Number.isFinite(vy)) { vx = 0; vy = 0; }
      if (this.shape === "round") {
        const dx = x - c, dy = y - c;
        const d = Math.hypot(dx, dy);
        if (d > rr) {
          const nx = dx / d, ny = dy / d;
          x = c + nx * rr;
          y = c + ny * rr;
          const vn = vx * nx + vy * ny;
          if (vn > 0) {
            const tx = vx - vn * nx, ty = vy - vn * ny;
            vx = tx * keep - restitution * vn * nx;
            vy = ty * keep - restitution * vn * ny;
          }
        }
      }
      // 사각 경계는 원형에서도 안전망으로 건다.
      if (x < lo) { x = lo; if (vx < 0) { vx = -restitution * vx; vy *= keep; } }
      if (x > hi) { x = hi; if (vx > 0) { vx = -restitution * vx; vy *= keep; } }
      if (y < lo) { y = lo; if (vy < 0) { vy = -restitution * vy; vx *= keep; } }
      if (y > hi) { y = hi; if (vy > 0) { vy = -restitution * vy; vx *= keep; } }
      pos[2 * i] = x;
      pos[2 * i + 1] = y;
      vel[2 * i] = vx;
      vel[2 * i + 1] = vy;
    }
  }

  /** 격자 보간 가중치 — u(dx=0, dy=h/2) / v(dx=h/2, dy=0) 공용. */
  private weights(x: number, y: number, ox: number, oy: number) {
    const { n, h } = this;
    const h1 = 1 / h;
    x = Math.max(h, Math.min((n - 1) * h, x));
    y = Math.max(h, Math.min((n - 1) * h, y));
    const x0 = Math.min(Math.floor((x - ox) * h1), n - 2);
    const tx = (x - ox - x0 * h) * h1;
    const x1 = Math.min(x0 + 1, n - 2);
    const y0 = Math.min(Math.floor((y - oy) * h1), n - 2);
    const ty = (y - oy - y0 * h) * h1;
    const y1 = Math.min(y0 + 1, n - 2);
    const sx = 1 - tx, sy = 1 - ty;
    W.d0 = sx * sy; W.d1 = tx * sy; W.d2 = tx * ty; W.d3 = sx * ty;
    W.n0 = x0 * n + y0; W.n1 = x1 * n + y0; W.n2 = x1 * n + y1; W.n3 = x0 * n + y1;
    return W;
  }

  private toGrid() {
    const { n, h, pos, vel, s, cellType } = this;
    for (let i = 0; i < n * n; i++) cellType[i] = s[i] === 0 ? SOLID : AIR;
    for (let i = 0; i < this.count; i++) {
      const xi = Math.max(0, Math.min(n - 1, Math.floor(pos[2 * i] / h)));
      const yi = Math.max(0, Math.min(n - 1, Math.floor(pos[2 * i + 1] / h)));
      const cell = xi * n + yi;
      if (cellType[cell] === AIR) cellType[cell] = FLUID;
    }
    for (let comp = 0; comp < 2; comp++) {
      const f = comp === 0 ? this.u : this.v;
      const d = comp === 0 ? this.du : this.dv;
      f.fill(0);
      d.fill(0);
      const ox = comp === 0 ? 0 : h / 2, oy = comp === 0 ? h / 2 : 0;
      for (let i = 0; i < this.count; i++) {
        const w = this.weights(pos[2 * i], pos[2 * i + 1], ox, oy);
        const pv = vel[2 * i + comp];
        f[w.n0] += pv * w.d0; d[w.n0] += w.d0;
        f[w.n1] += pv * w.d1; d[w.n1] += w.d1;
        f[w.n2] += pv * w.d2; d[w.n2] += w.d2;
        f[w.n3] += pv * w.d3; d[w.n3] += w.d3;
      }
      for (let i = 0; i < n * n; i++) if (d[i] > 0) f[i] /= d[i];
    }
    // 벽은 통과 함께 움직이므로(통 좌표계에서 정지) 벽에 닿은 면의 속도는 0 이다.
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        const solid = cellType[i * n + j] === SOLID;
        if (solid || (i > 0 && cellType[(i - 1) * n + j] === SOLID)) { this.u[i * n + j] = 0; this.du[i * n + j] = 0; }
        if (solid || (j > 0 && cellType[i * n + j - 1] === SOLID)) { this.v[i * n + j] = 0; this.dv[i * n + j] = 0; }
      }
    }
  }

  private updateDensity() {
    const { n, h, pos, density: d } = this;
    d.fill(0);
    for (let i = 0; i < this.count; i++) {
      const w = this.weights(pos[2 * i], pos[2 * i + 1], h / 2, h / 2);
      d[w.n0] += w.d0; d[w.n1] += w.d1; d[w.n2] += w.d2; d[w.n3] += w.d3;
    }
    if (this.restDensity === 0) {
      let sum = 0, cells = 0;
      for (let i = 0; i < n * n; i++) if (this.cellType[i] === FLUID) { sum += d[i]; cells++; }
      if (cells > 0) this.restDensity = sum / cells;
    }
  }

  /**
   * 점성 — 입자가 닿은 면끼리 속도를 이웃 평균 쪽으로 당긴다.
   * 입자가 없는 면(공기)은 평균에 넣지 않는다: 넣으면 공기 저항처럼 떨어지는 물방울까지 멈춘다.
   */
  private smooth(strength: number, passes: number) {
    const { n, tmp } = this;
    for (let comp = 0; comp < 2; comp++) {
      const f = comp === 0 ? this.u : this.v;
      const d = comp === 0 ? this.du : this.dv;
      for (let pass = 0; pass < passes; pass++) {
        tmp.set(f);
        for (let i = 1; i < n - 1; i++) {
          for (let j = 1; j < n - 1; j++) {
            const k = i * n + j;
            if (d[k] <= 0) continue;
            let sum = 0, cnt = 0;
            if (d[k - n] > 0) { sum += tmp[k - n]; cnt++; }
            if (d[k + n] > 0) { sum += tmp[k + n]; cnt++; }
            if (d[k - 1] > 0) { sum += tmp[k - 1]; cnt++; }
            if (d[k + 1] > 0) { sum += tmp[k + 1]; cnt++; }
            if (cnt > 0) f[k] = tmp[k] + strength * (sum / cnt - tmp[k]);
          }
        }
      }
    }
  }

  /** 비압축성 — Gauss-Seidel + 과이완, 입자가 뭉친 칸은 밀도 drift 를 함께 덜어 낸다. */
  private solve(iters: number) {
    const { n, u, v, s, cellType, density, fluidCells } = this;
    const over = 1.9;
    // 유체 칸만 모아 둔다 — 빈 칸까지 매 반복 훑으면 물이 적을수록 헛돈다.
    let count = 0;
    for (let i = 1; i < n - 1; i++) {
      for (let j = 1; j < n - 1; j++) {
        const c = i * n + j;
        if (cellType[c] === FLUID && s[c - n] + s[c + n] + s[c - 1] + s[c + 1] > 0) fluidCells[count++] = c;
      }
    }
    const rest = this.restDensity;
    for (let it = 0; it < iters; it++) {
      for (let k = 0; k < count; k++) {
        const c = fluidCells[k];
        const right = c + n, top = c + 1;
        const sx0 = s[c - n], sx1 = s[right], sy0 = s[c - 1], sy1 = s[top];
        let div = u[right] - u[c] + v[top] - v[c];
        if (rest > 0) {
          const compression = density[c] - rest;
          if (compression > 0) div -= compression;
        }
        const p = (-div / (sx0 + sx1 + sy0 + sy1)) * over;
        u[c] -= sx0 * p;
        u[right] += sx1 * p;
        v[c] -= sy0 * p;
        v[top] += sy1 * p;
      }
    }
  }

  private toParticles(flipRatio: number) {
    const { n, h, pos, vel, cellType } = this;
    for (let comp = 0; comp < 2; comp++) {
      const f = comp === 0 ? this.u : this.v;
      const prev = comp === 0 ? this.prevU : this.prevV;
      const offset = comp === 0 ? n : 1;
      const ox = comp === 0 ? 0 : h / 2, oy = comp === 0 ? h / 2 : 0;
      const valid = (k: number) => (cellType[k] !== AIR || cellType[k - offset] !== AIR ? 1 : 0);
      for (let i = 0; i < this.count; i++) {
        const w = this.weights(pos[2 * i], pos[2 * i + 1], ox, oy);
        const v0 = valid(w.n0) * w.d0, v1 = valid(w.n1) * w.d1, v2 = valid(w.n2) * w.d2, v3 = valid(w.n3) * w.d3;
        const d = v0 + v1 + v2 + v3;
        if (d <= 0) continue;
        const pic = (v0 * f[w.n0] + v1 * f[w.n1] + v2 * f[w.n2] + v3 * f[w.n3]) / d;
        const corr = (v0 * (f[w.n0] - prev[w.n0]) + v1 * (f[w.n1] - prev[w.n1])
          + v2 * (f[w.n2] - prev[w.n2]) + v3 * (f[w.n3] - prev[w.n3])) / d;
        const flip = vel[2 * i + comp] + corr;
        vel[2 * i + comp] = (1 - flipRatio) * pic + flipRatio * flip;
      }
    }
  }

  /** 입자 평균 속력² (m²/s²) — 저장된 속도 기준. */
  meanSpeed2(): number {
    if (this.count === 0) return 0;
    let sum = 0;
    for (let i = 0; i < this.count * 2; i++) sum += this.vel[i] * this.vel[i];
    return sum / this.count;
  }

  /**
   * 직전 스텝에서 입자가 **실제로** 움직인 평균 속력 (m/s) — 잠들기 판정은 이것으로 한다.
   *
   * 저장된 속도(`meanSpeed2`)로 재면 안 된다: 가만히 고인 물도 입자마다 g·dt(≈0.08 m/s)
   * 만큼의 아래 방향 속도를 늘 들고 있다. 다음 스텝에 벽과 압력이 지워 버리는 "대기 중인" 중력이라
   * 화면에서는 1초에 0.06 픽셀도 안 움직이는데, 속도로 재면 영영 잠들지 않는다 (실측).
   */
  motion(): number {
    if (this.count === 0 || this.lastDt <= 0) return 0;
    let sum = 0;
    for (let i = 0; i < this.count; i++) {
      sum += Math.hypot(this.pos[2 * i] - this.lastPos[2 * i], this.pos[2 * i + 1] - this.lastPos[2 * i + 1]);
    }
    return sum / this.count / this.lastDt;
  }

  /** 질량중심 (m) */
  centerOfMass(): { x: number; y: number } {
    let x = 0, y = 0;
    for (let i = 0; i < this.count; i++) { x += this.pos[2 * i]; y += this.pos[2 * i + 1]; }
    return this.count ? { x: x / this.count, y: y / this.count } : { x: BOX / 2, y: BOX / 2 };
  }
}

/** `weights` 가 매번 객체를 만들지 않도록 재사용하는 결과 버퍼 (단일 스레드). */
const W = { d0: 0, d1: 0, d2: 0, d3: 0, n0: 0, n1: 0, n2: 0, n3: 0 };
