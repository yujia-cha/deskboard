import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { call, invokeInOrder, useEvent, useWidget } from "deskboard";
import type { MergeContent, MergeView } from "./types";
import { Board } from "./Board";
import { Orders } from "./Orders";
import { HEADER_H, ORDERS_H, computeLayout } from "./layout";
import { formatMMSS, useRegenCountdown } from "./regen";
import "./Merge.css";

const FLASH_MS = 2500;
const LOW_ENERGY = "에너지가 부족합니다";

export function Merge() {
  const { instanceId, size, editing } = useWidget();
  const [view, setView] = useState<MergeView | null>(null);
  const [content, setContent] = useState<MergeContent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [shake, setShake] = useState(0);
  const [hint, setHint] = useState(false);
  const [notice, setNotice] = useState(false);
  const flashTimer = useRef<number | undefined>(undefined);
  // 카드 여백(--wpad)을 뺀 실제 내용 상자. `size` 는 카드 크기라 그대로 쓰면 격자가 넘쳐 WidgetFrame 이 배율을 줄인다.
  // clientWidth/Height 는 CSS zoom 아래서도 배율 전 px 다.
  const rootRef = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<{ w: number; h: number } | null>(null);

  const reload = useCallback(
    () =>
      call<MergeView>("merge_state")
        .then((v) => { setView(v); setLoadError(null); })
        .catch((e) => setLoadError(String(e))),
    [],
  );
  const loadContent = useCallback(
    () => { call<MergeContent>("merge_content").then(setContent).catch((e) => setLoadError(String(e))); },
    [],
  );

  useEffect(() => { reload(); loadContent(); }, [reload, loadContent]);
  useEvent("merge://changed", reload);
  useEvent("merge://content", loadContent);

  // 이 위젯이 떠 있는 동안만 ⌨ 클리커가 돈다 (백엔드가 켜진 위젯 수를 센다).
  useEffect(() => {
    invokeInOrder("merge_widget_present", { instanceId, present: true });
    return () => { invokeInOrder("merge_widget_present", { instanceId, present: false }); };
  }, [instanceId]);

  useEffect(() => () => window.clearTimeout(flashTimer.current), []);

  const flash = useCallback((msg: string) => {
    setError(msg);
    if (msg.includes(LOW_ENERGY)) { setShake((n) => n + 1); setHint(true); }
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => { setError(null); setHint(false); }, FLASH_MS);
  }, []);

  /** 변경 커맨드 — 돌려받은 MergeView 로 바로 갱신한다. 오류는 한국어 문자열로 던져진다. */
  const act = useCallback(
    async (cmd: string, args: Record<string, unknown> = {}) => {
      try {
        setView(await call<MergeView>(cmd, args));
        return true;
      } catch (e) {
        flash(String(e));
        return false;
      }
    },
    [flash],
  );

  const left = useRegenCountdown(view, reload);
  const ready = view !== null;
  useLayoutEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const read = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ready]);
  const inner = box && box.w > 0 && box.h > 0 ? box : size;
  const layout = useMemo(() => computeLayout(inner), [inner.w, inner.h]); // eslint-disable-line react-hooks/exhaustive-deps

  const setKeyClicker = async (on: boolean) => {
    await invokeInOrder("merge_set_key_clicker", { on });
    await reload();
  };
  const toggleKey = () => {
    if (!view) return;
    if (view.keyClicker) setKeyClicker(false);
    else if (!view.keyNoticeSeen) setNotice(true);
    else setKeyClicker(true);
  };
  const confirmNotice = async () => {
    setNotice(false);
    await invokeInOrder("merge_tutorial", { step: "key_notice" });
    await setKeyClicker(true);
  };

  if (!view) {
    return (
      <div className="merge merge-loading">
        {loadError ? (
          <>
            <div className="merge-msg warn">{loadError}</div>
            <button className="merge-btn" onClick={() => { reload(); loadContent(); }}>다시 시도</button>
          </>
        ) : "불러오는 중…"}
      </div>
    );
  }

  return (
    <div ref={rootRef} className="merge">
      <div className="merge-head" style={{ height: HEADER_H }}>
        <span title={view.nextLevelAt !== null ? `다음 레벨까지 ⭐${view.nextLevelAt}` : "최고 레벨"}>🍀{view.level}</span>
        <span className="merge-energy-wrap">
          <span key={shake} className={`merge-energy${shake ? " shake" : ""}`}>⚡{view.energy}</span>
          {left !== null && <span className="merge-regen">(+1 {formatMMSS(left)})</span>}
        </span>
        <span title={view.nextLevelAt !== null ? `다음 레벨 ⭐${view.nextLevelAt}` : undefined}>⭐{view.stars}</span>
        <button
          className={`merge-btn key${view.keyClicker ? " on" : ""}`}
          title={view.keyClicker ? "키보드 클리커 켜짐" : "키보드 클리커 꺼짐"}
          aria-pressed={view.keyClicker}
          onClick={toggleKey}
        >⌨</button>
      </div>

      {(error || notice) && (
        <div className="merge-overlay" style={{ top: HEADER_H }}>
          {error && <div className="merge-msg warn">{error}</div>}
          {notice && (
            <div className="merge-notice">
              <span>키를 누른 횟수만 셉니다. 어떤 키를 눌렀는지는 기억·저장·전송하지 않습니다.</span>
              <span className="merge-notice-btns">
                <button className="merge-btn ok" onClick={confirmNotice}>켜기</button>
                <button className="merge-btn" onClick={() => setNotice(false)}>취소</button>
              </span>
            </div>
          )}
        </div>
      )}

      <div style={{ height: ORDERS_H }}>
        <Orders view={view} content={content} disabled={editing} act={act} />
      </div>
      <Board view={view} content={content} layout={layout} editing={editing} act={act} reload={reload} hint={hint} />
    </div>
  );
}
