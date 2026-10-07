/**
 * 통에 담긴 물 — PIC/FLIP 하이브리드 **3D** 유체. `sim.ts`(2D) 의 구조를 그대로 한 축 늘렸다.
 *
 * 등각(2.5D) 그림을 위해 입자 하나가 복셀 하나다: 한 변 `res` 복셀, 입자는 입방 격자로 시드해
 * 처음 화면이 정확히 평면이 되게 한다. 입자가 2D 보다 굵은 만큼(수천 개) 스텝은 1/60 s 로 돈다.
 *
 * **좌표계** — 유체가 들어갈 수 있는 영역이 [0, BOX]³ 이고, 벽 칸은 그 바깥 한 칸(두께 h)이다.
 * 2D 와 달리 벽을 BOX 밖에 두어 복셀 좌표(`floor(x / voxel)`)가 늘 0..res-1 에 떨어진다.
 * x 오른쪽·y **아래**·z 는 화면 안쪽(등각에서 왼쪽 아래로 뻗는 축). 통의 모양은 SDF(벽면 기준,
 * 안쪽 음수)로 준다 — 벽 칸은 중심의 sdf > 0, 입자는 sdf ≤ −r 에 머문다.
 * (2D 실측 "원형 벽 칸 = 입자 원 + 0.3h" 는 r = 0.3h 를 넣으면 정확히 이 식이다.)
 *
 * 통의 이동·회전은 `Forces3` 의 관성력으로 들어온다. 그 밖의 "법칙"(비현실적 힘)은 `field` 훅이
 * 입자마다 더한다. DOM 에 의존하지 않는다.
 */
import { BOX, type SimParams } from "./sim";
import { boxSdf, projectOut, type Sdf } from "./shapes";

export { BOX };
/**
 * 입자 수 상한 — 스텝 예산을 지킨다. 실측(흔드는 동안, 압력 30회, 분리 1회, 해시 칸 2.5r):
 * 2,592개(res 18) 1.6 ms · 3,600개(res 20) 2.3 ms · 8,112개(res 26) 8.3 ms. 분리(이웃 탐색)가 절반이다.
 */
export const MAX_PARTICLES3 = 4000;

export interface SimParams3 extends SimParams {
  /**
   * 밀도 보정 세기. 2D 는 1 로 고정돼 있었는데, 3D 는 칸당 입자 수(≈4.6)가 2D(≈3.2)와 달라
   * 같은 식이 다른 세기가 된다. 부피 테스트로 맞춘다.
   */
  densityK: number;
}

/** 통 좌표계에서 본 외력. ω·α 는 벡터(rad/s, rad/s²). */
export interface Forces3 {
  ax: number; ay: number; az: number;
  wx: number; wy: number; wz: number;
  alx: number; aly: number; alz: number;
}

/** 법칙 훅 — 입자 위치·시각에서 추가 가속을 `out` 에 쓴다 (m/s²). 입자마다 불리므로 가벼워야 한다. */
export type Field3 = (x: number, y: number, z: number, t: number, out: Float32Array) => void;

export interface Sim3Options {
  /** 한 변의 복셀 수 */
  res: number;
  /** 물 높이 비율 0~1 */
  fill: number;
  /** 통 모양 (벽면 SDF). 생략하면 사각 통 */
  sdf?: Sdf;
  /** 시드 필터 — 참인 복셀 중심에만 물을 둔다 (법칙의 `seed`) */
  seed?: (x: number, y: number, z: number) => boolean;
  /** 공간 해시 칸 = r × 이 값. 응집 범위(COHESION_RANGE × 지름)가 한 칸 안에 들어와야 한다 */
  hashCell?: number;
  /** 스텝 (s) */
  dt?: number;
}

const FLUID = 0;
const AIR = 1;
const SOLID = 2;

/** 격자 한 칸 = 입자 반지름 / 0.3 (Müller) — 2D 와 같은 비율이라 벽 실측 규칙이 그대로 통한다 */
const R_PER_CELL = 0.3;
/** 응집 인력이 닿는 거리 (입자 지름의 배수). 2D 의 1.5 보다 좁다 — 3D 는 이웃이 훨씬 많다 */
export const COHESION_RANGE = 1.25;
const DEFAULT_HASH_CELL = 2 * COHESION_RANGE; // 지름 × 범위 = r × 2.5
export const DT3 = 1 / 60;

/** 프로파일 구간 이름 — `prof` 가 켜져 있으면 스텝마다 구간별 ms 를 누적한다 */
export const PHASES = ["integrate", "separate", "collide", "toGrid", "density", "smooth", "solve", "toParticles"] as const;

/**
 * 한 변의 복셀 수를 정한다 — 화면 복셀이 `pixelSize` 보다 작아지지 않게 하되 입자 수 상한을 넘기지 않는다.
 * `boxPx` 는 등각 그림에서 복셀 한 변이 차지하는 화면 px 로 환산하기 전의 통 한 변 px 다.
 */
export function resolutionFor3(boxPx: number, pixelSize: number, fill: number): number {
  const byPixel = Math.floor(boxPx / Math.max(1, pixelSize));
  const byBudget = Math.floor(Math.cbrt(MAX_PARTICLES3 / Math.max(0.05, Math.min(1, fill))));
  return Math.max(8, Math.min(byPixel, byBudget));
}

export class FlipSim3 {
  readonly res: number;
  /** 복셀 한 변 (m) */
  readonly voxel: number;
  /** 입자 반지름 (m) */
  readonly r: number;
  /** 격자 한 칸 (m), 한 변의 칸 수 (벽 포함), 안쪽 칸 수 */
  readonly h: number;
  readonly n: number;
  readonly m: number;
  readonly dt: number;
  readonly sdf: Sdf;
  /** 시각 (s) — 법칙 훅에 넘긴다 */
  t = 0;
  /** 법칙 훅 */
  field: Field3 | null = null;
  /** 켜면 스텝마다 구간별 ms 를 더한다 (벤치용) */
  prof: Float64Array | null = null;

  // --- 격자 (MAC: u 는 칸의 −x 면, v 는 −y 면, w 는 −z 면) ---
  private u: Float32Array;
  private v: Float32Array;
  private w: Float32Array;
  private du: Float32Array;
  private dv: Float32Array;
  private dw: Float32Array;
  private prevU: Float32Array;
  private prevV: Float32Array;
  private prevW: Float32Array;
  private tmp: Float32Array;
  /** 통 자체의 벽 (1 = 유체 가능, 0 = 벽) */
  private sStatic: Float32Array;
  /** 이번 스텝의 벽 — 통 벽 + 바디 */
  private s: Float32Array;
  /** 칸이 바디에 덮여 있으면 그 바디 번호, 아니면 −1 */
  readonly bodyOf: Int8Array;
  /** 바디 상자와 조금이라도 겹치는 칸 (1) — 밀도 보정을 건너뛴다 */
  readonly nearBody: Uint8Array;
  readonly cellType: Uint8Array;
  readonly density: Float32Array;
  /** 칸마다 닿아 있는 통 벽의 수 (0~3) */
  private wallCount: Uint8Array;
  /**
   * 벽 수별 기준 밀도. 복셀 격자(입자)와 압력 격자의 간격이 어긋나 칸이 읽는 밀도는 위치에 따라 다르다 —
   * 특히 벽 옆 칸은 입자 중심이 벽에서 r 떨어져 있어 속 칸과 다르게 읽힌다. 시드 직후(완벽한 격자)에
   * 벽 수마다 재서 그것을 "제자리" 로 삼는다. 해석값 (h/voxel)³ 하나만 쓰면 속이 2~3% 성기거나 벽 옆이
   * 20% 뭉친 것으로 보여, 물 전체가 부풀거나 벽 기둥이 비었다 (실측).
   */
  private restWall = new Float32Array(4);
  restDensity = 0;
  /** 바디 칸의 면 속도 — `markSolid` 가 채운다 (바디 수 × 3) */
  bodyVel: Float32Array = new Float32Array(0);

  // --- 입자 ---
  count = 0;
  readonly pos: Float32Array;
  readonly vel: Float32Array;
  private lastPos: Float32Array;
  private lastDt = 0;

  // --- 공간 해시 ---
  private pInv: number;
  private pn: number;
  private cellCount: Int32Array;
  private cellStart: Int32Array;
  private cellIds: Int32Array;
  private cellOf: Int32Array;
  private fluidCells: Int32Array;
  private fieldOut = new Float32Array(3);

  constructor(opts: Sim3Options) {
    this.res = Math.max(6, Math.floor(opts.res));
    this.voxel = BOX / this.res;
    // 안쪽 칸 수: 칸 = 복셀 / 0.6 에 가장 가까운 정수. 벽 두 칸을 더해 n.
    this.m = Math.max(4, Math.round(this.res * 2 * R_PER_CELL));
    this.h = BOX / this.m;
    this.n = this.m + 2;
    // 입자 지름 = 복셀 한 변. r/h 는 0.28~0.32 로 Müller 비율(0.3) 근처다.
    this.r = this.voxel / 2;
    this.dt = opts.dt ?? DT3;
    this.sdf = opts.sdf ?? boxSdf;
    const cells = this.n * this.n * this.n;
    this.u = new Float32Array(cells);
    this.v = new Float32Array(cells);
    this.w = new Float32Array(cells);
    this.du = new Float32Array(cells);
    this.dv = new Float32Array(cells);
    this.dw = new Float32Array(cells);
    this.prevU = new Float32Array(cells);
    this.prevV = new Float32Array(cells);
    this.prevW = new Float32Array(cells);
    this.tmp = new Float32Array(cells);
    this.sStatic = new Float32Array(cells);
    this.s = new Float32Array(cells);
    this.bodyOf = new Int8Array(cells).fill(-1);
    this.nearBody = new Uint8Array(cells);
    this.cellType = new Uint8Array(cells);
    this.density = new Float32Array(cells);
    this.wallCount = new Uint8Array(cells);
    this.fluidCells = new Int32Array(cells);

    this.pos = new Float32Array(MAX_PARTICLES3 * 3);
    this.vel = new Float32Array(MAX_PARTICLES3 * 3);
    this.lastPos = new Float32Array(MAX_PARTICLES3 * 3);

    const hashCell = (opts.hashCell ?? DEFAULT_HASH_CELL) * this.r;
    this.pInv = 1 / hashCell;
    this.pn = Math.floor(BOX * this.pInv) + 1;
    this.cellCount = new Int32Array(this.pn ** 3);
    this.cellStart = new Int32Array(this.pn ** 3 + 1);
    this.cellIds = new Int32Array(MAX_PARTICLES3);
    this.cellOf = new Int32Array(MAX_PARTICLES3);

    this.buildWalls();
    this.reseed(opts.fill, opts.seed);
  }

  /** 칸 번호 (i, j, k 는 벽 포함 0..n−1) */
  idx(i: number, j: number, k: number) { return (i * this.n + j) * this.n + k; }

  /** 좌표 → 칸 번호 (벽 칸은 0 과 n−1) */
  cellAt(x: number, y: number, z: number) {
    const { n, h } = this;
    const i = Math.max(0, Math.min(n - 1, Math.floor(x / h) + 1));
    const j = Math.max(0, Math.min(n - 1, Math.floor(y / h) + 1));
    const k = Math.max(0, Math.min(n - 1, Math.floor(z / h) + 1));
    return (i * n + j) * n + k;
  }

  private buildWalls() {
    const { n, h, sStatic, sdf } = this;
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        for (let k = 0; k < n; k++) {
          const border = i === 0 || j === 0 || k === 0 || i === n - 1 || j === n - 1 || k === n - 1;
          // 칸 중심이 벽면 밖(sdf > 0)이면 벽. 사각 통은 테두리 칸만 벽이 된다.
          const outside = !border && sdf((i - 0.5) * h, (j - 0.5) * h, (k - 0.5) * h) > 0;
          sStatic[(i * n + j) * n + k] = border || outside ? 0 : 1;
        }
      }
    }
    this.s.set(sStatic);
    const nn = n * n;
    for (let i = 1; i < n - 1; i++) for (let j = 1; j < n - 1; j++) for (let k = 1; k < n - 1; k++) {
      const c = (i * n + j) * n + k;
      this.wallCount[c] = (sStatic[c - nn] === 0 ? 1 : 0) + (sStatic[c + nn] === 0 ? 1 : 0) + (sStatic[c - n] === 0 ? 1 : 0)
        + (sStatic[c + n] === 0 ? 1 : 0) + (sStatic[c - 1] === 0 ? 1 : 0) + (sStatic[c + 1] === 0 ? 1 : 0);
      if (this.wallCount[c] > 3) this.wallCount[c] = 3;
    }
  }

  /** 바닥(+y)부터 `fill` 높이까지 복셀 중심마다 입자 하나. 속도 0. */
  reseed(fill: number, seed?: (x: number, y: number, z: number) => boolean) {
    const { res, voxel, r, sdf } = this;
    const layers = Math.round(Math.max(0, Math.min(1, fill)) * res);
    let c = 0;
    const pos = this.pos;
    outer: for (let iy = res - 1; iy >= res - layers; iy--) {
      const y = (iy + 0.5) * voxel;
      for (let ix = 0; ix < res; ix++) {
        const x = (ix + 0.5) * voxel;
        for (let iz = 0; iz < res; iz++) {
          const z = (iz + 0.5) * voxel;
          if (sdf(x, y, z) > -r * 0.999) continue; // 벽에 딱 붙은 복셀(= −r)은 둔다
          if (seed && !seed(x, y, z)) continue;
          if (c >= MAX_PARTICLES3) break outer;
          pos[3 * c] = x; pos[3 * c + 1] = y; pos[3 * c + 2] = z;
          c++;
        }
      }
    }
    this.count = c;
    this.vel.fill(0);
    this.u.fill(0); this.v.fill(0); this.w.fill(0);
    this.restDensity = 0;
    this.t = 0;
  }

  step(f: Forces3, p: SimParams3) {
    const dt = this.dt;
    const prof = this.prof;
    let t0 = prof ? performance.now() : 0;
    const lap = (k: number) => { if (prof) { const t = performance.now(); prof[k] += t - t0; t0 = t; } };
    this.lastPos.set(this.pos.subarray(0, this.count * 3));
    this.lastDt = dt;
    this.integrate(dt, f, p.drag); lap(0);
    this.separate(p.separationIters, p.cohesion); lap(1);
    this.collide(p.friction, p.restitution); lap(2);
    this.toGrid(); lap(3);
    this.updateDensity(); lap(4);
    this.prevU.set(this.u); this.prevV.set(this.v); this.prevW.set(this.w);
    if (p.viscosity > 0) this.smooth(p.viscosity, p.viscosityPasses); lap(5);
    this.solve(p.pressureIters, p.tension, p.densityK); lap(6);
    this.toParticles(p.flipRatio); lap(7);
    this.t += dt;
  }

  private integrate(dt: number, f: Forces3, drag: number) {
    const keep = Math.exp(-drag * dt);
    const c = BOX / 2;
    const { pos, vel, field, fieldOut, t } = this;
    const { wx, wy, wz, alx, aly, alz } = f;
    const spin = wx !== 0 || wy !== 0 || wz !== 0 || alx !== 0 || aly !== 0 || alz !== 0;
    for (let i = 0; i < this.count; i++) {
      const o = 3 * i;
      const x = pos[o], y = pos[o + 1], z = pos[o + 2];
      let vx = vel[o], vy = vel[o + 1], vz = vel[o + 2];
      let ax = f.ax, ay = f.ay, az = f.az;
      if (spin) {
        const rx = x - c, ry = y - c, rz = z - c;
        // 원심력 −ω×(ω×r) = ω²r − (ω·r)ω, 오일러력 −α×r, 코리올리 −2ω×v
        const w2 = wx * wx + wy * wy + wz * wz;
        const wr = wx * rx + wy * ry + wz * rz;
        ax += w2 * rx - wr * wx - (aly * rz - alz * ry) - 2 * (wy * vz - wz * vy);
        ay += w2 * ry - wr * wy - (alz * rx - alx * rz) - 2 * (wz * vx - wx * vz);
        az += w2 * rz - wr * wz - (alx * ry - aly * rx) - 2 * (wx * vy - wy * vx);
      }
      if (field) {
        fieldOut[0] = 0; fieldOut[1] = 0; fieldOut[2] = 0;
        field(x, y, z, t, fieldOut);
        ax += fieldOut[0]; ay += fieldOut[1]; az += fieldOut[2];
      }
      vx = (vx + ax * dt) * keep; vy = (vy + ay * dt) * keep; vz = (vz + az * dt) * keep;
      vel[o] = vx; vel[o + 1] = vy; vel[o + 2] = vz;
      pos[o] = x + vx * dt; pos[o + 1] = y + vy * dt; pos[o + 2] = z + vz * dt;
    }
  }

  /** 겹친 입자를 밀어내고, 응집이 켜져 있으면 조금 떨어진 이웃을 당긴다. */
  private separate(iters: number, cohesion: number) {
    const { pos, pInv, pn, cellCount, cellStart, cellIds, cellOf } = this;
    const count = this.count;
    const last = pn - 1;
    const pn2 = pn * pn;
    cellCount.fill(0);
    for (let i = 0; i < count; i++) {
      let cx = Math.floor(pos[3 * i] * pInv), cy = Math.floor(pos[3 * i + 1] * pInv), cz = Math.floor(pos[3 * i + 2] * pInv);
      cx = cx < 0 ? 0 : cx > last ? last : cx;
      cy = cy < 0 ? 0 : cy > last ? last : cy;
      cz = cz < 0 ? 0 : cz > last ? last : cz;
      const cell = (cx * pn + cy) * pn + cz;
      cellOf[i] = cell;
      cellCount[cell]++;
    }
    let first = 0;
    const total = pn2 * pn;
    for (let i = 0; i < total; i++) { first += cellCount[i]; cellStart[i] = first; }
    cellStart[total] = first;
    for (let i = 0; i < count; i++) { const cell = cellOf[i]; cellStart[cell]--; cellIds[cellStart[cell]] = i; }

    const minDist = 2 * this.r;
    const minDist2 = minDist * minDist;
    const range = minDist * COHESION_RANGE;
    const range2 = cohesion > 0 ? range * range : minDist2;
    const pull = (cohesion * 0.5) / (range - minDist);
    for (let it = 0; it < iters; it++) {
      for (let i = 0; i < count; i++) {
        const cell = cellOf[i];
        const xi = (cell / pn2) | 0;
        const rem = cell - xi * pn2;
        const yi = (rem / pn) | 0, zi = rem - yi * pn;
        const x0 = xi > 0 ? xi - 1 : 0, x1 = xi < last ? xi + 1 : last;
        const y0 = yi > 0 ? yi - 1 : 0, y1 = yi < last ? yi + 1 : last;
        const z0 = zi > 0 ? zi - 1 : 0, z1 = zi < last ? zi + 1 : last;
        let px = pos[3 * i], py = pos[3 * i + 1], pz = pos[3 * i + 2];
        for (let cx = x0; cx <= x1; cx++) {
          for (let cy = y0; cy <= y1; cy++) {
            const row = (cx * pn + cy) * pn;
            const kEnd = cellStart[row + z1 + 1];
            // 같은 (x,y) 열의 z0..z1 칸은 cellStart 에서 연속 — 한 번에 훑는다.
            for (let k = cellStart[row + z0]; k < kEnd; k++) {
              const id = cellIds[k];
              if (id <= i) continue;
              const dx = pos[3 * id] - px, dy = pos[3 * id + 1] - py, dz = pos[3 * id + 2] - pz;
              const d2 = dx * dx + dy * dy + dz * dz;
              if (d2 > range2 || d2 === 0) continue;
              const d = Math.sqrt(d2);
              const s = d < minDist ? (0.5 * (minDist - d)) / d : (-pull * (d - minDist) * (range - d)) / d;
              const mx = dx * s, my = dy * s, mz = dz * s;
              px -= mx; py -= my; pz -= mz;
              pos[3 * id] += mx; pos[3 * id + 1] += my; pos[3 * id + 2] += mz;
            }
          }
        }
        pos[3 * i] = px; pos[3 * i + 1] = py; pos[3 * i + 2] = pz;
      }
    }
  }

  private collide(friction: number, restitution: number) {
    const { pos, vel, r, sdf } = this;
    const lo = r, hi = BOX - r;
    const c = BOX / 2;
    const keep = 1 - friction;
    const isBox = sdf === boxSdf;
    const out = this.fieldOut;
    for (let i = 0; i < this.count; i++) {
      const o = 3 * i;
      let x = pos[o], y = pos[o + 1], z = pos[o + 2];
      let vx = vel[o], vy = vel[o + 1], vz = vel[o + 2];
      if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) { x = c; y = c; z = c; }
      if (!Number.isFinite(vx) || !Number.isFinite(vy) || !Number.isFinite(vz)) { vx = 0; vy = 0; vz = 0; }
      const d = isBox ? -1 : sdf(x, y, z);
      if (d > -r) {
        // 벽면 안쪽 r 까지 법선 방향으로 밀어 넣고, 벽으로 향하는 속도 성분을 반사·마찰한다.
        projectOut(sdf, x, y, z, r * 0.5, out);
        const nx = out[0], ny = out[1], nz = out[2];
        const push = d + r;
        x -= nx * push; y -= ny * push; z -= nz * push;
        const vn = vx * nx + vy * ny + vz * nz;
        if (vn > 0) {
          const tx = vx - vn * nx, ty = vy - vn * ny, tz = vz - vn * nz;
          vx = tx * keep - restitution * vn * nx;
          vy = ty * keep - restitution * vn * ny;
          vz = tz * keep - restitution * vn * nz;
        }
      }
      // 사각 경계는 어떤 모양에서도 안전망으로 건다.
      if (x < lo) { x = lo; if (vx < 0) { vx = -restitution * vx; vy *= keep; vz *= keep; } }
      if (x > hi) { x = hi; if (vx > 0) { vx = -restitution * vx; vy *= keep; vz *= keep; } }
      if (y < lo) { y = lo; if (vy < 0) { vy = -restitution * vy; vx *= keep; vz *= keep; } }
      if (y > hi) { y = hi; if (vy > 0) { vy = -restitution * vy; vx *= keep; vz *= keep; } }
      if (z < lo) { z = lo; if (vz < 0) { vz = -restitution * vz; vx *= keep; vy *= keep; } }
      if (z > hi) { z = hi; if (vz > 0) { vz = -restitution * vz; vx *= keep; vy *= keep; } }
      pos[o] = x; pos[o + 1] = y; pos[o + 2] = z;
      vel[o] = vx; vel[o + 1] = vy; vel[o + 2] = vz;
    }
  }

  /**
   * 삼선형 보간 가중치. `ox, oy, oz` 는 성분의 면 오프셋 (u: 0,h/2,h/2 / v: h/2,0,h/2 / w: h/2,h/2,0 /
   * 칸 중심: h/2,h/2,h/2). 좌표는 격자 공간(+h)으로 옮겨 [h, (n−1)h] 로 클램프한다.
   * 2D 와 달리 `x1 = x0 + 1` 을 그대로 쓴다 — 2D 는 `min(x0+1, n−2)` 로 오른쪽 벽 면을 한 번도 안 써
   * 표면이 왼쪽으로 1~2° 기울었다. 클램프 덕에 x0 ≤ n−2 이므로 x1 ≤ n−1 로 늘 범위 안이다.
   */
  private weights(x: number, y: number, z: number, ox: number, oy: number, oz: number) {
    const { n, h } = this;
    const h1 = 1 / h;
    const lim = (n - 1) * h;
    x += h; y += h; z += h;
    x = x < h ? h : x > lim ? lim : x;
    y = y < h ? h : y > lim ? lim : y;
    z = z < h ? h : z > lim ? lim : z;
    const x0 = Math.min(Math.floor((x - ox) * h1), n - 2);
    const y0 = Math.min(Math.floor((y - oy) * h1), n - 2);
    const z0 = Math.min(Math.floor((z - oz) * h1), n - 2);
    const tx = (x - ox) * h1 - x0, ty = (y - oy) * h1 - y0, tz = (z - oz) * h1 - z0;
    const sx = 1 - tx, sy = 1 - ty, sz = 1 - tz;
    W.d0 = sx * sy * sz; W.d1 = tx * sy * sz; W.d2 = tx * ty * sz; W.d3 = sx * ty * sz;
    W.d4 = sx * sy * tz; W.d5 = tx * sy * tz; W.d6 = tx * ty * tz; W.d7 = sx * ty * tz;
    const b = (x0 * n + y0) * n + z0;
    W.n0 = b; W.n1 = b + n * n; W.n2 = b + n * n + n; W.n3 = b + n;
    W.n4 = b + 1; W.n5 = b + n * n + 1; W.n6 = b + n * n + n + 1; W.n7 = b + n + 1;
    return W;
  }

  private toGrid() {
    const { n, h, pos, vel, s, cellType, bodyOf, bodyVel } = this;
    const total = n * n * n;
    for (let i = 0; i < total; i++) cellType[i] = s[i] === 0 ? SOLID : AIR;
    for (let i = 0; i < this.count; i++) {
      const cell = this.cellAt(pos[3 * i], pos[3 * i + 1], pos[3 * i + 2]);
      if (cellType[cell] === AIR) cellType[cell] = FLUID;
    }
    const half = h / 2;
    for (let comp = 0; comp < 3; comp++) {
      const f = comp === 0 ? this.u : comp === 1 ? this.v : this.w;
      const d = comp === 0 ? this.du : comp === 1 ? this.dv : this.dw;
      f.fill(0); d.fill(0);
      const ox = comp === 0 ? 0 : half, oy = comp === 1 ? 0 : half, oz = comp === 2 ? 0 : half;
      for (let i = 0; i < this.count; i++) {
        const w = this.weights(pos[3 * i], pos[3 * i + 1], pos[3 * i + 2], ox, oy, oz);
        const pv = vel[3 * i + comp];
        f[w.n0] += pv * w.d0; d[w.n0] += w.d0;
        f[w.n1] += pv * w.d1; d[w.n1] += w.d1;
        f[w.n2] += pv * w.d2; d[w.n2] += w.d2;
        f[w.n3] += pv * w.d3; d[w.n3] += w.d3;
        f[w.n4] += pv * w.d4; d[w.n4] += w.d4;
        f[w.n5] += pv * w.d5; d[w.n5] += w.d5;
        f[w.n6] += pv * w.d6; d[w.n6] += w.d6;
        f[w.n7] += pv * w.d7; d[w.n7] += w.d7;
      }
      for (let i = 0; i < total; i++) if (d[i] > 0) f[i] /= d[i];
    }
    // 벽에 닿은 면의 속도는 벽의 속도 — 통 벽은 0, 바디 칸은 바디 속도.
    const nn = n * n;
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n; j++) {
        for (let k = 0; k < n; k++) {
          const c = (i * n + j) * n + k;
          const solid = cellType[c] === SOLID;
          const b = bodyOf[c];
          if (solid || (i > 0 && cellType[c - nn] === SOLID)) {
            const bb = solid ? b : bodyOf[c - nn];
            this.u[c] = bb >= 0 ? bodyVel[3 * bb] : 0; this.du[c] = 0;
          }
          if (solid || (j > 0 && cellType[c - n] === SOLID)) {
            const bb = solid ? b : bodyOf[c - n];
            this.v[c] = bb >= 0 ? bodyVel[3 * bb + 1] : 0; this.dv[c] = 0;
          }
          if (solid || (k > 0 && cellType[c - 1] === SOLID)) {
            const bb = solid ? b : bodyOf[c - 1];
            this.w[c] = bb >= 0 ? bodyVel[3 * bb + 2] : 0; this.dw[c] = 0;
          }
        }
      }
    }
  }

  private updateDensity() {
    const { n, h, pos, density: d } = this;
    d.fill(0);
    const half = h / 2;
    // 벽 옆 입자의 가중치가 벽 칸으로 새지 않게 표본 위치를 첫 유체 칸 중심 안쪽으로 모은다 —
    // 새면 그만큼 질량이 사라져 벽 옆 칸이 물의 움직임과 무관하게 성기게 읽힌다. 남는 격자 어긋남은
    // 벽 수별 기준 밀도(`restWall`)가 흡수한다.
    const lo = half, hi = (n - 2) * h - half;
    for (let i = 0; i < this.count; i++) {
      const x = Math.max(lo, Math.min(hi, pos[3 * i])), y = Math.max(lo, Math.min(hi, pos[3 * i + 1])), z = Math.max(lo, Math.min(hi, pos[3 * i + 2]));
      const w = this.weights(x, y, z, half, half, half);
      d[w.n0] += w.d0; d[w.n1] += w.d1; d[w.n2] += w.d2; d[w.n3] += w.d3;
      d[w.n4] += w.d4; d[w.n5] += w.d5; d[w.n6] += w.d6; d[w.n7] += w.d7;
    }
    if (this.restDensity === 0) this.measureRest();
  }

  /** 시드 직후의 격자에서 벽 수별 기준 밀도를 잰다 — 공기에 닿은 칸은 뺀다. 표본이 없으면 해석값. */
  private measureRest() {
    const { n, cellType, density: d, wallCount } = this;
    const nn = n * n;
    const sum = [0, 0, 0, 0], cnt = [0, 0, 0, 0];
    for (let i = 1; i < n - 1; i++) {
      for (let j = 1; j < n - 1; j++) {
        for (let k = 1; k < n - 1; k++) {
          const c = (i * n + j) * n + k;
          if (cellType[c] !== FLUID) continue;
          if (cellType[c - nn] === AIR || cellType[c + nn] === AIR || cellType[c - n] === AIR
            || cellType[c + n] === AIR || cellType[c - 1] === AIR || cellType[c + 1] === AIR) continue;
          sum[wallCount[c]] += d[c]; cnt[wallCount[c]]++;
        }
      }
    }
    const analytic = Math.pow(this.h / this.voxel, 3);
    for (let w = 0; w < 4; w++) this.restWall[w] = cnt[w] > 0 ? sum[w] / cnt[w] : (w > 0 && cnt[w - 1] > 0 ? this.restWall[w - 1] : analytic);
    this.restDensity = this.restWall[0];
  }

  private smooth(strength: number, passes: number) {
    const { n, tmp } = this;
    const nn = n * n;
    for (let comp = 0; comp < 3; comp++) {
      const f = comp === 0 ? this.u : comp === 1 ? this.v : this.w;
      const d = comp === 0 ? this.du : comp === 1 ? this.dv : this.dw;
      for (let pass = 0; pass < passes; pass++) {
        tmp.set(f);
        for (let i = 1; i < n - 1; i++) {
          for (let j = 1; j < n - 1; j++) {
            for (let k = 1; k < n - 1; k++) {
              const c = (i * n + j) * n + k;
              if (d[c] <= 0) continue;
              let sum = 0, cnt = 0;
              if (d[c - nn] > 0) { sum += tmp[c - nn]; cnt++; }
              if (d[c + nn] > 0) { sum += tmp[c + nn]; cnt++; }
              if (d[c - n] > 0) { sum += tmp[c - n]; cnt++; }
              if (d[c + n] > 0) { sum += tmp[c + n]; cnt++; }
              if (d[c - 1] > 0) { sum += tmp[c - 1]; cnt++; }
              if (d[c + 1] > 0) { sum += tmp[c + 1]; cnt++; }
              if (cnt > 0) f[c] = tmp[c] + strength * (sum / cnt - tmp[c]);
            }
          }
        }
      }
    }
  }

  /** 비압축성 — Gauss-Seidel + 과이완, 밀도 보정(뭉침은 밀어내고 성김은 tension 만큼 당김). */
  private solve(iters: number, tension: number, densityK: number) {
    const { n, u, v, w, s, cellType, density, fluidCells } = this;
    const nn = n * n;
    const over = 1.9;
    let count = 0;
    for (let i = 1; i < n - 1; i++) {
      for (let j = 1; j < n - 1; j++) {
        for (let k = 1; k < n - 1; k++) {
          const c = (i * n + j) * n + k;
          if (cellType[c] === FLUID && s[c - nn] + s[c + nn] + s[c - n] + s[c + n] + s[c - 1] + s[c + 1] > 0) fluidCells[count++] = c;
        }
      }
    }
    const restWall = this.restWall, wallCount = this.wallCount;
    const hasRest = this.restDensity > 0;
    for (let it = 0; it < iters; it++) {
      const backward = it % 2 === 1;
      for (let kk = 0; kk < count; kk++) {
        const c = fluidCells[backward ? count - 1 - kk : kk];
        const sx0 = s[c - nn], sx1 = s[c + nn], sy0 = s[c - n], sy1 = s[c + n], sz0 = s[c - 1], sz1 = s[c + 1];
        const sum = sx0 + sx1 + sy0 + sy1 + sz0 + sz1;
        const div = u[c + nn] - u[c] + v[c + n] - v[c] + w[c + 1] - w[c];
        let p = (-div / sum) * over;
        // 바디 상자와 겹치는 칸은 밀도 보정을 하지 않는다 — 입자가 칸의 일부에만 들어갈 수 있어 성기게 읽히고,
        // 당기면 그 좁은 틈으로 입자를 욱여넣다 분리와 싸워 바디 둘레에 구덩이가 남았다 (실측).
        if (hasRest && this.nearBody[c] === 0) {
          const compression = (density[c] - restWall[wallCount[c]]) * densityK;
          let corr = 0;
          if (compression > 0) corr = compression;
          else if (cellType[c - nn] !== AIR && cellType[c + nn] !== AIR && cellType[c - n] !== AIR
            && cellType[c + n] !== AIR && cellType[c - 1] !== AIR && cellType[c + 1] !== AIR) {
            // 성긴 칸 당기기는 **공기에 닿지 않은 칸만**. 수면 칸은 가중치 절반이 공기로 새어 늘 성기게 읽히는데,
            // 거기서 당기면 수면 전체가 매 스텝 아래로 1.6 m/s 씩 끌려 내려가 아래층과 부딪히며 떨었다
            // (실측: PIC 물이 옆으로 밀어도 안 움직이고, 고인 물 잡음이 0.16 m/s).
            corr = tension * compression;
          }
          // 밀도 보정은 발산과 **따로** 여섯 면에 고르게 나눈다(벽 면 몫은 버린다). 발산과 합쳐 열린 면 수로
          // 나누면 벽 옆 칸이 안쪽으로 1/5, 속 칸은 1/6 씩 밀어 매 스텝 벽에서 안쪽으로 조금씩 흐른다 —
          // 4초면 벽 기둥 입자의 25% 가 빠져나갔다 (실측, 분리·응집·PIC 와 무관).
          p += (corr / 6) * over;
        }
        u[c] -= sx0 * p; u[c + nn] += sx1 * p;
        v[c] -= sy0 * p; v[c + n] += sy1 * p;
        w[c] -= sz0 * p; w[c + 1] += sz1 * p;
      }
    }
  }

  private toParticles(flipRatio: number) {
    const { n, h, pos, vel, cellType } = this;
    const half = h / 2;
    const nn = n * n;
    for (let comp = 0; comp < 3; comp++) {
      const f = comp === 0 ? this.u : comp === 1 ? this.v : this.w;
      const prev = comp === 0 ? this.prevU : comp === 1 ? this.prevV : this.prevW;
      const offset = comp === 0 ? nn : comp === 1 ? n : 1;
      const ox = comp === 0 ? 0 : half, oy = comp === 1 ? 0 : half, oz = comp === 2 ? 0 : half;
      for (let i = 0; i < this.count; i++) {
        const w = this.weights(pos[3 * i], pos[3 * i + 1], pos[3 * i + 2], ox, oy, oz);
        // 어느 쪽도 유체가 아닌 면은 뜻이 없다 — 빼고 가중 평균한다. 2D 는 "둘 다 공기" 만 뺐는데, 그러면
        // 벽 칸 안의 면(속도 0)이 벽 옆 입자의 **접선** 속도를 26% 깎는다. 가만히 고인 물도 매 스텝 g·dt² 만큼
        // 가라앉았다가 밀도 보정 흐름으로 되올라오는데, 벽 옆만 덜 올라와 눌리고 안쪽으로 밀려났다
        // (실측: 4초에 벽 기둥 입자 25% 이탈, 수면 1.5 복셀 낮음). 벽에 수직인 성분은 유체-벽 면이 유체 쪽
        // 칸 덕에 살아남아 그대로 0 으로 막힌다 (free-slip).
        const v0 = (cellType[w.n0] === FLUID || cellType[w.n0 - offset] === FLUID ? 1 : 0) * w.d0;
        const v1 = (cellType[w.n1] === FLUID || cellType[w.n1 - offset] === FLUID ? 1 : 0) * w.d1;
        const v2 = (cellType[w.n2] === FLUID || cellType[w.n2 - offset] === FLUID ? 1 : 0) * w.d2;
        const v3 = (cellType[w.n3] === FLUID || cellType[w.n3 - offset] === FLUID ? 1 : 0) * w.d3;
        const v4 = (cellType[w.n4] === FLUID || cellType[w.n4 - offset] === FLUID ? 1 : 0) * w.d4;
        const v5 = (cellType[w.n5] === FLUID || cellType[w.n5 - offset] === FLUID ? 1 : 0) * w.d5;
        const v6 = (cellType[w.n6] === FLUID || cellType[w.n6 - offset] === FLUID ? 1 : 0) * w.d6;
        const v7 = (cellType[w.n7] === FLUID || cellType[w.n7 - offset] === FLUID ? 1 : 0) * w.d7;
        const d = v0 + v1 + v2 + v3 + v4 + v5 + v6 + v7;
        if (d <= 0) continue;
        const pic = (v0 * f[w.n0] + v1 * f[w.n1] + v2 * f[w.n2] + v3 * f[w.n3]
          + v4 * f[w.n4] + v5 * f[w.n5] + v6 * f[w.n6] + v7 * f[w.n7]) / d;
        const corr = (v0 * (f[w.n0] - prev[w.n0]) + v1 * (f[w.n1] - prev[w.n1]) + v2 * (f[w.n2] - prev[w.n2]) + v3 * (f[w.n3] - prev[w.n3])
          + v4 * (f[w.n4] - prev[w.n4]) + v5 * (f[w.n5] - prev[w.n5]) + v6 * (f[w.n6] - prev[w.n6]) + v7 * (f[w.n7] - prev[w.n7])) / d;
        const flip = vel[3 * i + comp] + corr;
        vel[3 * i + comp] = (1 - flipRatio) * pic + flipRatio * flip;
      }
    }
  }

  // --- 바디가 쓰는 격자 접근 ---------------------------------------------------------

  /** 이번 스텝의 벽을 통 벽으로 되돌린다 — `bodies.markSolid` 가 그 위에 바디 칸을 찍는다. */
  resetSolids() {
    this.s.set(this.sStatic);
    this.bodyOf.fill(-1);
    this.nearBody.fill(0);
  }

  /** 칸을 바디 `id` 의 벽으로 표시한다. */
  setBodyCell(cell: number, id: number) {
    this.s[cell] = 0;
    this.bodyOf[cell] = id;
  }

  /** 격자 속도를 한 점에서 샘플한다 (바디 항력용). */
  sampleVelocity(x: number, y: number, z: number, out: Float32Array) {
    const half = this.h / 2;
    for (let comp = 0; comp < 3; comp++) {
      const f = comp === 0 ? this.u : comp === 1 ? this.v : this.w;
      const w = this.weights(x, y, z, comp === 0 ? 0 : half, comp === 1 ? 0 : half, comp === 2 ? 0 : half);
      out[comp] = f[w.n0] * w.d0 + f[w.n1] * w.d1 + f[w.n2] * w.d2 + f[w.n3] * w.d3
        + f[w.n4] * w.d4 + f[w.n5] * w.d5 + f[w.n6] * w.d6 + f[w.n7] * w.d7;
    }
  }

  /** 통 벽(바디 제외) 칸인가 */
  isWall(cell: number) { return this.sStatic[cell] === 0; }

  // --- 측정 --------------------------------------------------------------------------

  meanSpeed2(): number {
    if (this.count === 0) return 0;
    let sum = 0;
    for (let i = 0; i < this.count * 3; i++) sum += this.vel[i] * this.vel[i];
    return sum / this.count;
  }

  /** 직전 스텝에서 입자가 실제로 움직인 평균 속력 (m/s) — 잠들기 판정 (`sim.ts` 와 같은 이유). */
  motion(): number {
    if (this.count === 0 || this.lastDt <= 0) return 0;
    let sum = 0;
    const { pos, lastPos } = this;
    for (let i = 0; i < this.count; i++) {
      const o = 3 * i;
      sum += Math.hypot(pos[o] - lastPos[o], pos[o + 1] - lastPos[o + 1], pos[o + 2] - lastPos[o + 2]);
    }
    return sum / this.count / this.lastDt;
  }

  centerOfMass(): { x: number; y: number; z: number } {
    let x = 0, y = 0, z = 0;
    for (let i = 0; i < this.count; i++) { x += this.pos[3 * i]; y += this.pos[3 * i + 1]; z += this.pos[3 * i + 2]; }
    const c = this.count || 1;
    return this.count ? { x: x / c, y: y / c, z: z / c } : { x: BOX / 2, y: BOX / 2, z: BOX / 2 };
  }
}

/** `weights` 결과 버퍼 (단일 스레드 재사용) */
const W = {
  d0: 0, d1: 0, d2: 0, d3: 0, d4: 0, d5: 0, d6: 0, d7: 0,
  n0: 0, n1: 0, n2: 0, n3: 0, n4: 0, n5: 0, n6: 0, n7: 0,
};
