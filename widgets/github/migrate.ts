import type { WidgetSettings } from "deskboard";

/** 옛 저장값 옮기기. */
export function migrate(saved: WidgetSettings): WidgetSettings {
  const out = { ...saved };
  // github: v1 의 "CI 상태"(showChecks)는 저장소 줄로 흡수됐다 — 꺼 뒀던 사람이 갑자기 목록을 보게 되지 않도록 그 뜻을 새 키로 옮긴다.
  if ("showChecks" in out && !("showRepos" in out)) out.showRepos = out.showChecks;
  return out;
}
