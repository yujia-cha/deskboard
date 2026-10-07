/**
 * 워드 클락 — QLOCKTWO 스타일. 글자 격자에서 지금 시각에 해당하는 단어만 밝힌다.
 *
 * 5분 단위로만 표현된다 (그게 이 시계의 정체성이다).
 * 격자와 "어떤 칸을 켤지"는 순수 데이터라 그대로 테스트한다.
 */

/** 격자 한 줄에서 연속된 칸 범위를 켠다. [행, 시작열, 길이] */
export type Span = [row: number, col: number, len: number];

export interface WordGrid {
  /** 각 줄의 글자들. 모든 줄 길이가 같아야 한다. */
  rows: string[];
  /** 시각 → 켜야 할 범위들. */
  spansFor(h: number, m: number): Span[];
}

// --- 영어 (QLOCKTWO 영문판 배치) -------------------------------------------------
const EN_ROWS = [
  "ITLISASTIME",
  "ACQUARTERDC",
  "TWENTYFIVEX",
  "HALFSTENFTO",
  "PASTERUNINE",
  "ONESIXTHREE",
  "FOURFIVETWO",
  "EIGHTELEVEN",
  "SEVENTWELVE",
  "TENSEOCLOCK",
];

/** 영어 격자에서 "시" 단어가 있는 위치. */
const EN_HOURS: Record<number, Span> = {
  1: [5, 0, 3],   // ONE
  2: [6, 8, 3],   // TWO
  3: [5, 6, 5],   // THREE
  4: [6, 0, 4],   // FOUR
  5: [6, 4, 4],   // FIVE
  6: [5, 3, 3],   // SIX
  7: [8, 0, 5],   // SEVEN
  8: [7, 0, 5],   // EIGHT
  9: [4, 7, 4],   // NINE
  10: [9, 0, 3],  // TEN
  11: [7, 5, 6],  // ELEVEN
  12: [8, 5, 6],  // TWELVE
};

const EN_IT_IS: Span[] = [[0, 0, 2], [0, 3, 2]]; // IT IS
const EN_PAST: Span = [4, 0, 4];
const EN_TO: Span = [3, 9, 2];
const EN_OCLOCK: Span = [9, 5, 6];
/** 분 단위 표현. [범위들, "past/to" 를 붙일지] */
const EN_MINUTES: Record<number, Span[]> = {
  5: [[2, 6, 4]],            // FIVE
  10: [[3, 5, 3]],           // TEN
  15: [[1, 2, 7]],           // QUARTER
  20: [[2, 0, 6]],           // TWENTY
  25: [[2, 0, 6], [2, 6, 4]], // TWENTY FIVE
  30: [[3, 0, 4]],           // HALF
};

export const EN: WordGrid = {
  rows: EN_ROWS,
  spansFor(h, m) {
    const slot = Math.floor(m / 5) * 5;
    const out: Span[] = [...EN_IT_IS];
    // 35분부터는 다음 시 기준 "to" 로 읽는다 (TWENTY FIVE TO ELEVEN).
    const toNext = slot > 30;
    const mins = toNext ? 60 - slot : slot;
    const hour12 = (x: number) => ((x % 12) + 12) % 12 || 12;
    const hour = hour12(toNext ? h + 1 : h);

    if (mins === 0) {
      out.push(EN_HOURS[hour], EN_OCLOCK);
      return out;
    }
    out.push(...EN_MINUTES[mins]);
    out.push(toNext ? EN_TO : EN_PAST);
    out.push(EN_HOURS[hour]);
    return out;
  },
};

// --- 한국어 ------------------------------------------------------------------
// 한국어는 "열시 이십오분" 처럼 시가 먼저 오고 to 표현을 쓰지 않는다.
// 고유어 시(한시~열두시) + 한자어 분을 쓴다.
const KO_ROWS = [
  "지금은한두세",
  "네다섯여섯일",
  "곱여덟아홉열",
  "열한열두시정",
  "각오십십오이",
  "십이십오삼십",
  "삼십오사십사",
  "십오오십오분",
];

const KO_HOURS: Record<number, Span[]> = {
  1: [[0, 3, 1]],            // 한
  2: [[0, 4, 1]],            // 두
  3: [[0, 5, 1]],            // 세
  4: [[1, 0, 1]],            // 네
  5: [[1, 1, 2]],            // 다섯
  6: [[1, 3, 2]],            // 여섯
  7: [[1, 5, 1], [2, 0, 1]], // 일곱
  8: [[2, 1, 2]],            // 여덟
  9: [[2, 3, 2]],            // 아홉
  10: [[2, 5, 1]],           // 열
  11: [[3, 0, 2]],           // 열한
  12: [[3, 2, 2]],           // 열두
};

const KO_SI: Span = [3, 4, 1];      // 시
const KO_JEONGGAK: Span[] = [[3, 5, 1], [4, 0, 1]]; // 정각
const KO_BUN: Span = [7, 5, 1];     // 분
const KO_MINUTES: Record<number, Span[]> = {
  5: [[4, 3, 1]],            // 오
  10: [[4, 2, 1]],           // 십
  15: [[4, 2, 1], [4, 3, 1]], // 십오
  20: [[4, 4, 1], [5, 0, 1]], // 이십
  25: [[5, 1, 3]],           // 이십오  (이십오 연속)
  30: [[5, 4, 2]],           // 삼십
  35: [[6, 0, 3]],           // 삼십오
  40: [[6, 4, 1], [7, 0, 1]], // 사십
  45: [[6, 5, 1], [7, 0, 2]], // 사십오
  50: [[7, 2, 2]],           // 오십
  55: [[7, 2, 3]],           // 오십오
};

export const KO: WordGrid = {
  rows: KO_ROWS,
  spansFor(h, m) {
    const slot = Math.floor(m / 5) * 5;
    const hour = ((h % 12) + 12) % 12 || 12;
    const out: Span[] = [[0, 0, 3]]; // 지금은
    out.push(...KO_HOURS[hour], KO_SI);
    if (slot === 0) {
      out.push(...KO_JEONGGAK);
    } else {
      out.push(...KO_MINUTES[slot], KO_BUN);
    }
    return out;
  },
};

export const GRIDS: Record<"en" | "ko", WordGrid> = { en: EN, ko: KO };

/** 켜야 할 칸들을 "행,열" 집합으로 편다 — 렌더러가 칸마다 조회한다. */
export function litCells(grid: WordGrid, h: number, m: number): Set<string> {
  const out = new Set<string>();
  for (const [r, c, len] of grid.spansFor(h, m))
    for (let i = 0; i < len; i++) out.add(`${r},${c + i}`);
  return out;
}

/** 켜진 칸들을 읽어 만든 문장 — 테스트와 툴팁에 쓴다. */
export function readout(grid: WordGrid, h: number, m: number): string {
  return grid
    .spansFor(h, m)
    .map(([r, c, len]) => grid.rows[r].slice(c, c + len))
    .join(" ");
}
