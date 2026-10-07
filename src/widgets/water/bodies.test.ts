import { describe, expect, it } from "vitest";
import { BOX, FlipSim3, type Forces3 } from "./sim3";
import { PRESETS, simParams3Of } from "./params";
import { buildBodies, excludeParticles, markSolid, paintBodies, particlesInside, stepBodies, type BodySpec, type BodySet } from "./bodies";
import { newField, splat3, surfaceAt, BODY0 } from "./voxels";
import { boxSdf } from "./shapes";

const water = simParams3Of(PRESETS.water);
const still: Forces3 = { ax: 0, ay: 9.8, az: 0, wx: 0, wy: 0, wz: 0, alx: 0, aly: 0, alz: 0 };

const cube = (density: number, at: [number, number, number] = [0.5, 0.3, 0.5]): BodySpec => ({
  name: "cube",
  layers: [["##", "##"], ["##", "##"]],
  palette: { "#": "var(--text)" },
  density,
  at,
});

/** 엔진의 한 스텝과 같은 순서 */
function step(sim: FlipSim3, set: BodySet, f = still) {
  const field = newField(sim.res);
  markSolid(sim, set);
  sim.step(f, water);
  splat3(sim, field);
  stepBodies(set, sim, field, f, sim.dt, boxSdf);
  excludeParticles(sim, set);
  return field;
}

describe("bodies", () => {
  it("ASCII 층을 복셀로 — 층 배열은 아래부터, 복셀 y 는 아래로", () => {
    const set = buildBodies([{
      name: "t", layers: [["#.", ".."], ["##", "##"]], palette: { "#": "var(--text)" }, density: 1,
    }], 0.1);
    const b = set.bodies[0];
    expect(b.local.length / 3).toBe(5);
    expect(b.size).toEqual([2, 2, 2]);
    // 첫 층(아래)의 (0,0) 은 iy = 1
    expect(Array.from(b.local.slice(0, 3))).toEqual([0, 1, 0]);
    expect(set.matColors).toEqual(["var(--text)"]);
    expect(set.matBody).toEqual([0]);
  });

  it("가벼운 물체는 밀도만큼 잠긴 채 떠서 멈춘다", () => {
    const sim = new FlipSim3({ res: 14, fill: 0.5 });
    const set = buildBodies([cube(0.5, [0.5, 0.25, 0.5])], sim.voxel);
    const b = set.bodies[0];
    for (let k = 0; k < 4 / sim.dt; k++) step(sim, set);
    expect(b.submerged).toBeGreaterThan(0.3);
    expect(b.submerged).toBeLessThan(0.75);
    expect(Math.hypot(...b.vel)).toBeLessThan(0.05);
    // 수면 근처에 있다 — 통 바닥도 천장도 아니다
    expect(b.pos[1]).toBeGreaterThan(BOX * 0.3);
    expect(b.pos[1]).toBeLessThan(BOX * 0.7);
  });

  it("무거운 물체는 바닥까지 가라앉는다", () => {
    const sim = new FlipSim3({ res: 14, fill: 0.5 });
    const set = buildBodies([cube(2.5, [0.5, 0.3, 0.5])], sim.voxel);
    const b = set.bodies[0];
    for (let k = 0; k < 4 / sim.dt; k++) step(sim, set);
    expect(b.pos[1]).toBeGreaterThan(BOX - 2 * sim.voxel);
    expect(b.submerged).toBeGreaterThan(0.9);
  });

  it("바디 안에는 입자가 없고 입자 수는 그대로다", () => {
    const sim = new FlipSim3({ res: 14, fill: 0.5 });
    const n = sim.count;
    const set = buildBodies([cube(0.5, [0.5, 0.6, 0.5])], sim.voxel); // 물속에서 시작
    for (let k = 0; k < 2 / sim.dt; k++) {
      step(sim, set);
      expect(particlesInside(sim, set.bodies[0])).toBe(0);
    }
    expect(sim.count).toBe(n);
  });

  it("물속에 밀어 넣으면 수면이 그만큼 오른다", () => {
    const sim = new FlipSim3({ res: 16, fill: 0.5 });
    const field0 = newField(sim.res);
    splat3(sim, field0);
    const mean = (f: ReturnType<typeof newField>) => {
      let s = 0;
      for (let ix = 0; ix < sim.res; ix++) for (let iz = 0; iz < sim.res; iz++) s += surfaceAt(f, ix, iz);
      return s / (sim.res * sim.res);
    };
    const count = (f: ReturnType<typeof newField>) => { let n = 0; for (let i = 0; i < f.mat.length; i++) if (f.mat[i] === 1) n++; return n; };
    const before = mean(field0);
    const voxBefore = count(field0);
    // 4×4×4 복셀 덩어리를 물 위에서 집어 1.5초에 걸쳐 물속 깊이(수면 아래 ~3 복셀) 넣고 붙든다
    const big: BodySpec = { name: "big", layers: Array(4).fill(["####", "####", "####", "####"]), palette: { "#": "var(--text)" }, density: 0.3, at: [0.5, 0.3, 0.5] };
    const set = buildBodies([big], sim.voxel);
    let field = field0;
    for (let k = 0; k < 4 / sim.dt; k++) {
      const t = Math.min(1, (k * sim.dt) / 1.5);
      set.bodies[0].grab = [BOX / 2, BOX * (0.3 + 0.5 * t), BOX / 2];
      field = step(sim, set);
    }
    const after = mean(field);
    // 잠긴 부피 64 복셀 / 바닥 256 기둥 = 0.25 복셀 — 수면 평균(복셀 단위, 위로)이 크게 내려가지 않고 1 복셀 안에서 오른다.
    // 바디 바로 위의 얇은 물층은 복셀로 다 잡히지 않아 기둥 몇 개가 비어 보일 수 있다 — 그래서 아래쪽 여유를 둔다.
    expect(before - after).toBeGreaterThan(-0.35);
    expect(before - after).toBeLessThan(1);
    // 물 복셀 수가 유지된다 — 바디가 물을 뭉개거나(압축) 새게 하지 않는다
    expect(Math.abs(count(field) - voxBefore) / voxBefore).toBeLessThan(0.08);
    expect(particlesInside(sim, set.bodies[0])).toBe(0);
  });

  it("무중력에서는 가만히 있다", () => {
    const sim = new FlipSim3({ res: 12, fill: 0.5 });
    const set = buildBodies([cube(0.5, [0.5, 0.2, 0.5])], sim.voxel);
    const zero: Forces3 = { ...still, ay: 0 };
    const p0 = [...set.bodies[0].pos];
    for (let k = 0; k < 1 / sim.dt; k++) step(sim, set, zero);
    expect(Math.hypot(set.bodies[0].pos[0] - p0[0], set.bodies[0].pos[1] - p0[1], set.bodies[0].pos[2] - p0[2])).toBeLessThan(0.02);
  });

  it("복셀 장에 바디가 물 위에 그려진다", () => {
    const sim = new FlipSim3({ res: 12, fill: 0.5 });
    const set = buildBodies([cube(0.5, [0.5, 0.5, 0.5])], sim.voxel);
    const f = newField(sim.res);
    splat3(sim, f);
    paintBodies(set, f);
    let n = 0;
    for (let i = 0; i < f.mat.length; i++) if (f.mat[i] === BODY0) n++;
    expect(n).toBe(8);
  });
});
