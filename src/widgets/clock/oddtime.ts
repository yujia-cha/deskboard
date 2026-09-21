/**
 * 읽는 법에 학습이 필요한 "재미" 시계들의 순수 로직.
 * 렌더링과 분리해 두어 그대로 테스트한다.
 */

// --- 바이너리 시계 (BCD) ----------------------------------------------------------
//
// 자리마다 세로 한 줄. 위가 LSB 가 아니라 아래가 LSB 다 (흔한 바이너리 시계 배치).
// 열 높이는 그 자리가 가질 수 있는 최대 숫자에 맞춘다 — 늘 4칸씩 그리면 절대 안 켜지는
// 칸이 생겨서 읽기 어렵다.

/** 각 열이 쓰는 비트 수. [H십, H일, M십, M일, S십, S일] */
export const BCD_BITS = [2, 4, 3, 4, 3, 4] as const;

export interface BcdColumn {
  /** 위에서 아래로 내려가는 비트들. true = 켜짐. 마지막이 1의 자리. */
  bits: boolean[];
  /** 이 열이 나타내는 숫자 (툴팁용). */
  digit: number;
  /** 시/분/초 중 무엇인지 — 색을 다르게 준다. */
  unit: "h" | "m" | "s";
}

/** 시:분:초를 BCD 열 6개로. `seconds` 가 false 면 초 두 열을 뺀다. */
export function bcdColumns(h: number, m: number, s: number, seconds = true): BcdColumn[] {
  const digits: [number, "h" | "m" | "s"][] = [
    [Math.floor(h / 10), "h"], [h % 10, "h"],
    [Math.floor(m / 10), "m"], [m % 10, "m"],
    [Math.floor(s / 10), "s"], [s % 10, "s"],
  ];
  const take = seconds ? 6 : 4;
  return digits.slice(0, take).map(([digit, unit], i) => {
    const n = BCD_BITS[i];
    // 위(MSB) → 아래(LSB)
    const bits = Array.from({ length: n }, (_, k) => (digit & (1 << (n - 1 - k))) !== 0);
    return { bits, digit, unit };
  });
}

// --- 피보나치 시계 ---------------------------------------------------------------
//
// 1,1,2,3,5 크기 정사각형 5개. 시(1~12)와 분/5(0~11)를 각각 "칸 합"으로 나타낸다.
// 시에만 쓰이면 hour 색, 분에만 쓰이면 minute 색, 둘 다면 both 색, 안 쓰이면 꺼짐.

/** 칸 값 — 이 순서가 곧 화면 배치 순서다. */
export const FIB_VALUES = [1, 1, 2, 3, 5] as const;

export type FibRole = "off" | "hour" | "minute" | "both";

/** 합이 `target` 이 되는 칸 조합 전부 (비트마스크). */
export function fibCombos(target: number): number[] {
  const out: number[] = [];
  for (let mask = 0; mask < 32; mask++) {
    let sum = 0;
    for (let i = 0; i < 5; i++) if (mask & (1 << i)) sum += FIB_VALUES[i];
    if (sum === target) out.push(mask);
  }
  return out;
}

/**
 * 시각을 칸 5개의 역할로. 합이 같은 조합이 여러 개라 `variant` 로 하나를 고른다
 * (실물 피보나치 시계도 매분 조합을 바꿔 보여준다).
 */
export function fibRoles(h: number, m: number, variant = 0): FibRole[] {
  const hour = ((h % 12) + 12) % 12 || 12; // 1~12
  const min = Math.floor((((m % 60) + 60) % 60) / 5); // 0~11
  const pick = (target: number) => {
    const all = fibCombos(target);
    return all.length === 0 ? 0 : all[variant % all.length];
  };
  const hm = pick(hour);
  const mm = pick(min);
  return FIB_VALUES.map((_, i) => {
    const inH = (hm & (1 << i)) !== 0;
    const inM = (mm & (1 << i)) !== 0;
    if (inH && inM) return "both";
    if (inH) return "hour";
    if (inM) return "minute";
    return "off";
  });
}

/** 켜진 칸에서 시각을 되읽는다 — 테스트와 툴팁용. */
export function fibReadback(roles: FibRole[]): { hour: number; minute: number } {
  let hour = 0;
  let minute = 0;
  roles.forEach((r, i) => {
    if (r === "hour" || r === "both") hour += FIB_VALUES[i];
    if (r === "minute" || r === "both") minute += FIB_VALUES[i];
  });
  return { hour, minute: minute * 5 };
}
