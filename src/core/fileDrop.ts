import { useEffect, useRef } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Rect } from "./settings";

interface Handler {
  rectGetter: () => Rect | null;
  onHover: (hovering: boolean) => void;
  onDrop: (paths: string[]) => void;
}

const handlers = new Map<string, Handler>();
let started = false;

const inside = (p: { x: number; y: number }, r: Rect) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;

/** `getCurrentWebview().onDragDropEvent` 를 한 곳에서만 구독한다 (위젯마다 구독하면 중복). */
async function ensureStarted() {
  if (started) return;
  started = true;
  const webview = getCurrentWebview();
  await webview.onDragDropEvent(async (event) => {
    const p = event.payload;
    if (p.type === "over" || p.type === "drop") {
      // 창을 다른 모니터로 옮기는 등 배율이 바뀔 수 있으므로 매 이벤트마다 새로 읽는다.
      const scaleFactor = await getCurrentWindow().scaleFactor().catch(() => 1);
      const point = { x: p.position.x / scaleFactor, y: p.position.y / scaleFactor };
      for (const h of handlers.values()) {
        const r = h.rectGetter();
        const hit = !!r && inside(point, r);
        h.onHover(hit);
        if (p.type === "drop" && hit) h.onDrop(p.paths);
      }
    } else {
      for (const h of handlers.values()) h.onHover(false);
    }
  });
}

/**
 * 인스턴스별 드롭 대상 등록. `rectGetter` 는 창 좌표계(logical px) 사각형을 돌려준다 (없으면 null).
 * 전역 구독은 한 번만 시작되고, 이후 등록된 핸들러들로 분배된다.
 */
export function useFileDrop(id: string, rectGetter: () => Rect | null, handler: { onHover: (hovering: boolean) => void; onDrop: (paths: string[]) => void }) {
  const ref = useRef(handler);
  ref.current = handler;
  const rectRef = useRef(rectGetter);
  rectRef.current = rectGetter;
  useEffect(() => {
    ensureStarted().catch(console.warn);
    handlers.set(id, {
      rectGetter: () => rectRef.current(),
      onHover: (v) => ref.current.onHover(v),
      onDrop: (paths) => ref.current.onDrop(paths),
    });
    return () => { handlers.delete(id); };
  }, [id]);
}
