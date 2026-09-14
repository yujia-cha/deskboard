import type { PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettings } from "../core/settings";
import { widgetById } from "../widgets/registry";
import { WidgetFrame } from "./WidgetFrame";

/** 모든 위젯 인스턴스를 절대 좌표로 배치. 편집 모드에서 빈 곳 드래그 = 창 이동. */
export function Canvas() {
  const instances = useSettings((s) => s.instances);
  const locked = useSettings((s) => s.locked);

  const onBackgroundDown = (e: PointerEvent) => {
    if (locked || e.target !== e.currentTarget || e.button !== 0) return;
    getCurrentWindow().startDragging().catch(console.warn);
  };

  return (
    <div className={`canvas ${locked ? "" : "editing"}`} onPointerDown={onBackgroundDown}>
      {instances.map((inst) => {
        const def = widgetById(inst.widgetId);
        if (!def) return null;
        const C = def.component;
        return (
          <WidgetFrame key={inst.id} inst={inst}>
            {(inner) => <C instanceId={inst.id} settings={inst.settings} size={inner} editing={!locked} />}
          </WidgetFrame>
        );
      })}
    </div>
  );
}
