/**
 * 복셀 → 등각(2:1) 픽셀아트. 복셀 한 개는 윗면 마름모(4U×2U)와 두 옆면(각 2U×2U)의 육각형이다.
 * 버퍼는 작은 픽셀 그림이고, 화면에서는 정수 배로 **보간 없이** 확대한다 (`engine.ts`).
 *
 * 투영: 복셀 공간 (x, y, z) → 버퍼 px
 *   sx = ox + 2U·(x − z),  sy = U·(x + z) + 2U·y
 * x 는 화면 오른쪽 아래, z 는 왼쪽 아래, y 는 아래로 뻗는다. 시선은 (+x, −y, +z) 쪽에서 본다.
 * 그래서 보이는 면은 윗면(−y)·+x 면(오른쪽)·+z 면(왼쪽)이고, 깊이는 x − y + z 가 클수록 가깝다 —
 * 그 순서(페인터)로 그리면 가까운 복셀이 먼 것을 덮는다. 공기에 닿은 면만 그린다.
 *
 * 색은 물 색 하나를 밝히고 어둡혀 만든다(새 색을 하드코딩하지 않는다). DOM 에 의존하지 않는다.
 */
import type { Rgb } from "./render";
import { AIR, BODY0, WATER, type VoxelField } from "./voxels";

/** 복셀 반 변의 버퍼 px — 1 이면 윗면이 2×2 사각형으로 뭉개진다. 2 가 가장 작은 진짜 마름모다. */
export const U = 2;

export interface IsoLayout {
  res: number;
  u: number;
  bufW: number;
  bufH: number;
  /** 복셀 (0,·,0) 세로선의 x */
  ox: number;
}

export function isoLayout(res: number, u = U): IsoLayout {
  return { res, u, bufW: 4 * u * res, bufH: 4 * u * res, ox: 2 * u * res };
}

/** 복셀 공간의 점 → 버퍼 px (복셀 (i,j,k) 의 윗면 중심은 (i+.5, j, k+.5)) */
export function project(l: IsoLayout, x: number, y: number, z: number): { sx: number; sy: number } {
  return { sx: l.ox + 2 * l.u * (x - z), sy: l.u * (x + z) + 2 * l.u * y };
}

export interface IsoBuffer {
  w: number;
  h: number;
  rgba: Uint8ClampedArray;
  /** 픽셀마다 그려진 재질 (0 = 비었음) — 집기(picking) */
  pick: Uint8Array;
}

export function newIsoBuffer(l: IsoLayout): IsoBuffer {
  return { w: l.bufW, h: l.bufH, rgba: new Uint8ClampedArray(l.bufW * l.bufH * 4), pick: new Uint8Array(l.bufW * l.bufH) };
}

export interface IsoPalette {
  water: Rgb;
  /** 바디 i 의 색 (재질 BODY0 + i) */
  bodies: Rgb[];
}

export interface IsoOptions {
  /** 옆면에 일렁이는 빛 무늬 */
  caustics?: boolean;
  /** 무늬의 시각 (s) — 잠든 뒤에는 그리지 않으므로 저절로 멈춘다 */
  t?: number;
}

const TOP_LIGHT = 0.25;
const LEFT_DARK = 0.22;
/** 윗면 가장자리(옆이 공기인 물) — 수면 윤곽 */
const RIM_LIGHT = 0.18;
const FOAM_LIGHT = 0.5;
/** caustic 한 칸의 밝힘과 문턱 — 사인파 둘의 곱이 문턱을 넘는 복셀 면만 밝힌다 (픽셀아트의 얼룩) */
const CAUSTIC_LIGHT = 0.22;
const CAUSTIC_THRESHOLD = 0.55;

/** 옆면 복셀 (ix, iy, iz) 가 이 시각에 caustic 얼룩인가 */
export function causticAt(ix: number, iy: number, iz: number, t: number): boolean {
  const a = Math.sin(1.9 * ix + 1.3 * iy - 1.6 * t) * Math.sin(1.4 * iz - 1.1 * iy + 1.2 * t + 0.7);
  return a > CAUSTIC_THRESHOLD;
}

/** 면 마스크 — 윗면 중심 기준 픽셀 오프셋 (px, py 쌍) */
interface Masks { top: Int8Array; left: Int8Array; right: Int8Array }
const maskCache = new Map<number, Masks>();

function masks(u: number): Masks {
  let m = maskCache.get(u);
  if (m) return m;
  const top: number[] = [], left: number[] = [], right: number[] = [];
  for (let py = -u; py < 3 * u; py++) {
    for (let px = -2 * u; px < 2 * u; px++) {
      const cx = px + 0.5, cy = py + 0.5;
      if (Math.abs(cx) / (2 * u) + Math.abs(cy) / u <= 1) { top.push(px, py); continue; }
      // 옆면 — 윗면 아래쪽 변에서 2U 내려간 평행사변형
      const yTop = cx < 0 ? u + cx / 2 : u - cx / 2;
      if (cy > yTop && cy < yTop + 2 * u) (cx < 0 ? left : right).push(px, py);
    }
  }
  m = { top: Int8Array.from(top), left: Int8Array.from(left), right: Int8Array.from(right) };
  maskCache.set(u, m);
  return m;
}

const lighten = (c: number, t: number) => c + (255 - c) * t;
const darken = (c: number, t: number) => c * (1 - t);

export function paintIso(f: VoxelField, l: IsoLayout, out: IsoBuffer, pal: IsoPalette, opts: IsoOptions = {}) {
  const caustics = !!opts.caustics;
  const t = opts.t ?? 0;
  const { res, mat, foam } = f;
  const { u, ox, bufW } = l;
  const { rgba, pick } = out;
  rgba.fill(0);
  pick.fill(0);
  const mk = masks(u);
  const rr = res * res;
  const blit = (mask: Int8Array, cx: number, cy: number, r: number, g: number, b: number, m: number) => {
    for (let q = 0; q < mask.length; q += 2) {
      const x = cx + mask[q], y = cy + mask[q + 1];
      const p = y * bufW + x;
      const o = p * 4;
      rgba[o] = r; rgba[o + 1] = g; rgba[o + 2] = b; rgba[o + 3] = 255;
      pick[p] = m;
    }
  };
  // 깊이 s = ix − iy + iz 오름차순
  for (let s = -(res - 1); s <= 2 * (res - 1); s++) {
    for (let iy = 0; iy < res; iy++) {
      const izBase = s + iy;
      const ixLo = Math.max(0, izBase - (res - 1)), ixHi = Math.min(res - 1, izBase);
      for (let ix = ixLo; ix <= ixHi; ix++) {
        const iz = izBase - ix;
        const i = (ix * res + iy) * res + iz;
        const m = mat[i];
        if (m === AIR) continue;
        const topV = iy === 0 || mat[i - res] === AIR;
        const rightV = ix === res - 1 || mat[i + rr] === AIR;
        const leftV = iz === res - 1 || mat[i + 1] === AIR;
        if (!topV && !rightV && !leftV) continue;
        const c = m === WATER ? pal.water : pal.bodies[m - BODY0] ?? pal.water;
        const cx = ox + 2 * u * (ix - iz);
        const cy = u * (ix + iz) + 2 * u * iy;
        if (topV) {
          let t = TOP_LIGHT + (m === WATER ? foam[i] * FOAM_LIGHT : 0);
          if (m === WATER) {
            const rim = (ix > 0 && mat[i - rr] === AIR) || (ix < res - 1 && mat[i + rr] === AIR)
              || (iz > 0 && mat[i - 1] === AIR) || (iz < res - 1 && mat[i + 1] === AIR);
            if (rim) t += RIM_LIGHT;
          }
          t = Math.min(1, t);
          blit(mk.top, cx, cy, lighten(c.r, t), lighten(c.g, t), lighten(c.b, t), m);
        }
        // 옆면 — 수면 바로 아래부터 일렁이는 빛 무늬 (윗면이 보이는 복셀은 수면이라 뺀다)
        const ca = caustics && m === WATER && !topV && causticAt(ix, iy, iz, t) ? CAUSTIC_LIGHT : 0;
        if (rightV) blit(mk.right, cx, cy, lighten(c.r, ca), lighten(c.g, ca), lighten(c.b, ca), m);
        if (leftV) {
          const d = LEFT_DARK - ca;
          blit(mk.left, cx, cy, darken(c.r, d), darken(c.g, d), darken(c.b, d), m);
        }
      }
    }
  }
}

/** 버퍼 px 에 그려진 재질 (0 = 비었음) */
export function pickAt(buf: IsoBuffer, x: number, y: number): number {
  const ix = Math.floor(x), iy = Math.floor(y);
  if (ix < 0 || iy < 0 || ix >= buf.w || iy >= buf.h) return 0;
  return buf.pick[iy * buf.w + ix];
}

/** 복셀 하나의 면 픽셀 수 — 테스트·예산 계산용 */
export function facePixels(u = U) {
  const m = masks(u);
  return { top: m.top.length / 2, left: m.left.length / 2, right: m.right.length / 2 };
}
