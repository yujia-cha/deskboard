/**
 * 장면 — **여기를 고쳐서** 물의 성질, 통의 모양, 떠 있는 물체, 비현실적인 법칙을 바꾼다.
 * 설정 패널의 "장면" 은 이 목록에서 고른다 (`index.ts` 가 `SCENES` 로 선택지를 만든다).
 *
 * 장면 하나 = 법칙(`laws.ts` 의 Laws) + 바디 목록(`bodies.ts` 의 BodySpec).
 *  - `shape`: "box" | "cylinder" | "bowl" | "sphere" 또는 SDF 함수 `(x, y, z) => 거리` (벽면 0, 안 음수).
 *  - `field`: 입자마다 더해지는 가속. `equipotential(표면 모양)` 으로 "표면이 늘 이 모양" 법칙을 만든다.
 *  - `seed`: 처음 물을 둘 자리. `fluid`: 액체 프리셋/물리 값(사용자 설정보다 우선).
 *  - `bodies`: ASCII 층으로 그린 물체. 밀도 < 1 이면 뜨고 > 1 이면 가라앉는다. 색은 CSS 토큰만
 *    (`var(--text)`, `var(--text-dim)`, `var(--accent)`, `var(--ok)`, `var(--warn)`, `var(--danger)`, `var(--border)`).
 *
 * 좌표는 통 안쪽 [0, BOX]³ (BOX = 1.5 m), y 는 **아래**로 커진다. 바디의 `at` 은 0~1 비율.
 */
import type { BodySpec } from "./bodies";
import { catEars, equipotential, hollow, peak, slope, type Laws } from "./laws";

export interface Scene extends Laws {
  /** 설정 패널에 보이는 이름 */
  label: string;
  bodies: BodySpec[];
}

// --- 물체 -------------------------------------------------------------------------

/** 작은 낚싯배 — 선체 두 층 + 조타실. 가볍다(0.35) */
export const boat: BodySpec = {
  name: "boat",
  layers: [
    [".hhh.", ".hhh.", ".hhh."],
    ["hhhhh", "hhhhh", "hhhhh"],
    [".....", "..cc.", "....."],
    [".....", "..cc.", "....."],
  ],
  palette: { h: "var(--text)", c: "var(--text-dim)" },
  density: 0.35,
  at: [0.5, 0.3, 0.5],
};

/** 고무 오리 — 몸통 + 머리. 아주 가볍다 */
export const duck: BodySpec = {
  name: "duck",
  layers: [
    [".dd.", "dddd", ".dd."],
    ["....", ".dd.", "...."],
    ["....", "..d.", "...."],
  ],
  palette: { d: "var(--warn)" },
  density: 0.25,
  at: [0.3, 0.3, 0.65],
};

/** 돌 — 가라앉는다 */
export const stone: BodySpec = {
  name: "stone",
  layers: [
    [".ss.", "ssss", ".ss."],
    ["....", ".ss.", "...."],
  ],
  palette: { s: "var(--text-dim)" },
  density: 2.6,
  at: [0.7, 0.2, 0.3],
};

/** 얼음 — 거의 다 잠긴 채 뜬다 (0.92) */
export const ice: BodySpec = {
  name: "ice",
  layers: [["iii", "iii", "iii"], ["iii", "iii", "iii"], ["iii", "iii", "iii"]],
  palette: { i: "var(--border)" },
  density: 0.92,
  at: [0.5, 0.25, 0.5],
};

// --- 장면 -------------------------------------------------------------------------

export const SCENES: Record<string, Scene> = {
  default: { label: "물통", bodies: [], render: { caustics: true } },
  boat: { label: "낚싯배", bodies: [boat], render: { caustics: true } },
  pond: { label: "연못 (오리·돌)", bodies: [duck, stone], fluid: { preset: "water" }, render: { caustics: true } },
  ice: { label: "얼음 조각", bodies: [ice] },
  catears: {
    label: "고양이 귀 (표면이 늘 귀 모양)",
    field: equipotential(catEars(), 9.8),
    bodies: [],
  },
  peak: { label: "가운데가 솟은 물", field: equipotential(peak(), 9.8), bodies: [] },
  slope: { label: "한쪽이 높은 물", field: equipotential(slope(), 9.8), bodies: [] },
  hollow: { label: "구멍 뚫린 물", ...hollow(), bodies: [] },
  cylinder: { label: "원기둥 통", shape: "cylinder", bodies: [] },
  bowl: { label: "그릇", shape: "bowl", bodies: [] },
  honeypot: { label: "꿀단지 (원기둥·꿀)", shape: "cylinder", fluid: { preset: "honey" }, bodies: [] },
  moonpool: { label: "달의 수영장 (중력 1/6·오리)", fluid: { preset: "moon" }, bodies: [duck] },
};

export function sceneOf(name: unknown): Scene {
  return (typeof name === "string" && SCENES[name]) || SCENES.default;
}
