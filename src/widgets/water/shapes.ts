/**
 * 통의 모양 — 부호 거리 함수(SDF). 벽면에서 0, 안쪽이 음수, 바깥이 양수 (m).
 *
 * 유체 영역은 [0, BOX]³ 이고 통은 그 안에 들어 있다. 사각 통은 영역 전체, 원기둥은 세워 둔 통
 * (윗면이 원), 그릇은 구의 아래쪽 절반이다. 장면(`scene.ts`)에서 직접 함수를 써도 된다 —
 * 격자 벽 칸은 "칸 중심의 sdf > 0", 입자 제약은 "sdf ≤ −r" 로 어떤 모양이든 같은 규칙이다.
 * DOM 에 의존하지 않는다.
 */
import { BOX } from "./sim";

export type Sdf = (x: number, y: number, z: number) => number;

const C = BOX / 2;

/** 사각 통 — 영역 전체 */
export const boxSdf: Sdf = (x, y, z) => Math.max(Math.abs(x - C), Math.abs(y - C), Math.abs(z - C)) - C;

/** 세운 원기둥 — 윗면이 원, 영역에 내접 */
export const cylinderSdf: Sdf = (x, y, z) => Math.max(Math.hypot(x - C, z - C) - C, Math.abs(y - C) - C);

/** 구 — 영역에 내접 */
export const sphereSdf: Sdf = (x, y, z) => Math.hypot(x - C, y - C, z - C) - C;

/** 그릇 — 구의 아래쪽 절반 (y 는 아래로 커진다) */
export const bowlSdf: Sdf = (x, y, z) => Math.max(sphereSdf(x, y, z), C - y);

export type ShapeName = "box" | "cylinder" | "bowl" | "sphere";
export const SHAPES: Record<ShapeName, Sdf> = { box: boxSdf, cylinder: cylinderSdf, bowl: bowlSdf, sphere: sphereSdf };

export function sdfOf(shape: ShapeName | Sdf | undefined): Sdf {
  if (typeof shape === "function") return shape;
  return SHAPES[shape ?? "box"] ?? boxSdf;
}

/**
 * 벽면의 바깥쪽 법선(단위 벡터)을 유한차분으로 구해 `out` 에 쓴다.
 * `eps` 는 차분 간격 — 입자 반지름의 절반쯤이 적당하다.
 */
export function projectOut(sdf: Sdf, x: number, y: number, z: number, eps: number, out: Float32Array) {
  let nx = sdf(x + eps, y, z) - sdf(x - eps, y, z);
  let ny = sdf(x, y + eps, z) - sdf(x, y - eps, z);
  let nz = sdf(x, y, z + eps) - sdf(x, y, z - eps);
  const len = Math.hypot(nx, ny, nz);
  if (len > 0) { nx /= len; ny /= len; nz /= len; } else { nx = 0; ny = -1; nz = 0; }
  out[0] = nx; out[1] = ny; out[2] = nz;
}
