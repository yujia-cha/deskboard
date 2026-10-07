import { describe, expect, it } from "vitest";
import { calmed, LIMITS, physicsOf, PRESETS, simParamsOf } from "./params";

describe("물리 값", () => {
  it("프리셋을 고르면 직접 조정 값은 무시한다", () => {
    expect(physicsOf({ preset: "honey", gravity: 30 })).toEqual(PRESETS.honey);
  });

  it("직접 조정은 범위 안으로 자르고, 빠진 값은 물로 채운다", () => {
    const p = physicsOf({ preset: "custom", gravity: 99, viscosity: -5 });
    expect(p.gravity).toBe(LIMITS.gravity[1]);
    expect(p.viscosity).toBe(0);
    expect(p.splash).toBe(PRESETS.water.splash);
  });

  it("모르는 프리셋은 물로", () => {
    expect(physicsOf({ preset: "lava" as never })).toEqual(PRESETS.water);
  });

  it("모든 프리셋이 범위 안에 있다", () => {
    for (const p of Object.values(PRESETS)) {
      for (const [k, [lo, hi]] of Object.entries(LIMITS)) {
        const v = p[k as keyof typeof p];
        expect(v).toBeGreaterThanOrEqual(lo);
        expect(v).toBeLessThanOrEqual(hi);
      }
    }
  });

  it("점성이 높을수록 FLIP 이 줄고 감쇠가 는다", () => {
    const thin = simParamsOf(PRESETS.water), thick = simParamsOf(PRESETS.honey);
    expect(thick.flipRatio).toBeLessThan(thin.flipRatio);
    expect(thick.viscosity).toBeGreaterThan(thin.viscosity);
    expect(thick.drag).toBeGreaterThan(thin.drag);
    expect(thin.flipRatio).toBeLessThanOrEqual(0.99);
  });

  it("품질은 압력 반복 수를 바꾼다", () => {
    expect(simParamsOf(PRESETS.water, "low").pressureIters).toBeLessThan(simParamsOf(PRESETS.water, "high").pressureIters);
  });

  it("잔물결 가라앉히기는 0 이면 그대로, 1 이면 끈적한 쪽으로", () => {
    const p = simParamsOf(PRESETS.water);
    expect(calmed(p, 0)).toBe(p);
    expect(calmed(p, -3)).toBe(p);
    const c = calmed(p, 1);
    expect(c.flipRatio).toBeLessThanOrEqual(0.3);
    expect(c.drag).toBeGreaterThanOrEqual(3);
    expect(c.tension).toBe(p.tension); // 부피를 지키는 몫이라 가라앉힐 때도 그대로
    expect(calmed(p, 5)).toEqual(c); // 1 에서 멈춘다
  });
});
