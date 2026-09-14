import { useSettings } from "../core/settings";
import { widgetById } from "../widgets/registry";
import { WidgetFrame } from "./WidgetFrame";

/** 모든 위젯 인스턴스를 절대 좌표(작업영역 px)로 배치. 창이 작업영역 전체를 덮는다. */
export function Canvas() {
  const instances = useSettings((s) => s.instances);
  const locked = useSettings((s) => s.locked);

  return (
    <div className={`canvas ${locked ? "" : "editing"}`}>
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
