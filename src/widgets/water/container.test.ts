import { describe, expect, it } from "vitest";
import { angleDelta, boundsFor, boxSettled, grabMode, homeAngle, inertialForces, newBox, stepBox, type BoxTargets } from "./container";

const DT = 1 / 120;
const wide = { minX: -1e6, maxX: 1e6, minY: -1e6, maxY: 1e6 };
const opts = { gravity: 9.8, gravityDirDeg: 0, inertia: 1, boxPx: 150 };

describe("통 움직임", () => {
  it("목표를 스프링으로 따라가 멈춘다", () => {
    const b = newBox(0, 0);
    const t: BoxTargets = { pos: { x: 50, y: 0 }, ang: 0 };
    for (let k = 0; k < 240; k++) stepBox(b, t, DT, wide);
    expect(b.x).toBeCloseTo(50, 0);
    expect(boxSettled(b, t)).toBe(true);
  });

  it("목표가 없으면 미끄러지다 멈춘다", () => {
    const b = newBox(0, 0);
    b.vx = 300;
    const t: BoxTargets = { pos: null, ang: 0 };
    for (let k = 0; k < 120 * 4; k++) stepBox(b, t, DT, wide);
    expect(Math.abs(b.vx)).toBeLessThan(1);
    expect(b.x).toBeGreaterThan(30);
  });

  it("카드 가장자리에 부딪히면 튕기고, 그 충격이 큰 가속으로 남는다", () => {
    const b = newBox(98, 50);
    b.vx = 600;
    stepBox(b, { pos: null, ang: 0 }, DT, { minX: 0, maxX: 100, minY: 0, maxY: 100 });
    expect(b.x).toBe(100);
    expect(b.vx).toBeLessThan(0);
    expect(b.ax).toBeLessThan(-10000);
  });

  it("원위치는 가장 가까운 바퀴로 — 세 바퀴 돌렸다고 되감지 않는다", () => {
    expect(homeAngle(0.3)).toBe(0);
    expect(homeAngle(6 * Math.PI + 0.2)).toBeCloseTo(6 * Math.PI);
    expect(homeAngle(-2 * Math.PI - 0.4)).toBeCloseTo(-2 * Math.PI);
  });

  it("각도 변화는 ±π 를 넘을 때 튀지 않는다", () => {
    expect(angleDelta(-Math.PI + 0.1, Math.PI - 0.1)).toBeCloseTo(0.2);
    expect(angleDelta(Math.PI - 0.1, -Math.PI + 0.1)).toBeCloseTo(-0.2);
    expect(angleDelta(1, 0.5)).toBeCloseTo(0.5);
  });

  it("통 안쪽은 이동, 가장자리와 바깥은 회전", () => {
    expect(grabMode(0, 0, 100, "square")).toBe("move");
    expect(grabMode(45, 0, 100, "square")).toBe("rotate");
    expect(grabMode(80, 80, 100, "square")).toBe("rotate");
    expect(grabMode(25, 25, 100, "round")).toBe("move");
    expect(grabMode(36, 36, 100, "round")).toBe("rotate");
  });

  it("통이 카드보다 크면 가운데에 고정된다", () => {
    expect(boundsFor(100, 300, 120)).toEqual({ minX: 50, maxX: 50, minY: 60, maxY: 240 });
  });
});

describe("관성력", () => {
  it("가만히 있으면 중력만 아래로", () => {
    const f = inertialForces(newBox(0, 0), opts);
    expect(f.ax).toBeCloseTo(0);
    expect(f.ay).toBeCloseTo(9.8);
  });

  it("통이 시계 방향으로 90° 돌면 통 좌표계에서 중력은 +x 쪽이다", () => {
    // 캔버스 rotate(+90°) 는 통의 +x 축을 화면 아래로 돌린다 — 그쪽 벽이 새 바닥이다.
    const b = newBox(0, 0);
    b.th = Math.PI / 2;
    const f = inertialForces(b, opts);
    expect(f.ax).toBeCloseTo(9.8);
    expect(f.ay).toBeCloseTo(0);
  });

  it("중력 방향 90° 는 왼쪽", () => {
    const f = inertialForces(newBox(0, 0), { ...opts, gravityDirDeg: 90 });
    expect(f.ax).toBeCloseTo(-9.8);
    expect(f.ay).toBeCloseTo(0);
  });

  it("통을 오른쪽으로 가속하면 물은 왼쪽으로 밀린다", () => {
    const b = newBox(0, 0);
    b.ax = 1000; // px/s²
    expect(inertialForces(b, opts).ax).toBeLessThan(0);
  });

  it("민감도 0 이면 흔들어도 물이 모른다", () => {
    const b = newBox(0, 0);
    b.ax = 1e5; b.w = 10; b.alpha = 100;
    const f = inertialForces(b, { ...opts, inertia: 0 });
    expect(f.ax).toBeCloseTo(0);
    expect(f.omega).toBe(0);
    expect(f.alpha).toBe(0);
  });

  it("포인터가 한 번 튀어도 상한에서 잘린다", () => {
    const b = newBox(0, 0);
    b.ax = 1e9; b.w = 1e3; b.alpha = 1e6;
    const f = inertialForces(b, { ...opts, gravity: 0 });
    expect(Math.hypot(f.ax, f.ay)).toBeLessThanOrEqual(60 + 1e-9);
    expect(Math.abs(f.omega)).toBeLessThanOrEqual(25);
    expect(Math.abs(f.alpha)).toBeLessThanOrEqual(300);
  });
});
