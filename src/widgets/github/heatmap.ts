/** 기여도 잔디의 계산 — DOM 없이 되는 부분만 모아 둔다. */

export interface Day {
  /** "YYYY-MM-DD" */
  date: string;
  count: number;
  /** 0~4 (백엔드가 그 해 최댓값 기준으로 매긴다) */
  level: number;
}

/** 요일 칸 수. 한 열이 일요일~토요일 한 주다. */
export const ROWS = 7;

/**
 * 한 주를 7칸으로 채운다 — 빈칸은 `null`.
 *
 * GitHub 은 **처음과 마지막 주를 잘라서** 준다 (오늘이 수요일이면 마지막 주는 4일뿐).
 * 그대로 그리면 마지막 열이 위로 밀려 요일 줄이 어긋난다. 날짜의 요일로 앞뒤를 메운다.
 */
export function padWeek(week: Day[]): (Day | null)[] {
  if (week.length === 0) return Array<Day | null>(ROWS).fill(null);
  const lead = new Date(`${week[0].date}T00:00:00Z`).getUTCDay(); // 0=일
  const out: (Day | null)[] = [...Array<Day | null>(Number.isNaN(lead) ? 0 : lead).fill(null), ...week];
  while (out.length < ROWS) out.push(null);
  return out.slice(0, ROWS);
}

/** 위젯 크기에 맞는 칸 크기와 보여줄 주 수. 폭이 모자라면 **최근 주부터** 남긴다. */
export function fitGrid(size: { w: number; h: number }, gap = 2): { cell: number; cols: number } {
  // 잔디가 위젯의 절반을 넘지 않게 — 아래 저장소 목록이 보일 자리를 남긴다.
  const cell = Math.max(4, Math.min(12, Math.floor((size.h * 0.45 - (ROWS - 1) * gap) / ROWS)));
  const cols = Math.max(4, Math.floor((size.w + gap) / (cell + gap)));
  return { cell, cols };
}
