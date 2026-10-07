import type { WidgetSettings } from "deskboard";

export { Clock as default } from "./Clock";

/** 옛 저장값 옮기기 — 사용자가 직접 고르지 않은 옛 기본값만 바꾼다. */
export function migrate(saved: WidgetSettings): WidgetSettings {
  const out = { ...saved };
  // v1 기본값(dots + long)은 블록 스타일 도입 전 값 → 새 기본으로
  if (!("blockColor" in out)) {
    if (out.digitStyle === "dots") out.digitStyle = "blocks";
    if (out.dateFormat === "long") out.dateFormat = "mono";
  }
  // 블록 색 옛 하드코딩 기본값("#4fd1c5") → 빈 값(전역/위젯 강조색 사용)
  if (out.blockColor === "#4fd1c5") out.blockColor = "";
  return out;
}
