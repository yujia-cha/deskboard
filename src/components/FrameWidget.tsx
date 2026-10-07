import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useSettings } from "../core/settings";
import { useConfig } from "../core/config";
import { NS, VERSION, dbwUrl, parseFrameMessage, readTheme } from "../core/frameBridge";
import type { WidgetProps } from "../core/widgetTypes";

/** ready 신호를 이만큼 기다린다. 넘으면 "응답 없음" 을 보여 주고 다시 불러올 수 있게 한다. */
const READY_TIMEOUT_MS = 5000;

/**
 * HTML 위젯 — 위젯 폴더의 `index.html` 을 격리된 iframe 에 띄운다.
 *
 * iframe 은 셸과 다른 출처(`http://dbw.localhost`, sandbox 로 불투명 출처)라 Tauri 를 직접
 * 부르지 못한다. 할 수 있는 일은 `core/frameBridge.ts` 의 허용 목록뿐이고, 여기가 그것을
 * 대신 수행한다. 위젯 쪽 API(`window.deskboard`)는 스킴이 HTML 에 자동으로 넣는 bridge.js 다.
 */
export function FrameWidget({ instanceId, settings, size, editing, widgetId, entry, version, subscribe }:
  WidgetProps & { widgetId: string; entry: string; version: string; subscribe: string[] }) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [ready, setReady] = useState(false);
  const [timedOut, setTimedOut] = useState(false);
  const [reload, setReload] = useState(0);
  const [lastError, setLastError] = useState<string | null>(null);
  const userCss = useConfig((c) => c.files.find((f) => f.file === "user.css"));
  const theme = useSettings((s) => [s.palette, s.themeMode, s.cardStyle, s.accent, s.cornerRadius].join("|"));

  const post = (msg: Record<string, unknown>) =>
    ref.current?.contentWindow?.postMessage({ ns: NS, v: VERSION, ...msg }, "*");

  // 최신 props 는 ref 로 — 메시지 핸들러를 다시 걸지 않고도 init 에 현재 값을 실어 보낸다.
  const props = { settings, size, editing };
  const latest = useRef(props);
  latest.current = props;

  useEffect(() => {
    setReady(false);
    setTimedOut(false);
    const timer = window.setTimeout(() => setTimedOut(true), READY_TIMEOUT_MS);
    const subs: UnlistenFn[] = [];
    const onMessage = async (e: MessageEvent) => {
      if (!ref.current || e.source !== ref.current.contentWindow) return; // 다른 iframe·창의 메시지는 무시
      const m = parseFrameMessage(e.data, subscribe);
      if (!m) return;
      switch (m.type) {
        case "ready":
          window.clearTimeout(timer);
          setReady(true);
          setTimedOut(false);
          post({ type: "init", instanceId, widgetId, props: latest.current, theme: readTheme() });
          invoke("widget_command_last", { instanceId }).then((out) => { if (out) post({ type: "command", output: out }); }).catch(() => {});
          return;
        case "error":
          setLastError(m.message);
          console.warn(`위젯 ${widgetId}:`, m.message);
          return;
        case "focus":
          // 셸의 IME 판단(App.tsx 의 editable)이 iframe 안 입력란을 알 수 있게 표시해 둔다.
          if (ref.current) ref.current.dataset.editable = m.editable ? "1" : "0";
          ref.current?.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
          return;
        case "call": {
          const reply = (ok: boolean, value: unknown) =>
            post({ type: "reply", id: m.id, ok, ...(ok ? { result: value } : { error: String(value) }) });
          try {
            const a = m.args;
            switch (m.method) {
              case "storage.get": return reply(true, await invoke("widget_storage_get", { instanceId }));
              case "storage.set": return reply(true, await invoke("widget_storage_set", { instanceId, value: a[0] ?? null }));
              case "command.runNow": return reply(true, await invoke("widget_command_run_now", { instanceId }));
              case "command.last": return reply(true, await invoke("widget_command_last", { instanceId }));
              case "settings.update":
                useSettings.getState().updateWidgetSettings(instanceId, a[0] as Record<string, unknown>);
                return reply(true, null);
              case "openUrl": return reply(true, await openUrl(a[0] as string));
              case "events.subscribe": {
                const name = a[0] as string;
                subs.push(await listen(name, (ev) => post({ type: "event", name, payload: ev.payload })));
                return reply(true, null);
              }
            }
          } catch (err) {
            reply(false, err);
          }
        }
      }
    };
    window.addEventListener("message", onMessage);
    let unOut: UnlistenFn | undefined;
    let cancelled = false;
    listen(`widget://output/${instanceId}`, (ev) => post({ type: "command", output: ev.payload }))
      .then((f) => { if (cancelled) f(); else unOut = f; });
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.removeEventListener("message", onMessage);
      unOut?.();
      subs.forEach((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId, widgetId, version, reload, subscribe.join(",")]);

  // 크기·설정·편집 상태가 바뀌면 알려 준다.
  useEffect(() => { if (ready) post({ type: "props", props }); },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ready, settings, size.w, size.h, editing]);

  // 테마(팔레트·강조색·user.css)가 바뀌면 토큰을 다시 보낸다. applyTheme 이 끝난 다음 프레임에 읽는다.
  useEffect(() => {
    if (!ready) return;
    const raf = requestAnimationFrame(() => post({ type: "theme", theme: readTheme() }));
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, theme, userCss]);

  const src = `${dbwUrl(widgetId, entry)}?v=${encodeURIComponent(version)}-${reload}`;
  return (
    <div className="frame-widget">
      <iframe
        ref={ref}
        key={`${version}-${reload}`}
        src={src}
        title={widgetId}
        sandbox="allow-scripts allow-forms"
        // 편집 모드에서는 프레임이 포인터를 먹지 않아야 카드를 끌 수 있다
        style={{ pointerEvents: editing ? "none" : "auto" }}
      />
      {ready && lastError && (
        <button className="frame-widget-error" title={lastError} onClick={() => setLastError(null)}>⚠ {lastError.split("\n")[0]}</button>
      )}
      {timedOut && !ready && (
        <div className="frame-widget-overlay">
          <span>응답이 없습니다{lastError ? ` — ${lastError}` : ""}</span>
          <button onClick={() => setReload((n) => n + 1)}>다시 불러오기</button>
        </div>
      )}
    </div>
  );
}
