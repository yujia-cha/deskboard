/**
 * 통(용기)의 자세와 움직임 — 위젯 안에서 통을 잡고 흔들고 돌리는 부분.
 *
 * 통은 포인터를 **스프링-댐퍼로 따라간다.** 포인터 좌표를 바로 쓰면 그 2차 차분(가속도)이
 * 손떨림 잡음 그대로라 물이 폭주한다. 스프링이 걸러 낸 가속도는 우리가 적분한 값이라 정확하다 —
 * 그것을 그대로 물에 관성력으로 넘긴다 (`inertialForces`).
 *
 * 좌표는 캔버스의 레이아웃 px 다. 각도는 캔버스 `rotate()` 와 같은 방향(+x 에서 +y 쪽, 화면에서 시계 방향).
 * DOM 에 의존하지 않는다.
 */
import { BOX, type Forces, type Shape } from "./sim";

export interface BoxState {
  x: number;
  y: number;
  vx: number;
  vy: number;
  /** 각도 (rad). 한 바퀴를 넘어도 그대로 누적한다 — 되감기 없이 "가장 가까운 원위치" 로 돌아간다. */
  th: number;
  /** 각속도 (rad/s) */
  w: number;
  /** 저역 통과한 병진 가속 (px/s²) */
  ax: number;
  ay: number;
  /** 저역 통과한 각가속 (rad/s²) */
  alpha: number;
}

/** 통이 끌려가는 목표. null 이면 그 축은 감쇠만 받고 미끄러지다 멈춘다. */
export interface BoxTargets {
  pos: { x: number; y: number } | null;
  ang: number | null;
}

export interface Bounds { minX: number; maxX: number; minY: number; maxY: number }

const K_POS = 900;
const C_POS = 2 * Math.sqrt(K_POS) * 0.75;
const K_ANG = 500;
const C_ANG = 2 * Math.sqrt(K_ANG) * 0.8;
/** 목표가 없을 때의 감쇠 (1/s) — 던진 통이 미끄러지다 멈춘다 */
const COAST = 4;
const COAST_ANG = 1.5;
/** 카드 가장자리에 부딪혔을 때 남는 속도 비율 */
const WALL_BOUNCE = 0.35;
/** 가속도 저역 통과 계수 (스텝당) */
const SMOOTH = 0.35;

export function newBox(x: number, y: number): BoxState {
  return { x, y, vx: 0, vy: 0, th: 0, w: 0, ax: 0, ay: 0, alpha: 0 };
}

/** 지금 각도에서 가장 가까운 "똑바로 선" 각도 — 세 바퀴 돌렸다고 세 바퀴 되감지 않는다. */
export function homeAngle(th: number): number {
  return Math.round(th / (2 * Math.PI)) * 2 * Math.PI;
}

/** b → a 로 가는 가장 짧은 각 변화 (−π, π] — atan2 가 ±π 에서 튀는 것을 펴서 회전을 누적한다. */
export function angleDelta(a: number, b: number): number {
  let d = (a - b) % (2 * Math.PI);
  if (d > Math.PI) d -= 2 * Math.PI;
  if (d <= -Math.PI) d += 2 * Math.PI;
  return d;
}

/** 통 중심이 머물 수 있는 범위 — 통의 면(내접원)이 카드 안에 있도록. 모서리는 돌리면 조금 잘릴 수 있다. */
export function boundsFor(w: number, h: number, boxPx: number): Bounds {
  const e = boxPx / 2;
  const span = (lo: number, hi: number) => (lo <= hi ? [lo, hi] : [(lo + hi) / 2, (lo + hi) / 2]);
  const [minX, maxX] = span(e, w - e);
  const [minY, maxY] = span(e, h - e);
  return { minX, maxX, minY, maxY };
}

/**
 * 잡은 자리로 할 일을 고른다. `lx, ly` 는 통 중심 기준·통 좌표계(회전을 뺀) 위치다.
 * 통 안쪽 = 이동(흔들기), 가장자리 띠와 통 바깥 = 회전.
 */
export function grabMode(lx: number, ly: number, boxPx: number, shape: Shape): "move" | "rotate" {
  const half = boxPx / 2;
  const core = half * 0.78;
  const inside = shape === "round" ? Math.hypot(lx, ly) <= core : Math.abs(lx) <= core && Math.abs(ly) <= core;
  return inside ? "move" : "rotate";
}

/** 한 스텝 적분 (반암시적 오일러). 가속도는 실제로 바뀐 속도에서 구한다 — 벽에 부딪힌 충격까지 들어간다. */
export function stepBox(s: BoxState, t: BoxTargets, dt: number, b: Bounds) {
  const vx0 = s.vx, vy0 = s.vy, w0 = s.w;
  const ax = t.pos ? K_POS * (t.pos.x - s.x) - C_POS * s.vx : -COAST * s.vx;
  const ay = t.pos ? K_POS * (t.pos.y - s.y) - C_POS * s.vy : -COAST * s.vy;
  const al = t.ang !== null ? K_ANG * (t.ang - s.th) - C_ANG * s.w : -COAST_ANG * s.w;
  s.vx += ax * dt;
  s.vy += ay * dt;
  s.w += al * dt;
  s.x += s.vx * dt;
  s.y += s.vy * dt;
  s.th += s.w * dt;
  // 카드 가장자리에 부딪히면 튕긴다. 이 순간의 큰 가속이 그대로 "쾅" 하는 물보라가 된다.
  if (s.x < b.minX) { s.x = b.minX; if (s.vx < 0) s.vx = -s.vx * WALL_BOUNCE; }
  if (s.x > b.maxX) { s.x = b.maxX; if (s.vx > 0) s.vx = -s.vx * WALL_BOUNCE; }
  if (s.y < b.minY) { s.y = b.minY; if (s.vy < 0) s.vy = -s.vy * WALL_BOUNCE; }
  if (s.y > b.maxY) { s.y = b.maxY; if (s.vy > 0) s.vy = -s.vy * WALL_BOUNCE; }
  s.ax += SMOOTH * ((s.vx - vx0) / dt - s.ax);
  s.ay += SMOOTH * ((s.vy - vy0) / dt - s.ay);
  s.alpha += SMOOTH * ((s.w - w0) / dt - s.alpha);
}

/** 통이 멈췄고 목표에 닿아 있는가 — 잠들기 판정 */
export function boxSettled(s: BoxState, t: BoxTargets): boolean {
  const still = Math.hypot(s.vx, s.vy) < 1 && Math.abs(s.w) < 0.01;
  const atPos = !t.pos || Math.hypot(t.pos.x - s.x, t.pos.y - s.y) < 0.5;
  const atAng = t.ang === null || Math.abs(t.ang - s.th) < 0.002;
  return still && atPos && atAng;
}

/** 관성력 상한 — 포인터 한 번 튄 것으로 물이 통을 뚫고 나가지 않게 */
const A_MAX = 60; // m/s²
const W_MAX = 25; // rad/s
const ALPHA_MAX = 300; // rad/s²

const clampAbs = (v: number, m: number) => Math.max(-m, Math.min(m, v));

/**
 * 통 좌표계에서 물이 느끼는 힘.
 *
 *   a = Rᵀ(g − A_box)  (중력 − 통의 병진 가속)
 *   + ω²r − α×r − 2ω×v (원심력·오일러력·코리올리 — 입자마다 `FlipSim.integrate` 가 더한다)
 *
 * `inertia` 는 통의 움직임에서 오는 항에만 곱한다. 0 이면 흔들어도 물이 모르고, 돌리면 중력
 * 방향만 바뀐다.
 */
export function inertialForces(
  s: BoxState,
  o: { gravity: number; gravityDirDeg: number; inertia: number; boxPx: number },
): Forces {
  const mpp = BOX / Math.max(1, o.boxPx); // m per px
  const dir = (o.gravityDirDeg * Math.PI) / 180;
  const gx = -Math.sin(dir) * o.gravity;
  const gy = Math.cos(dir) * o.gravity;
  let Ax = s.ax * mpp * o.inertia;
  let Ay = s.ay * mpp * o.inertia;
  const mag = Math.hypot(Ax, Ay);
  if (mag > A_MAX) { Ax *= A_MAX / mag; Ay *= A_MAX / mag; }
  const wx = gx - Ax, wy = gy - Ay;
  const c = Math.cos(s.th), sn = Math.sin(s.th);
  return {
    ax: c * wx + sn * wy,
    ay: -sn * wx + c * wy,
    omega: clampAbs(s.w * o.inertia, W_MAX),
    alpha: clampAbs(s.alpha * o.inertia, ALPHA_MAX),
  };
}
