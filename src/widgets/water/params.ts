import type { Shape, SimParams } from "./sim";

export type Preset = "water" | "honey" | "jelly" | "zerog" | "moon" | "custom";
export type Quality = "low" | "medium" | "high";

/** 사용자가 조정하는 물리 법칙 — 설정 패널의 "직접" 항목과 같은 이름·단위다. */
export interface Physics {
  /** m/s² */
  gravity: number;
  /** 세상 기준 중력 방향(°). 0 = 아래, 90 = 왼쪽(시계 방향으로 돌린 아래) */
  gravityDir: number;
  /** 0~100 */
  viscosity: number;
  /** 0~100 — 튐 */
  splash: number;
  /** 0~100 — 표면장력·응집 */
  cohesion: number;
  /** 0~100 */
  wallFriction: number;
  /** 0~100 — 벽 반발 */
  bounce: number;
  /** % — 흔들림 민감도 */
  inertia: number;
  /** % — 시간 배율 */
  timeScale: number;
}

export interface WaterSettings extends Physics, Record<string, unknown> {
  preset: Preset;
  /** % */
  fill: number;
  /** 화면에서 입자 한 칸의 최소 크기(px) */
  pixelSize: number;
  shape: Shape;
  quality: Quality;
  returnHome: boolean;
  /** 비우면 강조색 */
  color: string;
}

export const PRESETS: Record<Exclude<Preset, "custom">, Physics> = {
  water: { gravity: 9.8, gravityDir: 0, viscosity: 5, splash: 90, cohesion: 10, wallFriction: 10, bounce: 0, inertia: 100, timeScale: 100 },
  honey: { gravity: 9.8, gravityDir: 0, viscosity: 90, splash: 30, cohesion: 30, wallFriction: 70, bounce: 0, inertia: 100, timeScale: 100 },
  jelly: { gravity: 9.8, gravityDir: 0, viscosity: 45, splash: 70, cohesion: 85, wallFriction: 30, bounce: 40, inertia: 100, timeScale: 100 },
  zerog: { gravity: 0, gravityDir: 0, viscosity: 5, splash: 95, cohesion: 50, wallFriction: 5, bounce: 50, inertia: 100, timeScale: 100 },
  moon: { gravity: 1.6, gravityDir: 0, viscosity: 5, splash: 95, cohesion: 10, wallFriction: 5, bounce: 10, inertia: 100, timeScale: 100 },
};

/** 각 값이 받아들이는 범위 — 설정 스키마와 같은 값을 쓴다 (손으로 고친 settings.json 도 버틴다). */
export const LIMITS: Record<keyof Physics, [number, number]> = {
  gravity: [0, 30],
  gravityDir: [-180, 180],
  viscosity: [0, 100],
  splash: [0, 100],
  cohesion: [0, 100],
  wallFriction: [0, 100],
  bounce: [0, 100],
  inertia: [0, 200],
  timeScale: [25, 200],
};

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** 지금 적용할 물리 값. "직접" 이 아니면 프리셋 값을 그대로 쓴다. */
export function physicsOf(s: Partial<WaterSettings>): Physics {
  if (s.preset !== "custom") return PRESETS[(s.preset as Exclude<Preset, "custom">) ?? "water"] ?? PRESETS.water;
  const out = { ...PRESETS.water };
  for (const key of Object.keys(LIMITS) as (keyof Physics)[]) {
    const v = Number(s[key]);
    if (Number.isFinite(v)) out[key] = clamp(v, ...LIMITS[key]);
  }
  return out;
}

const PRESSURE_ITERS: Record<Quality, number> = { low: 20, medium: 40, high: 80 };

/** 사람이 고르는 0~100 값을 솔버 계수로 옮긴다. */
export function simParamsOf(p: Physics, quality: Quality = "medium"): SimParams {
  const visc = clamp(p.viscosity, 0, 100) / 100;
  const splash = clamp(p.splash, 0, 100) / 100;
  return {
    // 점성이 높을수록 PIC 쪽으로 — 둘 다 속도의 잔떨림을 지운다.
    flipRatio: clamp((0.5 + 0.49 * splash) * (1 - 0.6 * visc), 0, 0.99),
    viscosity: visc === 0 ? 0 : 0.95 * Math.pow(visc, 1.5),
    viscosityPasses: 1 + Math.floor(visc * 3),
    cohesion: (clamp(p.cohesion, 0, 100) / 100) * 0.15,
    friction: (clamp(p.wallFriction, 0, 100) / 100) * 0.3,
    restitution: (clamp(p.bounce, 0, 100) / 100) * 0.9,
    // 끈적할수록 더 빨리 잦아든다. 물도 0 은 아니다 — 몇 초 안에 멈춰야 루프가 잠든다.
    drag: 0.4 + 2 * visc,
    pressureIters: PRESSURE_ITERS[quality] ?? PRESSURE_ITERS.medium,
    separationIters: quality === "low" ? 1 : 2,
  };
}

/** 통을 건드리지 않고 이만큼 지나면 잔물결을 가라앉히기 시작한다 (s) */
export const CALM_AFTER = 2;
/** 그 뒤 이만큼에 걸쳐 완전히 가라앉힌다 (s) */
export const CALM_RAMP = 1.5;

/**
 * 잔물결 가라앉히기 — `k`(0~1)만큼 PIC 쪽·강한 감쇠로 옮긴다.
 *
 * FLIP 은 에너지를 거의 잃지 않아 고인 물에도 1초에 몇 픽셀씩 꿈틀대는 잡음이 남는다 (실측:
 * 물 프리셋은 10초 뒤에도 0.05 m/s ≈ 3 px/s, 꿀은 0.003). 그대로면 화면이 늘 꿈틀대고
 * 루프도 잠들지 못한다. 통을 한동안 건드리지 않았을 때만 걸고, 다시 건드리면 즉시 0 으로 돌린다 —
 * 흔드는 동안의 출렁임은 그대로다.
 */
export function calmed(p: SimParams, k: number): SimParams {
  if (k <= 0) return p;
  const t = Math.min(1, k);
  const lerp = (a: number, b: number) => a * (1 - t) + b * t;
  return {
    ...p,
    flipRatio: lerp(p.flipRatio, Math.min(p.flipRatio, 0.3)),
    drag: lerp(p.drag, Math.max(p.drag, 3)),
    // 응집이 아주 세면(젤리) 가득 찬 통에서 압력과 맞서며 끝없이 떤다 (실측). 고인 물에서는
    // 응집이 모양을 거의 바꾸지 않으므로 함께 풀어 준다.
    cohesion: lerp(p.cohesion, p.cohesion * 0.3),
  };
}
