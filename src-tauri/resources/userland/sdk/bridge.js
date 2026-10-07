/*
 * deskboard HTML 위젯 브리지 — 위젯의 index.html 에 자동으로 들어간다 (직접 넣을 필요 없음).
 *
 * 위젯은 격리된 iframe 안에서 돈다. 대시보드와는 아래 window.deskboard 로만 이야기한다.
 *
 *   deskboard.onProps(({ settings, size, editing }) => …)   설정·크기가 바뀔 때마다 (처음에도 한 번)
 *   deskboard.onTheme(theme => …)                           테마가 바뀔 때 (CSS 변수는 자동 적용됨)
 *   deskboard.onCommand(out => …)                           widget.json 의 command 결과 { ok, text, data, error, at }
 *   deskboard.runCommand()                                  command 를 지금 한 번 실행
 *   await deskboard.storage.get() / deskboard.storage.set(v) 이 인스턴스만의 저장소 (JSON, 256KB)
 *   deskboard.subscribe("sysmon://update", payload => …)    widget.json 의 subscribe 에 적은 이벤트만
 *   deskboard.updateSettings({ key: value })                설정 저장
 *   deskboard.openUrl("https://…")                          기본 브라우저로 열기 (<a href> 클릭도 자동으로 이렇게 열린다)
 *
 * 색·글꼴은 대시보드 테마의 CSS 변수(--text, --accent, --font …)를 그대로 쓰면 된다.
 */
(() => {
  "use strict";
  if (window.deskboard) return;
  const NS = "deskboard", V = 1;
  const send = (m) => window.parent.postMessage(Object.assign({ ns: NS, v: V }, m), "*");

  const state = { instanceId: null, widgetId: null, props: null, theme: null, command: null };
  const listeners = { props: new Set(), theme: new Set(), command: new Set() };
  const events = new Map(); // name → Set<cb>
  const pending = new Map(); // id → { resolve, reject }
  let seq = 0;
  let resolveReady;
  const ready = new Promise((r) => { resolveReady = r; });

  const call = (method, ...args) => new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    send({ type: "call", id, method, args });
  });

  const fire = (kind, value) => {
    for (const cb of listeners[kind]) {
      try { cb(value); } catch (e) { report(e); }
    }
  };
  const on = (kind) => (cb) => {
    listeners[kind].add(cb);
    if (state[kind] != null) { try { cb(state[kind]); } catch (e) { report(e); } }
    return () => listeners[kind].delete(cb);
  };

  function applyTheme(theme) {
    if (!theme) return;
    const root = document.documentElement;
    for (const [k, v] of Object.entries(theme.vars || {})) root.style.setProperty(k, v);
    const ds = theme.dataset || {};
    if (ds.palette) root.dataset.palette = ds.palette;
    if (ds.themeMode) root.dataset.themeMode = ds.themeMode;
    if (ds.cardStyle) root.dataset.cardStyle = ds.cardStyle;
    root.style.colorScheme = ds.palette === "light" ? "light" : "dark";
  }

  window.addEventListener("message", (e) => {
    if (e.source !== window.parent) return;
    const m = e.data;
    if (!m || m.ns !== NS) return;
    switch (m.type) {
      case "init":
        state.instanceId = m.instanceId;
        state.widgetId = m.widgetId;
        state.props = m.props;
        state.theme = m.theme;
        applyTheme(m.theme);
        fire("theme", m.theme);
        fire("props", m.props);
        resolveReady();
        break;
      case "props": state.props = m.props; fire("props", m.props); break;
      case "theme": state.theme = m.theme; applyTheme(m.theme); fire("theme", m.theme); break;
      case "command": state.command = m.output; fire("command", m.output); break;
      case "event": {
        const set = events.get(m.name);
        if (set) for (const cb of set) { try { cb(m.payload); } catch (err) { report(err); } }
        break;
      }
      case "reply": {
        const p = pending.get(m.id);
        if (!p) break;
        pending.delete(m.id);
        if (m.ok) p.resolve(m.result); else p.reject(new Error(m.error));
        break;
      }
    }
  });

  window.deskboard = {
    ready,
    get instanceId() { return state.instanceId; },
    get widgetId() { return state.widgetId; },
    get settings() { return state.props ? state.props.settings : null; },
    get size() { return state.props ? state.props.size : null; },
    get editing() { return state.props ? state.props.editing : false; },
    onProps: on("props"),
    onTheme: on("theme"),
    onCommand: on("command"),
    runCommand: () => call("command.runNow"),
    lastCommand: () => call("command.last"),
    storage: {
      get: () => call("storage.get"),
      set: (value) => call("storage.set", value === undefined ? null : value),
    },
    subscribe(name, cb) {
      let set = events.get(name);
      if (!set) {
        set = new Set();
        events.set(name, set);
        call("events.subscribe", name).catch((e) => {
          events.delete(name);
          report(new Error(`이벤트 "${name}" 를 받을 수 없습니다 — widget.json 의 subscribe 에 적었나요? (${e.message})`));
        });
      }
      set.add(cb);
      return () => set.delete(cb);
    },
    updateSettings: (patch) => call("settings.update", patch),
    openUrl: (url) => call("openUrl", String(url)),
  };

  // --- 오류는 대시보드로 알려 준다 (위젯 카드에 표시) ---
  function report(e) {
    const msg = e && e.stack ? String(e.stack) : String(e && e.message ? e.message : e);
    send({ type: "error", message: msg });
    console.error(e);
  }
  window.addEventListener("error", (e) => report(e.error || e.message));
  window.addEventListener("unhandledrejection", (e) => report(e.reason));

  // --- 입력란 포커스 → 대시보드가 한/영(IME) 판단에 쓴다 ---
  const editable = (el) => !!el && (el.matches("input, textarea, select") || el.isContentEditable);
  const reportFocus = () => setTimeout(() => send({ type: "focus", editable: editable(document.activeElement) }), 0);
  document.addEventListener("focusin", reportFocus);
  document.addEventListener("focusout", reportFocus);

  // --- 링크는 iframe 안에서 열지 않고 기본 브라우저로 ---
  document.addEventListener("click", (e) => {
    const a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    if (!a) return;
    const href = a.href;
    if (/^https?:\/\//i.test(href) && !href.startsWith(location.origin)) {
      e.preventDefault();
      window.deskboard.openUrl(href).catch(report);
    }
  }, true);

  // --- 기본 모양: 투명 배경 + 테마 글꼴·색 (위젯 CSS 가 덮어쓸 수 있도록 가장 앞에) ---
  const base = document.createElement("style");
  base.textContent =
    "html,body{margin:0;background:transparent;color:var(--text);font-family:var(--font);" +
    "font-size:var(--fs-label);line-height:1.4;overflow:hidden;height:100%}" +
    "a{color:var(--accent)}button,input,select,textarea{font:inherit;color:inherit}";
  (document.head || document.documentElement).prepend(base);

  send({ type: "ready" });
})();
