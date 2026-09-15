import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 팝업·메뉴 닫기 훅. `active` 동안 다음 중 하나가 일어나면 `onDismiss` 를 호출한다.
 * - 창 안에서 `data-dismiss-scope` 에 `<scope>` 를 포함한 요소(공백 구분 여러 개 가능) 밖을 누름
 * - 히트 영역 밖(바탕화면·다른 앱)을 누름 → 백엔드 `hit://outside-press`
 * - Esc
 *
 * 창 `blur` 는 쓰지 않는다: always-on-bottom 창은 클릭 직후에도 포커스를 잃어 열리자마자 닫힌다.
 */
export function useDismiss(scope: string, active: boolean, onDismiss: () => void) {
  const ref = useRef(onDismiss);
  ref.current = onDismiss;
  useEffect(() => {
    if (!active) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Element | null;
      if (!t?.closest(`[data-dismiss-scope~="${CSS.escape(scope)}"]`)) ref.current();
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") ref.current(); };
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    let un: UnlistenFn | undefined;
    let cancelled = false;
    listen("hit://outside-press", () => ref.current()).then((f) => { if (cancelled) f(); else un = f; });
    return () => {
      cancelled = true;
      un?.();
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [scope, active]);
}
