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

/** 초기값은 커맨드로 가져오고 이후 이벤트로 갱신한다 (위젯이 늦게 마운트돼도 즉시 표시). */
export function useProviderData<T>(eventName: string, command: string): T | null {
  const [value, setValue] = useState<T | null>(null);
  useEffect(() => {
    let un: UnlistenFn | undefined;
    let cancelled = false;
    let gotEvent = false;
    invoke<T>(command).then((v) => { if (!cancelled && !gotEvent) setValue(v); }).catch(console.warn);
    listen<T>(eventName, (e) => { gotEvent = true; setValue(e.payload); }).then((f) => { if (cancelled) f(); else un = f; });
    return () => { cancelled = true; un?.(); };
  }, [eventName, command]);
  return value;
}

export const call = invoke;
