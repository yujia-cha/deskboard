import { Component, useEffect, useMemo, type ErrorInfo, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { invokeInOrder } from "../core/ipc";
import { useRegistry, useWidgetDef } from "../core/widgetRegistry";
import { useSettings, type WidgetInstance } from "../core/settings";
import { defaultsOf, type WidgetDefinition } from "../core/widgetTypes";
import { WidgetContext, type WidgetContextValue } from "../sdk/context";
import "./WidgetHost.css";

/**
 * 위젯 인스턴스 하나를 그린다 — 정의를 찾고, 오류를 가두고, SDK 컨텍스트를 채운다.
 *
 * 위젯 코드는 사용자가 손으로 쓴 것일 수 있다. 하나가 렌더 중에 던져도 **그 카드만** 오류
 * 카드로 바뀌고 나머지 대시보드는 그대로다. 파일을 고쳐 지문(version)이 바뀌면 다시 시도한다.
 * (무한 루프처럼 메인 스레드를 붙잡는 코드는 여기서 막을 수 없다.)
 */
export function WidgetHost({ inst, size, editing }: { inst: WidgetInstance; size: { w: number; h: number }; editing: boolean }) {
  const def = useWidgetDef(inst.widgetId);
  const entry = useRegistry((s) => s.entries[inst.widgetId]);

  if (!def) {
    if (entry?.status === "loading") return <div className="widget-fallback"><p className="dim">불러오는 중…</p></div>;
    if (entry?.status === "error") return <WidgetError widgetId={inst.widgetId} message={entry.error ?? "알 수 없는 오류"} />;
    return <MissingWidget inst={inst} />;
  }
  return (
    <ErrorBoundary key={def.version ?? def.id} widgetId={def.id} resetKey={inst.settings}>
      <Mounted def={def} inst={inst} size={size} editing={editing} />
    </ErrorBoundary>
  );
}

function Mounted({ def, inst, size, editing }: { def: WidgetDefinition; inst: WidgetInstance; size: { w: number; h: number }; editing: boolean }) {
  // 정의를 늦게 불러온 경우(폴더가 나중에 생김)에도 스키마 기본값을 받게 한다.
  const settings = useMemo(() => ({ ...defaultsOf(def.settingsSchema), ...inst.settings }), [def.settingsSchema, inst.settings]);
  const ctx = useMemo<WidgetContextValue>(
    () => ({ widgetId: def.id, instanceId: inst.id, settings, size, editing }),
    [def.id, inst.id, settings, size, editing],
  );
  useCommandLifecycle(def, inst.id, settings);
  const C = def.component;
  return (
    <WidgetContext.Provider value={ctx}>
      <C instanceId={inst.id} settings={settings} size={size} editing={editing} />
    </WidgetContext.Provider>
  );
}

/**
 * `widget.json` 의 `command` 는 **셸이** 인스턴스마다 켜고 끈다 — 위젯은 결과만 읽는다(`useCommand`).
 * 켜고 끄기는 `invokeInOrder` 로: StrictMode 의 마운트→언마운트→마운트에서 끄기가 마지막에 도착하면
 * 실행이 꺼진 채로 남는다.
 */
function useCommandLifecycle(def: WidgetDefinition, instanceId: string, settings: Record<string, unknown>) {
  const key = def.hasCommand ? JSON.stringify(settings) : "";
  useEffect(() => {
    if (!def.hasCommand) return;
    invokeInOrder("widget_command_start", { instanceId, widgetId: def.id, settings: JSON.parse(key) });
    return () => { invokeInOrder("widget_command_stop", { instanceId }); };
  }, [def.hasCommand, def.id, def.version, instanceId, key]);
}

class ErrorBoundary extends Component<{ widgetId: string; resetKey: unknown; children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) { return { error }; }
  componentDidCatch(error: Error, info: ErrorInfo) { console.error(`위젯 ${this.props.widgetId}`, error, info.componentStack); }
  componentDidUpdate(prev: { resetKey: unknown }) {
    // 설정을 바꾸면 한 번 더 기회를 준다 (설정값 때문에 던졌을 수 있다)
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }
  render() {
    if (this.state.error)
      return <WidgetError widgetId={this.props.widgetId} message={describe(this.state.error)} onRetry={() => this.setState({ error: null })} />;
    return this.props.children;
  }
}

/** 스택에서 위젯 파일 위치(`dbw://<id>/<file>:<line>`)를 찾아 메시지에 붙인다. */
function describe(e: Error): string {
  const at = /dbw:\/\/[^/\s]+\/([^\s)]+?):(\d+)(?::\d+)?/.exec(e.stack ?? "");
  return at ? `${e.message}\n(${at[1]}:${at[2]})` : e.message;
}

function WidgetError({ widgetId, message, onRetry }: { widgetId: string; message: string; onRetry?: () => void }) {
  const entry = useRegistry((s) => s.entries[widgetId]);
  const userCopy = entry?.info?.root === "user";
  return (
    <div className="widget-fallback error" title={message}>
      <strong>⚠ {entry?.info?.manifest?.title ?? widgetId}</strong>
      <pre>{message}</pre>
      <span className="widget-fallback-actions">
        {onRetry && <button onClick={onRetry}>다시 시도</button>}
        <button onClick={() => invoke("widgets_open_dir", { id: widgetId }).catch(console.warn)}>폴더 열기</button>
        {userCopy && entry?.info?.overridesBuiltin && (
          <button title="사용자 폴더 이름 앞에 _ 를 붙여 끄고 내장 위젯으로 돌아갑니다"
            onClick={() => invoke("widgets_disable", { id: widgetId }).catch((e) => alert(String(e)))}>내장본으로 되돌리기</button>
        )}
      </span>
    </div>
  );
}

/** 정의가 없는 인스턴스 — 버리지 않고 남겨 둔다 (폴더가 돌아오면 같은 설정으로 다시 뜬다). */
function MissingWidget({ inst }: { inst: WidgetInstance }) {
  const remove = useSettings((s) => s.removeWidget);
  const locked = useSettings((s) => s.locked);
  return (
    <div className="widget-fallback">
      <strong>위젯 "{inst.widgetId}" 를 찾을 수 없습니다</strong>
      <p className="dim">widgets 폴더에서 사라졌거나 이름이 바뀌었습니다. 되돌리면 같은 설정으로 다시 뜹니다.</p>
      <span className="widget-fallback-actions">
        <button onClick={() => invoke("widgets_open_dir", { id: null }).catch(console.warn)}>위젯 폴더 열기</button>
        {!locked && <button onClick={() => remove(inst.id)}>이 카드 지우기</button>}
      </span>
    </div>
  );
}
