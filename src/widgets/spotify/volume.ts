/** 볼륨/음소거 순수 로직 — UI/IPC 없이 테스트 가능. */

export function clampVolume(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.max(0, Math.min(100, Math.round(v)));
}

/** 세로 슬라이더 트랙에서 pointer y 좌표 → 볼륨(0=아래, 100=위). */
export function volumeFromPointerY(clientY: number, trackTop: number, trackHeight: number): number {
  if (trackHeight <= 0) return 0;
  const ratio = 1 - (clientY - trackTop) / trackHeight;
  return clampVolume(ratio * 100);
}

export interface MuteState { volume: number; prevVolume: number | null }

/**
 * 음소거 아이콘 클릭 토글.
 * - 켜져 있으면(volume > 0): 0 으로 내리고 직전 값을 기억한다.
 * - 꺼져 있으면(volume === 0): 기억해둔 값(없거나 0이면 50)으로 복원한다.
 */
export function toggleMute(state: MuteState): MuteState {
  if (state.volume > 0) return { volume: 0, prevVolume: state.volume };
  const restored = state.prevVolume && state.prevVolume > 0 ? state.prevVolume : 50;
  return { volume: restored, prevVolume: null };
}

/** 볼륨 값에 맞는 스피커 아이콘. */
export function volumeIcon(volume: number): string {
  if (volume <= 0) return "🔇";
  if (volume < 34) return "🔈";
  if (volume < 67) return "🔉";
  return "🔊";
}

export interface Rect { x: number; y: number; w: number; h: number }

/**
 * 아이콘 기준 세로 볼륨 바 위치. 기본은 위로 펼치고(슬라이더는 보통 아이콘 위에 뜸),
 * 위쪽 공간이 부족하면 아래로, 결과는 항상 뷰포트 안으로 클램프한다.
 */
export function computeVolumeBarRect(anchor: Rect, size: { w: number; h: number }, viewport: { w: number; h: number }, gap = 8): Rect {
  const spaceAbove = anchor.y - gap;
  const spaceBelow = viewport.h - (anchor.y + anchor.h) - gap;
  const openBelow = spaceAbove < size.h && spaceBelow > spaceAbove;
  const y = openBelow ? anchor.y + anchor.h + gap : anchor.y - gap - size.h;
  const x = anchor.x + anchor.w / 2 - size.w / 2;
  const maxX = Math.max(gap, viewport.w - size.w - gap);
  const maxY = Math.max(gap, viewport.h - size.h - gap);
  return { x: Math.min(Math.max(gap, x), maxX), y: Math.min(Math.max(gap, y), maxY), w: size.w, h: size.h };
}
