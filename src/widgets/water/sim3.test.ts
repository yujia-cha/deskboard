import { describe, expect, it } from "vitest";
import { BOX, FlipSim3, resolutionFor3, MAX_PARTICLES3, type Forces3 } from "./sim3";
import { calmed, PRESETS, simParams3Of } from "./params";
import { bowlSdf, boxSdf, cylinderSdf, sdfOf, type ShapeName } from "./shapes";
import { newField, splat3, surfaceAt } from "./voxels";

const water = simParams3Of(PRESETS.water);
const still = (g = 9.8): Forces3 => ({ ax: 0, ay: g, az: 0, wx: 0, wy: 0, wz: 0, alx: 0, aly: 0, alz: 0 });

export function run(sim: FlipSim3, seconds: number, forces: (t: number) => Forces3, params = water) {
  for (let k = 0; k < seconds / sim.dt; k++) sim.step(forces(k * sim.dt), params);
}

/** 엔진처럼 손을 뗀 뒤 가라앉히기까지 걸어 재운다 */
export function settle(sim: FlipSim3, seconds: number, params = water, f: Forces3 = still()) {
  for (let k = 0; k < seconds / sim.dt; k++) sim.step(f, calmed(params, (k * sim.dt) / 1.5));
}

function allInside(sim: FlipSim3) {
  for (let i = 0; i < sim.count * 3; i++) {
    const v = sim.pos[i];
    if (!Number.isFinite(v) || v < 0 || v > BOX) return false;
  }
  return true;
}

/** 복셀 수면 높이의 평균과, (ix, iz) 기둥별 높이 */
function surface(sim: FlipSim3) {
  const f = newField(sim.res);
  splat3(sim, f);
  const h: number[] = [];
  for (let ix = 0; ix < sim.res; ix++) for (let iz = 0; iz < sim.res; iz++) h.push(surfaceAt(f, ix, iz));
  const mean = h.reduce((a, b) => a + b, 0) / h.length;
  return { h, mean, at: (ix: number, iz: number) => h[ix * sim.res + iz] };
}

describe("FlipSim3", () => {
  it("물 양만큼 바닥부터 복셀 하나에 입자 하나로 채운다", () => {
    const sim = new FlipSim3({ res: 12, fill: 0.5 });
    expect(sim.count).toBe(12 * 12 * 6);
    expect(sim.centerOfMass().y).toBeGreaterThan(BOX / 2);
    expect(sim.r).toBeCloseTo(sim.voxel / 2);
    expect(sim.r / sim.h).toBeGreaterThan(0.25);
    expect(sim.r / sim.h).toBeLessThan(0.35);
  });

  it("사각 통의 벽 칸은 테두리 한 칸 — 2D 와 같은 규칙", () => {
    const sim = new FlipSim3({ res: 12, fill: 0.5 });
    const { n } = sim;
    for (let i = 0; i < n; i++) for (let j = 0; j < n; j++) for (let k = 0; k < n; k++) {
      const border = i === 0 || j === 0 || k === 0 || i === n - 1 || j === n - 1 || k === n - 1;
      expect(sim.isWall(sim.idx(i, j, k))).toBe(border);
    }
  });

  it("원기둥 벽 칸은 안쪽 원 + 0.3h 밖 (2D 실측 규칙과 같은 식)", () => {
    const sim = new FlipSim3({ res: 16, fill: 0.5, sdf: cylinderSdf });
    const { n, h } = sim;
    const c = BOX / 2;
    const j = Math.floor(n / 2);
    for (let i = 1; i < n - 1; i++) for (let k = 1; k < n - 1; k++) {
      const d = Math.hypot((i - 0.5) * h - c, (k - 0.5) * h - c);
      expect(sim.isWall(sim.idx(i, j, k))).toBe(d > c);
    }
  });

  it.each<ShapeName>(["box", "cylinder", "bowl"])("마구 흔들고 돌려도 입자는 사라지지도 새지도 않는다 (%s)", (shape) => {
    const sdf = sdfOf(shape);
    const sim = new FlipSim3({ res: 14, fill: 0.5, sdf });
    const n = sim.count;
    expect(n).toBeGreaterThan(100);
    run(sim, 4, (t) => ({
      ax: 40 * Math.sin(t * 17), ay: 9.8 + 40 * Math.cos(t * 11), az: 40 * Math.sin(t * 13),
      wx: 10 * Math.sin(t * 3), wy: 0, wz: 10 * Math.cos(t * 2), alx: 200 * Math.cos(t * 5), aly: 0, alz: 0,
    }));
    expect(sim.count).toBe(n);
    expect(allInside(sim)).toBe(true);
    let outside = 0;
    for (let i = 0; i < sim.count; i++) if (sdf(sim.pos[3 * i], sim.pos[3 * i + 1], sim.pos[3 * i + 2]) > 0) outside++;
    expect(outside).toBe(0);
  });

  it.each<ShapeName>(["box", "cylinder", "bowl", "sphere"])("가만히 두면 가라앉아 잠들 수 있을 만큼 멈춘다 (%s)", (shape) => {
    const sim = new FlipSim3({ res: 16, fill: 0.5, sdf: sdfOf(shape) });
    run(sim, 1, (t) => ({ ...still(), ax: t < 0.5 ? 20 : 0 }));
    let sum = 0, cnt = 0;
    for (let k = 0; k < 5 / sim.dt; k++) {
      sim.step(still(), calmed(water, (k * sim.dt) / 1.5));
      if (k * sim.dt >= 4) { sum += sim.motion(); cnt++; }
    }
    // engine.ts 의 SLEEP_MOTION3 (0.04). 경사진 바닥(그릇·구)은 평평한 바닥보다 잡음이 크다 (실측 0.028 vs 0.007)
    expect(sum / cnt).toBeLessThan(0.04);
  });

  it("고인 물의 수면은 평평하고 좌우·앞뒤 대칭이다", () => {
    const sim = new FlipSim3({ res: 16, fill: 0.5 });
    settle(sim, 4);
    const s = surface(sim);
    const q = (f: number) => Math.round(f * (sim.res - 1));
    // 네 귀퉁이 근처와 가운데의 수면 차가 복셀 하나 미만
    const a = s.at(q(0.25), q(0.5)), b = s.at(q(0.75), q(0.5)), c = s.at(q(0.5), q(0.25)), d = s.at(q(0.5), q(0.75));
    expect(Math.abs(a - b)).toBeLessThanOrEqual(1);
    expect(Math.abs(c - d)).toBeLessThanOrEqual(1);
    // 전체 기둥의 90% 가 평균 ±1 복셀 안
    const near = s.h.filter((v) => Math.abs(v - s.mean) <= 1).length / s.h.length;
    expect(near).toBeGreaterThan(0.9);
  });

  it("세게 흔든 뒤에도 부피(수면 높이)가 유지된다", () => {
    const sim = new FlipSim3({ res: 16, fill: 0.5 });
    const before = surface(sim).mean;
    run(sim, 2, (t) => ({ ...still(), ax: 30 * Math.sin(t * 9), az: 30 * Math.cos(t * 7) }));
    settle(sim, 4);
    const after = surface(sim).mean;
    // 수면이 1 복셀 넘게 오르내리면 부피가 새거나 불어난 것
    expect(Math.abs(after - before)).toBeLessThanOrEqual(1);
  });

  it("restDensity 는 속 칸 기준이라 해석값 (h/voxel)³ 근처다", () => {
    const sim = new FlipSim3({ res: 16, fill: 0.6 });
    sim.step(still(), water);
    const expected = Math.pow(sim.h / sim.voxel, 3);
    expect(sim.restDensity).toBeGreaterThan(expected * 0.9);
    expect(sim.restDensity).toBeLessThan(expected * 1.1);
  });

  it("통이 앞으로 기울면(가속이 +x+z) 물이 그쪽으로 쏠린다", () => {
    const sim = new FlipSim3({ res: 14, fill: 0.4 });
    const g = 9.8;
    const tilt: Forces3 = { ...still(), ax: g * 0.5, ay: g * Math.SQRT1_2, az: g * 0.5 };
    run(sim, 2, () => tilt);
    const c = sim.centerOfMass();
    expect(c.x).toBeGreaterThan(BOX / 2 + 0.05);
    expect(c.z).toBeGreaterThan(BOX / 2 + 0.05);
  });

  it("통을 오른쪽(+x)으로 밀면 물은 왼쪽으로 쏠린다", () => {
    const sim = new FlipSim3({ res: 14, fill: 0.4 });
    run(sim, 1.5, () => ({ ...still(), ax: -6 }));
    expect(sim.centerOfMass().x).toBeLessThan(BOX / 2 - 0.05);
  });

  it("무중력에서는 질량중심이 움직이지 않는다", () => {
    const sim = new FlipSim3({ res: 14, fill: 0.5 });
    const before = sim.centerOfMass();
    run(sim, 3, () => still(0), simParams3Of(PRESETS.zerog));
    const after = sim.centerOfMass();
    expect(Math.hypot(after.x - before.x, after.y - before.y, after.z - before.z)).toBeLessThan(0.05 * BOX);
  });

  it("법칙 훅의 가속이 입자에 더해진다", () => {
    const sim = new FlipSim3({ res: 12, fill: 0.4 });
    sim.field = (_x, _y, _z, _t, out) => { out[0] += 6; };
    run(sim, 1.5, () => still());
    expect(sim.centerOfMass().x).toBeGreaterThan(BOX / 2 + 0.05);
  });

  it("resolutionFor3 는 예산을 넘지 않고 최소값을 지킨다", () => {
    for (const fill of [0.1, 0.45, 0.8]) {
      const res = resolutionFor3(1000, 1, fill);
      expect(res ** 3 * fill).toBeLessThanOrEqual(MAX_PARTICLES3 * 1.01);
    }
    expect(resolutionFor3(10, 4, 0.5)).toBe(8);
    expect(boxSdf(BOX / 2, BOX / 2, BOX / 2)).toBeLessThan(0);
    expect(bowlSdf(BOX / 2, BOX * 0.9, BOX / 2)).toBeLessThan(0);
    expect(bowlSdf(BOX / 2, BOX * 0.2, BOX / 2)).toBeGreaterThan(0);
  });
});
