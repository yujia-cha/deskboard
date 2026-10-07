/**
 * 물에 뜨거나 가라앉는 복셀 물체(바디). 장면 파일이 ASCII 층으로 모양을 그리고 밀도를 준다 —
 * 물(1)보다 가벼우면 뜨고 무거우면 가라앉는다. 병진만 하고 돌지는 않는다.
 *
 * 물과의 결합:
 *  - 물 → 바디: 아르키메데스. 바디 둘레 기둥의 수면 높이에서 **수면 평면**을 잡고(법선 = 그 순간의
 *    유효 중력 방향 — 기울인 통에서도 맞다), 그 아래 잠긴 복셀의 부피만큼 중력 반대 방향으로 민다.
 *    여기에 격자 속도와의 차이에 비례하는 항력. 압력장을 적분하지 않는 이유: 솔버가 압력을 저장하지
 *    않고 반복 수에 따라 잡음이 커서 작은 바디가 떤다.
 *  - 바디 → 물: 바디가 덮은 격자 칸은 그 스텝 동안 **움직이는 벽**이다(면 속도 = 바디 속도).
 *    바디 상자 안에 들어온 입자는 가장 가까운 면으로 밀어낸다.
 * DOM 에 의존하지 않는다.
 */
import { BOX } from "./sim";
import type { FlipSim3, Forces3 } from "./sim3";
import { BODY0, surfaceAt, type VoxelField } from "./voxels";
import { projectOut, type Sdf } from "./shapes";

export interface BodySpec {
  name: string;
  /**
   * 모양 — 아래층부터 위층 순서의 ASCII 층. 각 층은 z 방향 줄(앞이 작은 z)의 배열, 줄의 글자는 x.
   * `.` 과 공백은 빈 칸, 그 밖의 글자는 `palette` 의 색이다.
   */
  layers: string[][];
  /** 글자 → CSS 색 (`var(--text)` 같은 토큰) */
  palette: Record<string, string>;
  /** 물 = 1. 작으면 뜬다 */
  density: number;
  /** 처음 자리 — 통 크기 비율 (0~1). y 는 아래로 커진다. 생략하면 가운데 위 */
  at?: [number, number, number];
}

export interface Body {
  spec: BodySpec;
  /** 복셀 로컬 좌표 (ix, iy, iz) 묶음 — 모델 원점 기준 */
  local: Int16Array;
  /** 복셀마다 재질 번호 */
  mats: Uint8Array;
  /** 모델 크기 (복셀) */
  size: [number, number, number];
  /** 중심 (m), 속도 (m/s) */
  pos: [number, number, number];
  vel: [number, number, number];
  /** 복셀 부피 (m³) — 통의 복셀 크기로 정해진다 */
  volume: number;
  /** 잡혀 있을 때의 목표 (m) */
  grab: [number, number, number] | null;
  /** 직전 스텝의 잠긴 비율 0~1 (관찰·테스트용) */
  submerged: number;
}

export interface BodySet {
  bodies: Body[];
  /** 재질 번호(BODY0 + i) → 색 토큰 */
  matColors: string[];
  /** 재질 번호 → 바디 번호 */
  matBody: number[];
}

/** 잡은 바디가 손을 따라가는 스프링 */
const K_GRAB = 400;
const C_GRAB = 2 * Math.sqrt(K_GRAB) * 0.9;
/** 물속 항력 (1/s) · 공기 저항 (1/s). 작은 물체는 물에서 세게 감쇠된다 — 약하면 부력에 튕겨 오르내린다 */
const DRAG_WATER = 10;
const DRAG_AIR = 0.3;
/** 바디 속력 상한 (m/s). 빠르면 물에 구덩이를 내고 그 구덩이가 부력 판정을 흔든다 */
const V_MAX = 1.2;

/** 장면의 바디 명세를 바디로. 모두 같은 복셀 크기(`voxel`, m)를 쓴다. */
export function buildBodies(specs: BodySpec[], voxel: number): BodySet {
  const bodies: Body[] = [];
  const matColors: string[] = [];
  const matBody: number[] = [];
  specs.forEach((spec, bi) => {
    const local: number[] = [];
    const mats: number[] = [];
    const matOf = new Map<string, number>();
    let sx = 0, sz = 0;
    const sy = spec.layers.length;
    spec.layers.forEach((layer, iyUp) => {
      const iy = sy - 1 - iyUp; // 층 배열은 아래부터, 복셀 y 는 아래로 커지므로 뒤집는다
      sz = Math.max(sz, layer.length);
      layer.forEach((row, iz) => {
        sx = Math.max(sx, row.length);
        for (let ix = 0; ix < row.length; ix++) {
          const ch = row[ix];
          if (ch === "." || ch === " ") continue;
          let m = matOf.get(ch);
          if (m === undefined) {
            m = BODY0 + matColors.length;
            matOf.set(ch, m);
            matColors.push(spec.palette[ch] ?? "var(--text)");
            matBody.push(bi);
          }
          local.push(ix, iy, iz);
          mats.push(m);
        }
      });
    });
    const at = spec.at ?? [0.5, 0.35, 0.5];
    bodies.push({
      spec,
      local: Int16Array.from(local),
      mats: Uint8Array.from(mats),
      size: [sx, sy, sz],
      pos: [at[0] * BOX, at[1] * BOX, at[2] * BOX],
      vel: [0, 0, 0],
      volume: voxel * voxel * voxel,
      grab: null,
      submerged: 0,
    });
  });
  return { bodies, matColors, matBody };
}

/** 바디의 축 정렬 상자 (m) */
export function aabb(b: Body, voxel: number) {
  const hx = (b.size[0] * voxel) / 2, hy = (b.size[1] * voxel) / 2, hz = (b.size[2] * voxel) / 2;
  return { x0: b.pos[0] - hx, x1: b.pos[0] + hx, y0: b.pos[1] - hy, y1: b.pos[1] + hy, z0: b.pos[2] - hz, z1: b.pos[2] + hz };
}

/** 바디가 덮은 격자 칸을 이번 스텝의 벽으로 표시한다. */
export function markSolid(sim: FlipSim3, set: BodySet) {
  sim.resetSolids();
  if (sim.bodyVel.length < set.bodies.length * 3) sim.bodyVel = new Float32Array(set.bodies.length * 3);
  const { n, h } = sim;
  set.bodies.forEach((b, bi) => {
    sim.bodyVel[3 * bi] = b.vel[0]; sim.bodyVel[3 * bi + 1] = b.vel[1]; sim.bodyVel[3 * bi + 2] = b.vel[2];
    const box = aabb(b, sim.voxel);
    // 칸 [(i−1)h, i·h] 이 **통째로** 상자 안인 칸만. 중심만 보고 표시하면 벽 칸이 상자 밖으로 반 칸
    // 삐져나와, 상자 밖(입자 배제 밖)인데 압력이 안 닿는 칸에 입자가 뭉쳤다 (실측: 물 부피 6% 손실).
    // 한 칸보다 얇은 바디는 격자에 안 잡힌다 — 입자 배제만으로 움직인다.
    const i0 = Math.max(1, Math.ceil(box.x0 / h) + 1), i1 = Math.min(n - 2, Math.floor(box.x1 / h));
    const j0 = Math.max(1, Math.ceil(box.y0 / h) + 1), j1 = Math.min(n - 2, Math.floor(box.y1 / h));
    const k0 = Math.max(1, Math.ceil(box.z0 / h) + 1), k1 = Math.min(n - 2, Math.floor(box.z1 / h));
    for (let i = i0; i <= i1; i++) for (let j = j0; j <= j1; j++) for (let k = k0; k <= k1; k++) {
      const cell = sim.idx(i, j, k);
      if (!sim.isWall(cell)) sim.setBodyCell(cell, bi);
    }
  });
}

/** 바디 상자 안에 들어온 입자를 가장 가까운 면으로 밀어내고, 바디 쪽으로 가던 속도를 바디 속도에 맞춘다. */
export function excludeParticles(sim: FlipSim3, set: BodySet) {
  const { pos, vel, r } = sim;
  for (const b of set.bodies) {
    const box = aabb(b, sim.voxel);
    const x0 = box.x0 - r, x1 = box.x1 + r, y0 = box.y0 - r, y1 = box.y1 + r, z0 = box.z0 - r, z1 = box.z1 + r;
    for (let i = 0; i < sim.count; i++) {
      const o = 3 * i;
      const x = pos[o], y = pos[o + 1], z = pos[o + 2];
      if (x <= x0 || x >= x1 || y <= y0 || y >= y1 || z <= z0 || z >= z1) continue;
      // 여섯 면까지의 거리 중 가장 작은 쪽으로
      const dx0 = x - x0, dx1 = x1 - x, dy0 = y - y0, dy1 = y1 - y, dz0 = z - z0, dz1 = z1 - z;
      let m = dx0, axis = 0, sign = -1;
      if (dx1 < m) { m = dx1; axis = 0; sign = 1; }
      if (dy0 < m) { m = dy0; axis = 1; sign = -1; }
      if (dy1 < m) { m = dy1; axis = 1; sign = 1; }
      if (dz0 < m) { m = dz0; axis = 2; sign = -1; }
      if (dz1 < m) { m = dz1; axis = 2; sign = 1; }
      pos[o + axis] += sign * m;
      const rel = vel[o + axis] - b.vel[axis];
      if (rel * sign < 0) vel[o + axis] = b.vel[axis];
    }
  }
}

/**
 * 바디 한 스텝. `f` 는 통 좌표계의 균일 가속(중력 − 통 가속), `field` 는 복셀 장(수면 높이).
 * 수면 평면: 바디 발자국 둘레 한 겹 기둥의 수면 평균 → 점 p0, 법선 = 가속 방향.
 */
export function stepBodies(set: BodySet, sim: FlipSim3, vox: VoxelField, f: Forces3, dt: number, sdf: Sdf) {
  const { voxel, res } = sim;
  const g = Math.hypot(f.ax, f.ay, f.az);
  const gx = g > 1e-6 ? f.ax / g : 0, gy = g > 1e-6 ? f.ay / g : 1, gz = g > 1e-6 ? f.az / g : 0;
  const tmp = new Float32Array(3);
  for (const b of set.bodies) {
    const box = aabb(b, voxel);
    // 수면 높이 — 발자국에서 두세 복셀 떨어진 둘레 기둥들의 **중앙값** (복셀 → m). 바로 옆 기둥은
    // 바디가 밀어낸 구덩이라 낮게 나오고, 평균은 그 구덩이에 끌려간다. 물이 없는 기둥은 뺀다.
    const ix0 = Math.max(0, Math.floor(box.x0 / voxel) - 3), ix1 = Math.min(res - 1, Math.floor(box.x1 / voxel) + 3);
    const iz0 = Math.max(0, Math.floor(box.z0 / voxel) - 3), iz1 = Math.min(res - 1, Math.floor(box.z1 / voxel) + 3);
    const inX0 = Math.floor(box.x0 / voxel) - 1, inX1 = Math.floor(box.x1 / voxel) + 1;
    const inZ0 = Math.floor(box.z0 / voxel) - 1, inZ1 = Math.floor(box.z1 / voxel) + 1;
    const heights: number[] = [];
    for (let ix = ix0; ix <= ix1; ix++) {
      for (let iz = iz0; iz <= iz1; iz++) {
        if (ix >= inX0 && ix <= inX1 && iz >= inZ0 && iz <= inZ1) continue;
        const sy = surfaceAt(vox, ix, iz);
        if (sy < res) heights.push(sy);
      }
    }
    let sub = 0;
    if (heights.length > 0 && g > 1e-6) {
      heights.sort((a, c) => a - c);
      const yw = heights[heights.length >> 1] * voxel; // 수면 (복셀 위 경계 → 그 복셀의 윗면)
      // 평면: 바디 중심 기둥 위의 수면점 p0 = (cx, yw, cz), 법선 ĝ. 복셀마다 잠긴 비율을 선형으로 —
      // 중심만 보면 0/1 로 뛰어 부력이 계단이 되고 바디가 튄다.
      const p0x = b.pos[0], p0y = yw, p0z = b.pos[2];
      const hx = (b.size[0] * voxel) / 2, hy = (b.size[1] * voxel) / 2, hz = (b.size[2] * voxel) / 2;
      let inside = 0;
      for (let q = 0; q < b.local.length; q += 3) {
        const px = b.pos[0] - hx + (b.local[q] + 0.5) * voxel;
        const py = b.pos[1] - hy + (b.local[q + 1] + 0.5) * voxel;
        const pz = b.pos[2] - hz + (b.local[q + 2] + 0.5) * voxel;
        const depth = (px - p0x) * gx + (py - p0y) * gy + (pz - p0z) * gz; // 수면 아래가 양수
        inside += Math.max(0, Math.min(1, depth / voxel + 0.5));
      }
      sub = inside / (b.local.length / 3);
    }
    b.submerged = sub;
    // 가속 = 중력·관성 − 부력(잠긴 비율 / 밀도) + 항력 + 잡기 스프링
    const buoy = 1 - sub / Math.max(0.05, b.spec.density);
    let ax = f.ax * buoy, ay = f.ay * buoy, az = f.az * buoy;
    sim.sampleVelocity(b.pos[0], b.pos[1], b.pos[2], tmp);
    const drag = DRAG_WATER * sub + DRAG_AIR * (1 - sub);
    ax += drag * (tmp[0] * sub - b.vel[0]);
    ay += drag * (tmp[1] * sub - b.vel[1]);
    az += drag * (tmp[2] * sub - b.vel[2]);
    if (b.grab) {
      ax += K_GRAB * (b.grab[0] - b.pos[0]) - C_GRAB * b.vel[0];
      ay += K_GRAB * (b.grab[1] - b.pos[1]) - C_GRAB * b.vel[1];
      az += K_GRAB * (b.grab[2] - b.pos[2]) - C_GRAB * b.vel[2];
    }
    b.vel[0] += ax * dt; b.vel[1] += ay * dt; b.vel[2] += az * dt;
    // 한 스텝에 격자 반 칸 넘게 움직이면 입자가 바디를 뚫는다 — 그리고 V_MAX
    const vmax = Math.min(V_MAX, (0.5 * sim.h) / dt);
    const sp = Math.hypot(b.vel[0], b.vel[1], b.vel[2]);
    if (sp > vmax) { const k = vmax / sp; b.vel[0] *= k; b.vel[1] *= k; b.vel[2] *= k; }
    b.pos[0] += b.vel[0] * dt; b.pos[1] += b.vel[1] * dt; b.pos[2] += b.vel[2] * dt;
    // 통 안에 가둔다 — 사각 영역으로 먼저, 모양이 다르면 SDF 로 한 번 더
    const hx = (b.size[0] * voxel) / 2, hy = (b.size[1] * voxel) / 2, hz = (b.size[2] * voxel) / 2;
    clampAxis(b, 0, hx); clampAxis(b, 1, hy); clampAxis(b, 2, hz);
    const rad = Math.min(hx, hy, hz);
    const d = sdf(b.pos[0], b.pos[1], b.pos[2]);
    if (d > -rad) {
      projectOut(sdf, b.pos[0], b.pos[1], b.pos[2], voxel * 0.5, tmp);
      const push = d + rad;
      b.pos[0] -= tmp[0] * push; b.pos[1] -= tmp[1] * push; b.pos[2] -= tmp[2] * push;
      const vn = b.vel[0] * tmp[0] + b.vel[1] * tmp[1] + b.vel[2] * tmp[2];
      if (vn > 0) { b.vel[0] -= vn * tmp[0]; b.vel[1] -= vn * tmp[1]; b.vel[2] -= vn * tmp[2]; }
    }
  }
}

function clampAxis(b: Body, axis: number, half: number) {
  if (b.pos[axis] < half) { b.pos[axis] = half; if (b.vel[axis] < 0) b.vel[axis] = 0; }
  if (b.pos[axis] > BOX - half) { b.pos[axis] = BOX - half; if (b.vel[axis] > 0) b.vel[axis] = 0; }
}

/** 바디 복셀을 복셀 장에 찍는다 (물 위에 덮어쓴다). `splat3` 뒤에 부른다. */
export function paintBodies(set: BodySet, vox: VoxelField) {
  const { res, mat } = vox;
  const voxel = BOX / res;
  for (const b of set.bodies) {
    const hx = (b.size[0] * voxel) / 2, hy = (b.size[1] * voxel) / 2, hz = (b.size[2] * voxel) / 2;
    for (let q = 0; q < b.local.length; q += 3) {
      const ix = Math.floor((b.pos[0] - hx) / voxel + b.local[q] + 0.5);
      const iy = Math.floor((b.pos[1] - hy) / voxel + b.local[q + 1] + 0.5);
      const iz = Math.floor((b.pos[2] - hz) / voxel + b.local[q + 2] + 0.5);
      if (ix < 0 || iy < 0 || iz < 0 || ix >= res || iy >= res || iz >= res) continue;
      mat[(ix * res + iy) * res + iz] = b.mats[q / 3];
    }
  }
}

/** 바디 안에 든 입자 수 (테스트용) */
export function particlesInside(sim: FlipSim3, b: Body): number {
  const box = aabb(b, sim.voxel);
  let n = 0;
  for (let i = 0; i < sim.count; i++) {
    const x = sim.pos[3 * i], y = sim.pos[3 * i + 1], z = sim.pos[3 * i + 2];
    if (x > box.x0 && x < box.x1 && y > box.y0 && y < box.y1 && z > box.z0 && z < box.z1) n++;
  }
  return n;
}
