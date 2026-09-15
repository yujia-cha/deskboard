import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type PointerEvent as RPointerEvent } from "react";
import { createPortal } from "react-dom";
import { useSettings, type Rect } from "../../core/settings";
import { useDismiss } from "../../core/dismiss";
import { computeVolumeBarRect, volumeFromPointerY, volumeIcon } from "./volume";
import "./VolumeControl.css";

const POP_SIZE = { w: 36, h: 120 };
const CLOSE_DELAY_MS = 250;
const DRAG_DEBOUNCE_MS = 150;

/**
 * 스피커 아이콘 + 호버 시 뜨는 세로 볼륨 바.
 * WidgetFrame 의 overflow 에 갇히지 않도록 바는 document.body 로 portal 하고,
 * 위젯 밖으로 나가는 만큼 setOverlayRect 로 히트 영역을 등록한다(안 하면 클릭이 바탕화면으로 샌다).
 * 창 blur 는 always-on-bottom 창에서 클릭 직후에도 발생해 쓰지 않고, 호버 이탈(지연)+useDismiss 로 닫는다.
 */
export function VolumeControl({ instanceId, volume, accent, onVolumeChange, onToggleMute }: {
  instanceId: string;
  /** 서버 기준 현재 볼륨(0~100). 드래그 중에는 내부 표시값이 우선한다. */
  volume: number;
  accent?: string;
  /** 값이 바뀔 때마다 호출. final=false 는 드래그 중 디바운스된 중간값, true 는 놓았을 때 최종값. */
  onVolumeChange: (percent: number, final: boolean) => void;
  onToggleMute: () => void;
}) {
  const setOverlayRect = useSettings((s) => s.setOverlayRect);
  const [open, setOpen] = useState(false);
  const [display, setDisplay] = useState(volume);
  const [rect, setRect] = useState<Rect | null>(null);
  const iconRef = useRef<HTMLButtonElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const closeTimer = useRef<number | undefined>(undefined);
  const debounceTimer = useRef<number | undefined>(undefined);
  const popupId = `${instanceId}-volume`;

  useEffect(() => { if (!dragging.current) setDisplay(volume); }, [volume]);
  useEffect(() => () => { window.clearTimeout(closeTimer.current); window.clearTimeout(debounceTimer.current); }, []);

  const openNow = () => { window.clearTimeout(closeTimer.current); setOpen(true); };
  // 드래그 중에는 커서가 바 밖으로 나가도 닫지 않는다 — 닫히면 pointerup 을 잃어 드래그 상태가 풀리지 않는다.
  const scheduleClose = () => {
    window.clearTimeout(closeTimer.current);
    if (dragging.current) return;
    closeTimer.current = window.setTimeout(() => setOpen(false), CLOSE_DELAY_MS);
  };

  useDismiss(popupId, open, () => setOpen(false));

  useLayoutEffect(() => {
    if (!open) { setRect(null); return; }
    const el = iconRef.current;
    if (!el) return;
    const a = el.getBoundingClientRect();
    setRect(computeVolumeBarRect({ x: a.left, y: a.top, w: a.width, h: a.height }, POP_SIZE, { w: window.innerWidth, h: window.innerHeight }));
  }, [open]);

  useEffect(() => {
    if (!rect) { setOverlayRect(popupId, null); return; }
    setOverlayRect(popupId, rect);
    return () => setOverlayRect(popupId, null);
  }, [rect, popupId, setOverlayRect]);

  const commit = (percent: number, final: boolean) => {
    setDisplay(percent);
    window.clearTimeout(debounceTimer.current);
    if (final) { onVolumeChange(percent, true); return; }
    debounceTimer.current = window.setTimeout(() => onVolumeChange(percent, false), DRAG_DEBOUNCE_MS);
  };

  const fromEvent = (e: RPointerEvent) => {
    const r = trackRef.current!.getBoundingClientRect();
    return volumeFromPointerY(e.clientY, r.top, r.height);
  };
  const onTrackDown = (e: RPointerEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging.current = true;
    commit(fromEvent(e), false);
  };
  const onTrackMove = (e: RPointerEvent) => { if (dragging.current) commit(fromEvent(e), false); };
  const onTrackUp = (e: RPointerEvent) => {
    if (!dragging.current) return;
    dragging.current = false;
    commit(fromEvent(e), true);
    // 바 밖에서 놓았으면 호버 이탈과 같게 닫기 예약
    const pop = (e.currentTarget as HTMLElement).parentElement?.getBoundingClientRect();
    if (pop && !(e.clientX >= pop.left && e.clientX <= pop.right && e.clientY >= pop.top && e.clientY <= pop.bottom)) scheduleClose();
  };

  return (
    <>
      <button
        ref={iconRef}
        className="sp-icon sp-volume-icon"
        data-dismiss-scope={popupId}
        title={`볼륨 ${display}% — 클릭: 음소거`}
        onMouseEnter={openNow}
        onMouseLeave={scheduleClose}
        onClick={onToggleMute}
      >
        {volumeIcon(display)}
      </button>
      {open && rect && createPortal(
        <div
          data-dismiss-scope={popupId}
          className="sp-volume-pop"
          style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, ...(accent ? { "--accent": accent } as CSSProperties : {}) }}
          onPointerDown={(e) => e.stopPropagation()}
          onMouseEnter={openNow}
          onMouseLeave={scheduleClose}
        >
          <div
            ref={trackRef}
            className="sp-volume-track"
            onPointerDown={onTrackDown}
            onPointerMove={onTrackMove}
            onPointerUp={onTrackUp}
            onPointerCancel={onTrackUp}
          >
            <div className="sp-volume-fill" style={{ height: `${display}%` }} />
          </div>
          <div className="sp-volume-value dim">{display}%</div>
        </div>,
        document.body,
      )}
    </>
  );
}
