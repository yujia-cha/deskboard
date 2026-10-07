/**
 * 위젯 SDK — 위젯 코드에서 `import { … } from "deskboard"` 로 쓰는 것 전부.
 *
 * 내장 위젯과 사용자 위젯이 **같은 것**을 쓴다. 위젯은 이 모듈과 자기 폴더 안의 파일만 import 한다
 * (그 밖에 쓸 수 있는 것은 `react` 와 `date-fns` 뿐 — `core/loader/host.ts`).
 * 타입 검사는 `tsconfig.json` 의 `paths` 가 이 파일을 가리켜서 이뤄진다.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl as openerOpenUrl } from "@tauri-apps/plugin-opener";
import { open as dialogOpen } from "@tauri-apps/plugin-dialog";
import { useSettings, type Rect } from "../core/settings";
import { invokeInOrder, useEvent, useProviderData, useProviderEvent } from "../core/ipc";
import { useWidgetContext, type WidgetContextValue } from "./context";
import { dbwUrl } from "../core/frameBridge";

export type { SettingField, WidgetProps, WidgetSettings } from "../core/widgetTypes";
export type { Rect } from "../core/settings";
export type { Slice } from "../components/Donut";
export type { ContextMenuItem } from "../components/ContextMenu";

// --- 공용 컴포넌트 ---
export { Gauge } from "../components/Gauge";
export { Sparkline } from "../components/Sparkline";
export { Donut } from "../components/Donut";
export { ContextMenu } from "../components/ContextMenu";

// --- 백엔드 호출·이벤트 ---
/** Tauri 커맨드 호출 (= `invoke`). */
export const call = invoke;
export { invokeInOrder, useEvent, useProviderData, useProviderEvent };

// --- 셸과 주고받기 ---
export { useDismiss } from "../core/dismiss";
export { useFileDrop } from "../core/fileDrop";

/** 이 위젯의 현재 정보 (`instanceId`, `settings`, `size`, `editing`, `widgetId`). */
export function useWidget(): WidgetContextValue {
  return useWidgetContext();
}

/** 인스턴스 설정 일부를 바꾸고 저장한다 (설정 패널을 거치지 않는 위젯 안 조작용). */
export function updateWidgetSettings(instanceId: string, patch: Record<string, unknown>) {
  useSettings.getState().updateWidgetSettings(instanceId, patch);
}

/** 위젯 사각형 밖으로 펼쳐지는 팝업의 히트 영역을 등록한다 (null = 해제). 안 하면 클릭이 바탕화면으로 샌다. */
export function setOverlayRect(id: string, rect: Rect | null) {
  useSettings.getState().setOverlayRect(id, rect);
}

/** 설정 패널을 연다. 인스턴스 id 를 주면 그 위젯의 설정으로. */
export function openSettings(instanceId: string | null = null) {
  useSettings.getState().openSettings(instanceId);
}

/** 이 인스턴스에만 지정된 강조색 (없으면 undefined — 그때는 `var(--accent)` 가 전역 색이다). */
export function useInstanceAccent(instanceId: string): string | undefined {
  return useSettings((s) => s.instances.find((i) => i.id === instanceId)?.accent);
}

/** 편집 모드(잠금 해제) 여부. */
export function useEditing(): boolean {
  return useSettings((s) => !s.locked);
}

/** 기본 브라우저로 연다. http/https 만. */
export function openUrl(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) return Promise.reject(new Error(`열 수 없는 주소입니다: ${url}`));
  return openerOpenUrl(url);
}

/** 파일·폴더 고르기 창 (`@tauri-apps/plugin-dialog` 의 `open`). */
export const openDialog = dialogOpen;

/** 위젯 폴더 안 파일의 URL (이미지 등). 예: `<img src={asset(widgetId, "icon.png")} />` */
export function asset(widgetId: string, relPath: string): string {
  return dbwUrl(widgetId, relPath);
}

// --- 명령 실행 (`widget.json` 의 `command`) ---

export interface CommandOutput {
  ok: boolean;
  /** 표준 출력 그대로 */
  text: string;
  /** parse 가 json/lines 일 때의 결과 */
  data: unknown;
  error: string | null;
  exitCode: number | null;
  /** 끝난 시각 (unix ms) */
  at: number;
  /** 걸린 시간 (ms) */
  ms: number;
}

/**
 * 이 위젯의 `command` 마지막 결과. 실행 주기·시작·정지는 셸이 맡는다 — 위젯은 읽기만 한다.
 * 아직 한 번도 끝나지 않았으면 null.
 */
export function useCommand(): CommandOutput | null {
  const { instanceId } = useWidgetContext();
  const [out, setOut] = useState<CommandOutput | null>(null);
  useEffect(() => {
    let cancelled = false, got = false;
    let un: UnlistenFn | undefined;
    invoke<CommandOutput | null>("widget_command_last", { instanceId })
      .then((v) => { if (!cancelled && !got && v) setOut(v); })
      .catch(() => {});
    listen<CommandOutput>(`widget://output/${instanceId}`, (e) => { got = true; setOut(e.payload); })
      .then((f) => { if (cancelled) f(); else un = f; });
    return () => { cancelled = true; un?.(); };
  }, [instanceId]);
  return out;
}

/** 주기를 기다리지 않고 지금 한 번 실행한다. */
export function runCommand(instanceId: string): Promise<void> {
  return invoke("widget_command_run_now", { instanceId });
}

// --- 인스턴스 저장소 ---

/**
 * 이 인스턴스만의 작은 영속 저장소 (JSON, 256KB 까지). `useState` 처럼 쓴다.
 * 저장은 300ms 모아서 한 번. 처음 읽기 전에는 `initial` 을 돌려준다.
 */
export function useStorage<T>(initial: T): [T, (next: T | ((prev: T) => T)) => void, boolean] {
  const { instanceId } = useWidgetContext();
  const [value, setValue] = useState<T>(initial);
  const [loaded, setLoaded] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef(value);
  useEffect(() => {
    let cancelled = false;
    invoke<T | null>("widget_storage_get", { instanceId })
      .then((v) => { if (!cancelled) { if (v != null) { latest.current = v; setValue(v); } setLoaded(true); } })
      .catch(() => { if (!cancelled) setLoaded(true); });
    return () => {
      cancelled = true;
      // 떠나기 전에 모아 둔 저장을 바로 보낸다
      if (timer.current !== undefined) {
        window.clearTimeout(timer.current);
        invoke("widget_storage_set", { instanceId, value: latest.current }).catch(console.warn);
      }
    };
  }, [instanceId]);
  const set = useCallback((next: T | ((prev: T) => T)) => {
    setValue((prev) => {
      const v = typeof next === "function" ? (next as (p: T) => T)(prev) : next;
      latest.current = v;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        timer.current = undefined;
        invoke("widget_storage_set", { instanceId, value: latest.current }).catch(console.warn);
      }, 300);
      return v;
    });
  }, [instanceId]);
  return [value, set, loaded];
}
