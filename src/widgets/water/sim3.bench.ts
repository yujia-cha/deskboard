/**
 * sim3 스텝 비용 — `npx vitest bench src/widgets/water/sim3.bench.ts`.
 * 모듈을 불러올 때 구간별 프로파일 표를 먼저 찍고(res 22 기본값), 그다음 설정 조합별 스텝 시간을 잰다.
 */
import { FlipSim3, PHASES, type Forces3, type SimParams3 } from "./sim3";
import { PRESETS, simParamsOf } from "./params";

const still: Forces3 = { ax: 0, ay: 9.8, az: 0, wx: 0, wy: 0, wz: 0, alx: 0, aly: 0, alz: 0 };
const base: SimParams3 = { ...simParamsOf(PRESETS.water), pressureIters: 30, separationIters: 1, densityK: 1 };

function make(res: number, hashCell: number) {
  return new FlipSim3({ res, fill: 0.45, hashCell });
}

// 흔드는 동안의 비용을 재야 한다 — 고인 물은 유체 칸이 적고 이웃이 정렬돼 있어 싸게 나온다.
const shake = (t: number): Forces3 => ({ ...still, ax: 20 * Math.sin(t * 7), ay: 9.8 + 10 * Math.cos(t * 5) });

function profile(res: number, hashCell: number, p: SimParams3, steps = 600) {
  const sim = make(res, hashCell);
  for (let k = 0; k < 120; k++) sim.step(shake(k * sim.dt), p);
  sim.prof = new Float64Array(PHASES.length);
  const times: number[] = [];
  for (let k = 0; k < steps; k++) {
    const t0 = performance.now();
    sim.step(shake(k * sim.dt), p);
    times.push(performance.now() - t0);
  }
  times.sort((a, b) => a - b);
  const q = (f: number) => times[Math.min(times.length - 1, Math.floor(f * times.length))].toFixed(2);
  const phases = PHASES.map((n, i) => `${n} ${(sim.prof![i] / steps).toFixed(2)}`).join(" · ");
  console.log(`res ${res} n ${sim.n} particles ${sim.count} hash ${hashCell}r sep ${p.separationIters} iters ${p.pressureIters}: p50 ${q(0.5)} p95 ${q(0.95)} ms | ${phases}`);
}

profile(20, 2.5, base);
profile(18, 2.5, base);
profile(22, 2.5, base);
profile(22, 2, base);
profile(22, 3, base);
profile(22, 2.5, { ...base, separationIters: 2 });
profile(22, 2.5, { ...base, pressureIters: 60 });
