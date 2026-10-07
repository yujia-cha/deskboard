/**
 * 입자 → 픽셀. 통 한 변 `res` 픽셀의 RGBA 버퍼에 입자 하나를 한 픽셀로 찍는다.
 * 화면에서는 이 작은 이미지를 **보간 없이** 확대해 계단 모양 픽셀을 그대로 살린다.
 *
 * 색은 하나(강조색)에 셰이딩만 더한다:
 *  - 가장자리(이웃 중 빈 칸이 있는 픽셀)는 밝게 — 물 표면과 물방울 윤곽
 *  - 빠르게 움직이는 입자는 거품처럼 더 밝게
 * 밝히는 것은 같은 색을 흰 쪽으로 섞는 계산이다 — 새 색을 하드코딩하지 않는다.
 *
 * DOM 에 의존하지 않는다 (테스트 가능).
 */
import { BOX, type FlipSim } from "./sim";

export interface Rgb { r: number; g: number; b: number }

export interface Raster {
  res: number;
  /** res*res*4 RGBA — `ImageData.data` 에 그대로 넣는다 */
  rgba: Uint8ClampedArray;
  occ: Uint8Array;
  speed: Float32Array;
}

export function newRaster(res: number): Raster {
  return { res, rgba: new Uint8ClampedArray(res * res * 4), occ: new Uint8Array(res * res), speed: new Float32Array(res * res) };
}

/** 거품으로 칠하기 시작하는 속력 / 완전히 밝아지는 속력 (m/s) */
const FOAM_FROM = 1.2;
const FOAM_FULL = 3.5;
const EDGE_LIGHTEN = 0.28;
const FOAM_LIGHTEN = 0.6;

const OCCUPIED = 1;
/** 빈 칸이지만 사방이 거의 물이라 메운 칸 */
const FILLED = 2;

export function rasterize(sim: FlipSim, out: Raster, water: Rgb) {
  const { res, rgba, occ, speed } = out;
  occ.fill(0);
  speed.fill(0);
  const k = res / BOX;
  for (let i = 0; i < sim.count; i++) {
    const x = Math.min(res - 1, Math.max(0, Math.floor(sim.pos[2 * i] * k)));
    const y = Math.min(res - 1, Math.max(0, Math.floor(sim.pos[2 * i + 1] * k)));
    const idx = y * res + x;
    occ[idx] = OCCUPIED;
    const vx = sim.vel[2 * i], vy = sim.vel[2 * i + 1];
    const s2 = vx * vx + vy * vy;
    if (s2 > speed[idx]) speed[idx] = s2;
  }
  // 구멍 메우기 — 입자 간격이 픽셀과 딱 맞지 않아 물 한가운데 점점이 빈 픽셀이 생긴다.
  // 네 이웃 중 셋 이상이 물이면 메운다. 원본(OCCUPIED)만 보고 판단해 번져 나가지 않게 한다.
  for (let y = 1; y < res - 1; y++) {
    for (let x = 1; x < res - 1; x++) {
      const idx = y * res + x;
      if (occ[idx]) continue;
      const n = (occ[idx - 1] === OCCUPIED ? 1 : 0) + (occ[idx + 1] === OCCUPIED ? 1 : 0)
        + (occ[idx - res] === OCCUPIED ? 1 : 0) + (occ[idx + res] === OCCUPIED ? 1 : 0);
      if (n >= 3) occ[idx] = FILLED;
    }
  }
  const lighten = (c: number, t: number) => c + (255 - c) * t;
  for (let y = 0; y < res; y++) {
    for (let x = 0; x < res; x++) {
      const idx = y * res + x;
      const o = idx * 4;
      if (!occ[idx]) { rgba[o + 3] = 0; continue; }
      const edge = x === 0 || y === 0 || x === res - 1 || y === res - 1
        || !occ[idx - 1] || !occ[idx + 1] || !occ[idx - res] || !occ[idx + res];
      const sp = Math.sqrt(speed[idx]);
      const foam = sp <= FOAM_FROM ? 0 : Math.min(1, (sp - FOAM_FROM) / (FOAM_FULL - FOAM_FROM)) * FOAM_LIGHTEN;
      const t = Math.min(1, (edge ? EDGE_LIGHTEN : 0) + foam);
      rgba[o] = lighten(water.r, t);
      rgba[o + 1] = lighten(water.g, t);
      rgba[o + 2] = lighten(water.b, t);
      rgba[o + 3] = 255;
    }
  }
}
