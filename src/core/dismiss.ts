import { useEffect, useRef, useSyncExternalStore } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 지금 열려 있는 팝업 수. 팝업이 열린 동안에는 대시보드가 Esc 를 받아야 하므로
 * 백엔드가 전경을 돌려주지 않게 `App.tsx` 가 알린다 (`keyboardWanted`).
 * 위젯이 따로 알릴 필요 없이 `useDismiss` 를 쓰는 것만으로 세어진다.
 */
let openPopups = 0;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };

/** 열린 팝업 수를 구독한다. */
export function useOpenPopups(): number {
  return useSyncExternalStore(subscribe, () => openPopups);
}

/**
 * 팝업·메뉴 닫기 훅. `active` 동안 다음 중 하나가 일어나면 `onDismiss` 를 호출한다.
 * - 창 안에서 `data-dismiss-scope` 에 `<scope>` 를 포함한 요소(공백 구분 여러 개 가능) 밖을 누름
 * - 히트 영역 밖(바탕화면·다른 앱 — 위젯을 덮은 앱 창 포함)을 누름 → 백엔드 `hit://outside-press`
 * - Esc
 *
 * 창 `blur` 는 쓰지 않는다: always-on-bottom 창은 클릭 직후에도 포커스를 잃어 열리자마자 닫힌다.
 */
export function useDismiss(scope: string, active: boolean, onDismiss: () => void) {
  const ref = useRef(onDismiss);
  ref.current = onDismiss;
  useEffect(() => {
    if (!active) return;
    openPopups++;
    notify();
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
      openPopups--;
      notify();
    };
  }, [scope, active]);
}
