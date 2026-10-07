import { describe, expect, it } from "vitest";
import { BOX, FlipSim, MAX_PARTICLES, resolutionFor, type Forces, type Shape } from "./sim";
import { calmed, PRESETS, simParamsOf } from "./params";
import { newRaster, rasterize } from "./render";

const DT = 1 / 120;
const water = simParamsOf(PRESETS.water);
const still = (g = 9.8): Forces => ({ ax: 0, ay: g, omega: 0, alpha: 0 });

function run(sim: FlipSim, seconds: number, forces: (t: number) => Forces, params = water) {
  for (let k = 0; k < seconds / DT; k++) sim.step(DT, forces(k * DT), params);
}

function allInside(sim: FlipSim) {
  for (let i = 0; i < sim.count * 2; i++) {
    const v = sim.pos[i];
    if (!Number.isFinite(v) || v < 0 || v > BOX) return false;
  }
  return true;
}

describe("FlipSim", () => {
  it("물 양만큼 바닥부터 채운다", () => {
    const half = new FlipSim({ res: 40, fill: 0.5, shape: "square" });
    const full = new FlipSim({ res: 40, fill: 1, shape: "square" });
    expect(half.count).toBeGreaterThan(0);
    expect(half.count / full.count).toBeCloseTo(0.5, 1);
    expect(half.centerOfMass().y).toBeGreaterThan(BOX / 2); // y 는 아래로 커진다
  });

  it.each<Shape>(["square", "round"])("마구 흔들고 돌려도 입자는 사라지지도 새지도 않는다 (%s)", (shape) => {
    const sim = new FlipSim({ res: 40, fill: 0.5, shape });
    const n = sim.count;
    run(sim, 6, (t) => ({
      ax: 60 * Math.sin(t * 17), ay: 9.8 + 60 * Math.cos(t * 11),
      omega: 20 * Math.sin(t * 3), alpha: 300 * Math.cos(t * 5),
    }));
    expect(sim.count).toBe(n);
    expect(allInside(sim)).toBe(true);
    if (shape === "round") {
      for (let i = 0; i < sim.count; i++) {
        expect(Math.hypot(sim.pos[2 * i] - BOX / 2, sim.pos[2 * i + 1] - BOX / 2)).toBeLessThan(BOX / 2);
      }
    }
  });

  it.each<Shape>(["square", "round"])("가만히 두면 가라앉아 잠들 수 있을 만큼 멈춘다 (%s)", (shape) => {
    const sim = new FlipSim({ res: 40, fill: 0.5, shape });
    run(sim, 1, (t) => ({ ...still(), ax: t < 0.5 ? 30 : 0 }));
    // 엔진처럼: 손을 뗀 뒤 잔물결 가라앉히기가 걸린다. 마지막 1초의 평균 움직임을 본다.
    let sum = 0, n = 0;
    for (let k = 0; k < 5 / DT; k++) {
      sim.step(DT, still(), calmed(water, (k * DT) / 1.5));
      if (k * DT >= 4) { sum += sim.motion(); n++; }
    }
    expect(sum / n).toBeLessThan(0.03); // engine.ts 의 SLEEP_MOTION
  });

  it("고인 물이 들고 있는 g·dt 속도는 움직임으로 치지 않는다", () => {
    // 저장된 속도는 매 스텝 중력만큼 남아 있지만(벽과 압력이 다음 스텝에 지운다) 실제 변위는 거의 없다.
    const sim = new FlipSim({ res: 40, fill: 0.5, shape: "square" });
    for (let k = 0; k < 4 / DT; k++) sim.step(DT, still(), calmed(water, 1));
    expect(Math.sqrt(sim.meanSpeed2())).toBeGreaterThan(0.05);
    expect(sim.motion()).toBeLessThan(0.025);
  });

  it("통이 90° 돌면 물은 새 아래쪽 벽으로 쏠린다", () => {
    // 통이 시계 방향으로 90° 돌면 세상의 아래(+y)는 통 좌표계의 +x 다 (container.test 참고).
    const sim = new FlipSim({ res: 40, fill: 0.4, shape: "square" });
    run(sim, 3, () => ({ ax: 9.8, ay: 0, omega: 0, alpha: 0 }));
    expect(sim.centerOfMass().x).toBeGreaterThan(BOX * 0.65);
  });

  it("통을 오른쪽으로 밀면 물은 왼쪽으로 쏠린다", () => {
    const sim = new FlipSim({ res: 40, fill: 0.4, shape: "square" });
    run(sim, 0.4, () => ({ ax: -20, ay: 9.8, omega: 0, alpha: 0 })); // a = g − A_box
    expect(sim.centerOfMass().x).toBeLessThan(BOX / 2 - 0.05);
  });

  it.each(["zerog", "water"] as const)("세게 흔든 뒤에도 물이 흩어져 불어나지 않는다 (%s)", (preset) => {
    // 밀도 보정이 밀어내기만 하면 흩어진 물이 그대로 남아 통을 다 덮는다 (실측: 무중력 45% → 70%).
    const area = (s: FlipSim) => {
      const r = newRaster(s.res);
      rasterize(s, r, { r: 0, g: 0, b: 0 });
      let n = 0;
      for (let i = 0; i < r.occ.length; i++) if (r.occ[i]) n++;
      return n;
    };
    const sim = new FlipSim({ res: 40, fill: 0.45, shape: "square" });
    const before = area(sim);
    const p = simParamsOf(PRESETS[preset]);
    const g = PRESETS[preset].gravity;
    run(sim, 1.2, (t) => ({
      ax: 60 * Math.sign(Math.sin(t * 25)), ay: g + 30 * Math.cos(t * 17),
      omega: 6 * Math.sin(t * 7), alpha: 200 * Math.cos(t * 7),
    }), p);
    run(sim, 4, () => still(g), p);
    expect(area(sim) / before).toBeLessThan(1.1);
  });

  it("벽에 물 막이 들러붙지 않는다 — 중력을 옆으로 돌리면 반대쪽 벽은 비어 있다", () => {
    const sim = new FlipSim({ res: 40, fill: 0.45, shape: "square" });
    run(sim, 1, (t) => ({ ax: 40 * Math.sin(t * 20), ay: 9.8, omega: 0, alpha: 0 }));
    run(sim, 4, () => ({ ax: -9.8, ay: 0, omega: 0, alpha: 0 }));
    const r = newRaster(sim.res);
    rasterize(sim, r, { r: 0, g: 0, b: 0 });
    let all = 0, far = 0;
    for (let y = 0; y < r.res; y++) {
      for (let x = 0; x < r.res; x++) {
        if (!r.occ[y * r.res + x]) continue;
        all++;
        if (x > r.res * 0.6) far++;
      }
    }
    expect(far / all).toBeLessThan(0.01);
  });

  it("무중력에서는 질량중심이 제자리에 있다", () => {
    const sim = new FlipSim({ res: 40, fill: 0.4, shape: "square" });
    const before = sim.centerOfMass();
    run(sim, 2, () => still(0), simParamsOf(PRESETS.zerog));
    const after = sim.centerOfMass();
    expect(Math.abs(after.x - before.x)).toBeLessThan(0.05);
    expect(Math.abs(after.y - before.y)).toBeLessThan(0.05);
  });

  it("점성이 높을수록 같은 흔들기 뒤 더 빨리 잦아든다", () => {
    const after = (preset: keyof typeof PRESETS) => {
      const sim = new FlipSim({ res: 40, fill: 0.5, shape: "square" });
      const p = simParamsOf(PRESETS[preset]);
      run(sim, 0.5, (t) => ({ ...still(), ax: 40 * Math.sin(t * 20) }), p);
      run(sim, 1, () => still(), p);
      return sim.motion();
    };
    expect(after("honey")).toBeLessThan(after("water"));
  });
});

describe("resolutionFor", () => {
  it("픽셀 크기를 지키되 입자 수 상한을 넘지 않는다", () => {
    expect(resolutionFor(120, 2, 0.45, "square")).toBe(60);
    const big = resolutionFor(2000, 1, 0.8, "square");
    const capacity = big * big * (2 / Math.sqrt(3)) * 0.8;
    expect(capacity).toBeLessThanOrEqual(MAX_PARTICLES * 1.05);
    expect(resolutionFor(10, 4, 0.5, "round")).toBe(16); // 아무리 작아도 최소 해상도
  });
});
