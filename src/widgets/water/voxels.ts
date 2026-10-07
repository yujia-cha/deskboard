/**
 * 복셀 장 — 등각 그림의 입력. 한 변 `res` 복셀, 복셀마다 재질(0 공기 · 1 물 · 2+ 바디)과
 * 밝기 보정(거품)을 든다. 입자(3D)나 2D 래스터를 여기로 옮긴 뒤 `iso.ts` 가 그린다.
 *
 * 입자 → 복셀은 **삼선형 splat + 히스테리시스**다. 입자를 그냥 `floor` 로 찍으면 입자 간격이 복셀과
 * 어긋나는 곳마다 구멍이 뚫리고, 경계에 걸친 입자는 매 스텝 켜졌다 꺼졌다 깜빡인다.
 * 켜지는 문턱(0.6)과 꺼지는 문턱(0.4)을 달리 두면 경계 복셀이 떨지 않는다.
 * DOM 에 의존하지 않는다.
 */
import type { FlipSim3 } from "./sim3";
import type { Raster } from "./render";
import { BOX } from "./sim";

export const AIR = 0;
export const WATER = 1;
/** 바디 재질 번호의 시작 — 바디 i 는 재질 BODY0 + i */
export const BODY0 = 2;

export interface VoxelField {
  res: number;
  /** 재질 */
  mat: Uint8Array;
  /** 거품 밝힘 0~1 */
  foam: Float32Array;
  /** splat 누적 (물) */
  acc: Float32Array;
  /** 속력² splat 누적 */
  spd: Float32Array;
}

export function newField(res: number): VoxelField {
  const n = res * res * res;
  return { res, mat: new Uint8Array(n), foam: new Float32Array(n), acc: new Float32Array(n), spd: new Float32Array(n) };
}

/** 복셀 번호 */
export function vIdx(res: number, ix: number, iy: number, iz: number) {
  return (ix * res + iy) * res + iz;
}

/**
 * 켜지는 / 꺼지는 문턱. 흐트러진 물은 복셀마다 가중치가 1 안팎으로 흩어진다 — 0.6 으로 켜면 질량의
 * 10% 가 안 보여 물이 줄어든 것처럼 보였다(실측). 0.5 면 6% 안쪽이고 나머지는 `fill6` 이 메운다.
 */
export const ON_AT = 0.5;
export const OFF_AT = 0.35;
/** 거품으로 칠하기 시작하는 속력 / 완전히 밝아지는 속력 (m/s) — render.ts 와 같은 값 */
const FOAM_FROM = 1.2;
const FOAM_FULL = 3.5;

/**
 * 입자를 복셀로. 물이 아닌 재질(바디)은 건드리지 않는다 — 바디는 그 뒤에 `paintBodies` 가 덮어쓴다.
 */
export function splat3(sim: FlipSim3, f: VoxelField) {
  const { res, mat, acc, spd } = f;
  acc.fill(0);
  spd.fill(0);
  const k = res / BOX;
  const last = res - 1;
  const { pos, vel } = sim;
  for (let p = 0; p < sim.count; p++) {
    const o = 3 * p;
    // 복셀 중심이 (i + 0.5)·voxel 이므로 0.5 를 빼면 i 와 i+1 사이의 비율이 나온다.
    const gx = pos[o] * k - 0.5, gy = pos[o + 1] * k - 0.5, gz = pos[o + 2] * k - 0.5;
    let x0 = Math.floor(gx), y0 = Math.floor(gy), z0 = Math.floor(gz);
    const tx = gx - x0, ty = gy - y0, tz = gz - z0;
    // 벽에 붙은 입자는 바깥 복셀이 없다 — 가중치를 안쪽으로 몰아 벽 복셀이 꽉 차게 한다.
    let x1 = x0 + 1, y1 = y0 + 1, z1 = z0 + 1;
    if (x0 < 0) x0 = 0; if (x1 > last) x1 = last;
    if (y0 < 0) y0 = 0; if (y1 > last) y1 = last;
    if (z0 < 0) z0 = 0; if (z1 > last) z1 = last;
    const sx = 1 - tx, sy = 1 - ty, sz = 1 - tz;
    const vx = vel[o], vy = vel[o + 1], vz = vel[o + 2];
    const s2 = vx * vx + vy * vy + vz * vz;
    const put = (ix: number, iy: number, iz: number, w: number) => {
      const i = (ix * res + iy) * res + iz;
      acc[i] += w;
      spd[i] += s2 * w;
    };
    put(x0, y0, z0, sx * sy * sz); put(x1, y0, z0, tx * sy * sz);
    put(x1, y1, z0, tx * ty * sz); put(x0, y1, z0, sx * ty * sz);
    put(x0, y0, z1, sx * sy * tz); put(x1, y0, z1, tx * sy * tz);
    put(x1, y1, z1, tx * ty * tz); put(x0, y1, z1, sx * ty * tz);
  }
  const n = res * res * res;
  for (let i = 0; i < n; i++) {
    const m = mat[i];
    if (m >= BODY0) { mat[i] = AIR; } // 지난 프레임의 바디 — 다시 칠해진다
    const a = acc[i];
    const was = m === WATER;
    const on = a >= ON_AT || (was && a >= OFF_AT);
    mat[i] = on ? WATER : AIR;
    if (on) {
      const sp = Math.sqrt(spd[i] / Math.max(a, 1e-6));
      f.foam[i] = sp <= FOAM_FROM ? 0 : Math.min(1, (sp - FOAM_FROM) / (FOAM_FULL - FOAM_FROM));
    } else f.foam[i] = 0;
  }
  fill6(f);
}

/** 여섯 이웃 중 다섯 이상이 물인 빈 복셀을 메운다 — 물 한가운데 점점이 뚫린 구멍. 원본만 보고 판단해 번지지 않는다. */
export function fill6(f: VoxelField) {
  const { res, mat, acc } = f;
  const rr = res * res;
  const holes: number[] = [];
  for (let ix = 1; ix < res - 1; ix++) {
    for (let iy = 1; iy < res - 1; iy++) {
      for (let iz = 1; iz < res - 1; iz++) {
        const i = (ix * res + iy) * res + iz;
        if (mat[i] !== AIR || acc[i] <= 0) continue;
        const w = (mat[i - rr] === WATER ? 1 : 0) + (mat[i + rr] === WATER ? 1 : 0) + (mat[i - res] === WATER ? 1 : 0)
          + (mat[i + res] === WATER ? 1 : 0) + (mat[i - 1] === WATER ? 1 : 0) + (mat[i + 1] === WATER ? 1 : 0);
        if (w >= 5) holes.push(i);
      }
    }
  }
  for (const i of holes) { mat[i] = WATER; f.foam[i] = 0; }
}

/** 2D 래스터(측면 단면)를 깊이 방향으로 늘여 복셀로 — 3D 가 없을 때의 폴백. */
export function extrude2d(r: Raster, f: VoxelField) {
  const { res, mat, foam } = f;
  if (r.res !== res) throw new Error(`extrude2d: res ${r.res} != ${res}`);
  for (let iy = 0; iy < res; iy++) {
    for (let ix = 0; ix < res; ix++) {
      const src = iy * res + ix;
      const on = r.occ[src] !== 0;
      const sp = Math.sqrt(r.speed[src]);
      const fm = sp <= FOAM_FROM ? 0 : Math.min(1, (sp - FOAM_FROM) / (FOAM_FULL - FOAM_FROM));
      for (let iz = 0; iz < res; iz++) {
        const i = (ix * res + iy) * res + iz;
        mat[i] = on ? WATER : AIR;
        foam[i] = on ? fm : 0;
      }
    }
  }
}

/**
 * 수면 높이 — (ix, iz) 기둥에서 가장 위(작은 iy)의 물 복셀 번호. 물이 없으면 res.
 * 바디 부력(수면 평면)과 테스트가 쓴다.
 */
export function surfaceAt(f: VoxelField, ix: number, iz: number): number {
  const { res, mat } = f;
  for (let iy = 0; iy < res; iy++) if (mat[(ix * res + iy) * res + iz] === WATER) return iy;
  return res;
}
