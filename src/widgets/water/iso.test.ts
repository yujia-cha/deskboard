import { describe, expect, it } from "vitest";
import { facePixels, isoLayout, newIsoBuffer, paintIso, pickAt, project, U } from "./iso";
import { AIR, BODY0, extrude2d, newField, splat3, surfaceAt, vIdx, WATER } from "./voxels";
import { newRaster } from "./render";
import { FlipSim3 } from "./sim3";

const grey = { r: 100, g: 100, b: 100 };
const pal = { water: grey, bodies: [{ r: 200, g: 50, b: 50 }] };

function painted(buf: ReturnType<typeof newIsoBuffer>) {
  let n = 0;
  for (let i = 3; i < buf.rgba.length; i += 4) if (buf.rgba[i]) n++;
  return n;
}
function countColor(buf: ReturnType<typeof newIsoBuffer>, r: number) {
  let n = 0;
  for (let i = 0; i < buf.rgba.length; i += 4) if (buf.rgba[i + 3] && buf.rgba[i] === r) n++;
  return n;
}

describe("iso", () => {
  it("통의 여덟 꼭짓점이 버퍼 안에 투영된다", () => {
    const res = 12;
    const l = isoLayout(res);
    for (const [x, y, z] of [[0, 0, 0], [res, 0, 0], [0, 0, res], [res, 0, res], [0, res, 0], [res, res, res]]) {
      const p = project(l, x, y, z);
      expect(p.sx).toBeGreaterThanOrEqual(0);
      expect(p.sx).toBeLessThanOrEqual(l.bufW);
      expect(p.sy).toBeGreaterThanOrEqual(0);
      expect(p.sy).toBeLessThanOrEqual(l.bufH);
    }
  });

  it("복셀 하나는 윗면 + 두 옆면 — 겹치지도 비지도 않는다", () => {
    const fp = facePixels();
    // 육각형 넓이 = 마름모(4U·2U/2) + 옆면 둘(2U·2U 씩) = 4U² + 8U² = 12U²
    expect(fp.top + fp.left + fp.right).toBe(12 * U * U);
    const res = 6;
    const f = newField(res);
    f.mat[vIdx(res, 2, 3, 2)] = WATER;
    const l = isoLayout(res);
    const buf = newIsoBuffer(l);
    paintIso(f, l, buf, pal);
    expect(painted(buf)).toBe(12 * U * U);
  });

  it("위 복셀이 아래 복셀의 윗면을, 앞 복셀이 뒤 복셀의 옆면을 가린다", () => {
    const res = 6;
    const l = isoLayout(res);
    const fp = facePixels();
    // 위아래로 쌓인 둘: 아래 것의 윗면이 사라진다
    let f = newField(res);
    f.mat[vIdx(res, 2, 2, 2)] = WATER;
    f.mat[vIdx(res, 2, 3, 2)] = WATER;
    let buf = newIsoBuffer(l);
    paintIso(f, l, buf, pal);
    expect(painted(buf)).toBe(2 * (fp.top + fp.left + fp.right) - fp.top);
    // x 로 나란한 둘: 뒤(작은 x) 것의 +x 면이 사라진다
    f = newField(res);
    f.mat[vIdx(res, 1, 2, 2)] = WATER;
    f.mat[vIdx(res, 2, 2, 2)] = WATER;
    buf = newIsoBuffer(l);
    paintIso(f, l, buf, pal);
    expect(painted(buf)).toBe(2 * (fp.top + fp.left + fp.right) - fp.right);
  });

  it("평평한 수면은 윗면이 res² 개, 속 복셀은 그려지지 않는다", () => {
    const res = 8;
    const f = newField(res);
    for (let ix = 0; ix < res; ix++) for (let iz = 0; iz < res; iz++) for (let iy = 4; iy < res; iy++) f.mat[vIdx(res, ix, iy, iz)] = WATER;
    const l = isoLayout(res);
    const buf = newIsoBuffer(l);
    paintIso(f, l, buf, pal);
    const fp = facePixels();
    const top = Math.round(100 + 155 * 0.25); // TOP_LIGHT, 가장자리 밝힘 없음(옆이 벽)
    expect(countColor(buf, top)).toBe(res * res * fp.top);
    // 전체 = 윗면 res² + 오른면(ix = res−1 열) + 왼면(iz = res−1 열), 각 4층 × res
    expect(painted(buf)).toBe(res * res * fp.top + 4 * res * fp.right + 4 * res * fp.left);
  });

  it("픽셀을 집으면 그 자리의 재질이 나온다", () => {
    const res = 6;
    const f = newField(res);
    f.mat[vIdx(res, 1, 2, 1)] = WATER;
    f.mat[vIdx(res, 4, 2, 4)] = BODY0;
    const l = isoLayout(res);
    const buf = newIsoBuffer(l);
    paintIso(f, l, buf, pal);
    const w = project(l, 1.5, 2, 1.5);
    const b = project(l, 4.5, 2, 4.5);
    expect(pickAt(buf, w.sx, w.sy)).toBe(WATER);
    expect(pickAt(buf, b.sx, b.sy)).toBe(BODY0);
    expect(pickAt(buf, 0, 0)).toBe(AIR);
    expect(pickAt(buf, -5, 3)).toBe(AIR);
  });
});

describe("voxels", () => {
  it("2D 래스터를 깊이로 늘이면 같은 높이의 수면이 된다", () => {
    const res = 8;
    const r = newRaster(res);
    for (let y = 5; y < res; y++) for (let x = 0; x < res; x++) r.occ[y * res + x] = 1;
    const f = newField(res);
    extrude2d(r, f);
    for (let ix = 0; ix < res; ix++) for (let iz = 0; iz < res; iz++) expect(surfaceAt(f, ix, iz)).toBe(5);
  });

  it("막 시드한 입자는 복셀과 정확히 맞아 평면이 된다", () => {
    const sim = new FlipSim3({ res: 12, fill: 0.5 });
    const f = newField(12);
    splat3(sim, f);
    let water = 0;
    for (let i = 0; i < f.mat.length; i++) if (f.mat[i] === WATER) water++;
    expect(water).toBe(sim.count);
    for (let ix = 0; ix < 12; ix++) for (let iz = 0; iz < 12; iz++) expect(surfaceAt(f, ix, iz)).toBe(6);
  });

  it("경계에 걸친 입자도 떨지 않는다 (히스테리시스)", () => {
    const res = 6;
    const sim = new FlipSim3({ res, fill: 0 });
    // 입자 하나를 두 복셀 사이 58% 지점에 둔다 — 가중치 0.42 / 0.58: 한쪽만 켜진다…
    sim.count = 1;
    sim.pos[0] = (2 + 0.5 + 0.58) * sim.voxel; sim.pos[1] = 2.5 * sim.voxel; sim.pos[2] = 2.5 * sim.voxel;
    const f = newField(res);
    splat3(sim, f);
    const a = vIdx(res, 2, 2, 2), b = vIdx(res, 3, 2, 2);
    expect(f.mat[a]).toBe(AIR);
    expect(f.mat[b]).toBe(WATER);
    // …하지만 켜져 있던 복셀은 0.35 까지 유지된다 — 경계에 걸친 입자가 깜빡이지 않는다.
    f.mat[a] = WATER;
    splat3(sim, f);
    expect(f.mat[a]).toBe(WATER);
    expect(f.mat[b]).toBe(WATER);
  });
});
