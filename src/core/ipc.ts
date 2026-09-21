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

/**
 * 이벤트 구독. 페이로드가 필요 없으면 무시하면 된다 (트레이 메뉴 등).
 *
 * `useProviderEvent` 와 달리 값을 들고 있지 않는다 — 커맨드 결과와 이벤트를 **한 상태에**
 * 모으고 싶을 때 쓴다. 둘을 따로 들고 `이벤트 ?? 커맨드결과` 로 고르면, 한 번 온 이벤트가
 * 그 뒤의 커맨드 결과를 영영 가려 버린다.
 */
export function useEvent<T = unknown>(name: string, handler: (payload: T) => void) {
  useEffect(() => {
    let un: UnlistenFn | undefined;
    let cancelled = false;
    listen<T>(name, (e) => handler(e.payload)).then((f) => { if (cancelled) f(); else un = f; });
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

/**
 * 커맨드를 **보낸 순서대로** 도착시킨다.
 *
 * `invoke` 는 도착 순서를 보장하지 않는다. 위젯이 백엔드를 켜고 끄는 토글
 * (`*_set_active` 류)에서 이게 문제가 된다 — StrictMode 는 effect 를
 * 마운트 → 언마운트 → 마운트 로 돌리므로 `true` → `false` → `true` 가 연달아 나가는데,
 * `false` 가 마지막에 도착하면 기능이 꺼진 채로 남는다. 실제로 활동 추적이 이렇게 죽었다.
 *
 * `args` 에 함수를 주면 **보내기 직전에** 계산한다 — 큐에 쌓인 사이 상태가 바뀌었어도
 * 마지막 호출이 현재 상태를 전한다.
 */
let chain: Promise<unknown> = Promise.resolve();
export function invokeInOrder(
  cmd: string,
  args: Record<string, unknown> | (() => Record<string, unknown>) = {},
): Promise<unknown> {
  chain = chain
    .then(() => invoke(cmd, typeof args === "function" ? args() : args))
    .catch((e) => console.warn(cmd, e));
  return chain;
}

export const call = invoke;
