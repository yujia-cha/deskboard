import type { WidgetSettings } from "deskboard";

/** 옛 저장값 옮기기. */
export function migrate(saved: WidgetSettings, instanceId: string): WidgetSettings {
  const out = { ...saved };
  // folder: v1 은 경로 칸 하나였다 ("비우면 자동 생성"). 이제 두 모드가 나뉘었으므로 저장된 경로가 무엇이었는지로 가른다 — 전용 폴더의 경로는 `.../folders/<인스턴스 id>` 로 끝난다. 그 경우 `dir` 은 지운다. 남겨 두면 다른 PC 에서 남의 계정 경로를 연결하려 든다.
  if (!("source" in out)) {
    const dir = String(out.dir ?? "").trim();
    const managed = !dir || (!!instanceId && dir.replace(/\\/g, "/").toLowerCase().endsWith(`/folders/${instanceId.toLowerCase()}`));
    out.source = managed ? "managed" : "link";
    if (managed) out.dir = "";
  }
  return out;
}
