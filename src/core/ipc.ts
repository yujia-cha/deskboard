import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 백엔드 Provider 가 `<id>://update` 로 푸시하는 페이로드를 구독한다. */
export function useProviderEvent<T>(name: string, initial: T | null = null): T | null {
  const [value, setValue] = useState<T | null>(initial);
  useEffect(() => {
    let un: UnlistenFn | undefined;
    let cancelled = false;
    listen<T>(name, (e) => setValue(e.payload)).then((f) => {
      if (cancelled) f(); else un = f;
    });
    return () => { cancelled = true; un?.(); };
  }, [name]);
  return value;
}

/** 단발 이벤트 구독 (트레이 메뉴 등). */
export function useEvent(name: string, handler: () => void) {
  useEffect(() => {
    let un: UnlistenFn | undefined;
    let cancelled = false;
    listen(name, () => handler()).then((f) => { if (cancelled) f(); else un = f; });
    return () => { cancelled = true; un?.(); };
  }, [name, handler]);
}

export const call = invoke;
