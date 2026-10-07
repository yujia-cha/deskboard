import { describe, expect, it } from "vitest";
import { BOX, FlipSim3 } from "./sim3";
import { catEars, equipotential, slope } from "./laws";
import { sceneOf, SCENES } from "./scene";
import { calmed, PRESETS, simParams3Of } from "./params";
import { newField, splat3, surfaceAt } from "./voxels";

const water = simParams3Of(PRESETS.water);
const g = 9.8;
const still = { ax: 0, ay: g, az: 0, wx: 0, wy: 0, wz: 0, alx: 0, aly: 0, alz: 0 };

function settleWith(field: ReturnType<typeof equipotential>, seconds: number, res = 16, fill = 0.45) {
  const sim = new FlipSim3({ res, fill });
  sim.field = field;
  for (let k = 0; k < seconds / sim.dt; k++) sim.step(still, calmed(water, (k * sim.dt - 1) / 1.5));
  const f = newField(res);
  splat3(sim, f);
  return { sim, surface: (ix: number, iz: number) => surfaceAt(f, ix, iz) };
}

describe("laws", () => {
  it("등퍼텐셜 장은 표면이 높은 쪽으로 미는 수평 가속이다", () => {
    const field = equipotential(catEars(), g);
    const out = new Float32Array(3);
    // 왼쪽 귀(x = 0.3 BOX) 바로 왼쪽: +x 로 밀어야 한다
    field(0.3 * BOX - 0.1, BOX / 2, BOX / 2, 0, out);
    expect(out[0]).toBeGreaterThan(0);
    expect(Math.abs(out[1])).toBe(0);
    out.fill(0);
    field(0.3 * BOX + 0.1, BOX / 2, BOX / 2, 0, out);
    expect(out[0]).toBeLessThan(0);
  });

  it("'한쪽이 높은 물' 법칙 — 오른쪽(+x) 수면이 왼쪽보다 높다", () => {
    const { sim, surface } = settleWith(equipotential(slope(), g), 4);
    const z = Math.floor(sim.res / 2);
    const left = surface(1, z), right = surface(sim.res - 2, z);
    // surfaceAt 은 복셀 번호(작을수록 높다). 0.25·BOX 기울기면 res 16 에서 약 4 복셀 차이
    expect(left - right).toBeGreaterThanOrEqual(2);
  });

  it("고양이 귀 — 귀 자리의 수면이 사이보다 높다", () => {
    const { sim, surface } = settleWith(equipotential(catEars(), g), 4);
    const z = Math.floor(sim.res / 2);
    const ear = (fx: number) => surface(Math.round(fx * (sim.res - 1)), z);
    const earL = ear(0.3), earR = ear(0.7), mid = ear(0.5), edge = surface(0, z);
    expect(mid - earL).toBeGreaterThanOrEqual(2);
    expect(mid - earR).toBeGreaterThanOrEqual(2);
    expect(edge - earL).toBeGreaterThanOrEqual(2);
  });

  it("장면 — 모르는 이름은 기본 장면, 모든 장면에 이름표가 있다", () => {
    expect(sceneOf("nope")).toBe(SCENES.default);
    expect(sceneOf(undefined)).toBe(SCENES.default);
    for (const s of Object.values(SCENES)) {
      expect(s.label.length).toBeGreaterThan(0);
      for (const b of s.bodies) expect(b.layers.length).toBeGreaterThan(0);
    }
  });

  it("'구멍 뚫린 물' 은 가운데 기둥에 물이 없다", () => {
    const scene = SCENES.hollow;
    const sim = new FlipSim3({ res: 16, fill: 0.5, seed: scene.seed });
    sim.field = scene.field ?? null;
    for (let k = 0; k < 3 / sim.dt; k++) sim.step(still, water);
    const f = newField(16);
    splat3(sim, f);
    expect(surfaceAt(f, 8, 8)).toBeGreaterThanOrEqual(15); // 바닥에 한 겹쯤은 스며들 수 있다
    expect(surfaceAt(f, 1, 1)).toBeLessThan(16);
  });
});
