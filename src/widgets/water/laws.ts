/**
 * 법칙 — 액체·통·세상의 규칙을 코드로 바꾸는 자리. 장면(`scene.ts`)이 이것을 조합한다.
 *
 * 현실에 없는 규칙도 된다. 예를 들어 "물 표면에는 항상 고양이 귀가 솟아 있다" 는 `equipotential` 로 —
 * 원하는 표면 모양 y_T(x, z) 를 주면, 자유표면이 그 모양(+ 상수)이 되도록 중력의 수평 성분을 만든다:
 *   a_x = −g·∂y_T/∂x,  a_z = −g·∂y_T/∂z
 * (퍼텐셜 U = −g·(y − y_T) 의 등퍼텐셜면이 y = y_T + c 이므로, 고인 물의 표면이 거기 놓인다.)
 * 물은 여전히 비압축성 유체라 흔들면 출렁이고, 가만두면 다시 귀 모양으로 돌아간다.
 *
 * DOM 에 의존하지 않는다.
 */
import type { Field3 } from "./sim3";
import type { Sdf, ShapeName } from "./shapes";
import type { Physics, Preset } from "./params";
import { BOX } from "./sim";

export interface Laws {
  /** 통 모양 — 이름 또는 SDF 함수 (벽면 기준, 안쪽 음수). 기본 사각 */
  shape?: ShapeName | Sdf;
  /** 입자마다 더해지는 가속 (m/s²) — 비현실 법칙은 대개 여기 */
  field?: Field3;
  /** 처음 물을 둘 자리를 거른다 — 참인 복셀 중심에만 물이 생긴다 */
  seed?: (x: number, y: number, z: number) => boolean;
  /** 액체 — 프리셋 이름이나 직접 조정 값. 사용자의 설정보다 우선한다 */
  fluid?: { preset?: Preset } & Partial<Physics>;
  render?: {
    /** 옆면에 일렁이는 빛 무늬 */
    caustics?: boolean;
  };
}

/** 표면 모양 y_T(x, z) (m, 작을수록 높다) 를 자유표면이 따르게 하는 중력장. `g` 는 장면의 중력 크기. */
export function equipotential(yT: (x: number, z: number) => number, g = 9.8, eps = BOX / 200): Field3 {
  const inv = g / (2 * eps);
  return (x, _y, z, _t, out) => {
    out[0] -= inv * (yT(x + eps, z) - yT(x - eps, z));
    out[2] -= inv * (yT(x, z + eps) - yT(x, z - eps));
  };
}

/** 매끈한 봉우리 — 중심에서 1, 반지름 r 에서 0 (cos²) */
export function bump(dx: number, dz: number, r: number): number {
  const d = Math.hypot(dx, dz) / r;
  if (d >= 1) return 0;
  const c = Math.cos((d * Math.PI) / 2);
  return c * c;
}

/** 고양이 귀 두 개 — 통 가운데 줄(z = BOX/2)에 x = 0.3·BOX, 0.7·BOX. 높이 0.3·BOX, 반지름 0.18·BOX. */
export function catEars(height = 0.3 * BOX, radius = 0.18 * BOX): (x: number, z: number) => number {
  const z0 = BOX / 2;
  return (x, z) => -height * (bump(x - 0.3 * BOX, z - z0, radius) + bump(x - 0.7 * BOX, z - z0, radius));
}

/** 가운데가 솟은 봉우리 하나 */
export function peak(height = 0.35 * BOX, radius = 0.35 * BOX): (x: number, z: number) => number {
  return (x, z) => -height * bump(x - BOX / 2, z - BOX / 2, radius);
}

/** 한쪽으로 기운 표면 — 늘 오른쪽(+x)이 높다 */
export function slope(rise = 0.25 * BOX): (x: number, z: number) => number {
  return (x) => -rise * (x / BOX);
}

/** 가운데가 비어 있는 물 — 중심에서 r 안쪽은 공기로 두는 시드 필터와, 그 구멍을 유지하는 바깥 방향 힘 */
export function hollow(r = 0.3 * BOX, g = 9.8): Pick<Laws, "seed" | "field"> {
  const c = BOX / 2;
  return {
    seed: (x, _y, z) => Math.hypot(x - c, z - c) > r,
    field: (x, _y, z, _t, out) => {
      const dx = x - c, dz = z - c;
      const d = Math.hypot(dx, dz);
      // 구멍 안은 바깥으로 3g, 구멍 둘레 20% 띠에서 0 으로 줄어든다. 약하면 바닥에서 물이 스며든다
      if (d < r * 1.2) {
        const mag = 3 * g * Math.min(1, (r * 1.2 - d) / (r * 0.2));
        const s = d > 1e-6 ? mag / d : 0;
        out[0] += dx * s; out[2] += dz * s;
      }
    },
  };
}
