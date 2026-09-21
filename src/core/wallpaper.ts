import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invokeInOrder } from "./ipc";
import { useSettings } from "./settings";

export interface WallpaperSnapshot {
  /** `data:image/png;base64,...`. 배경화면을 못 읽으면 빈 문자열. */
  data_uri: string;
  /** Windows 가 실제로 그리는 크기 (채우기/맞춤이 이미 반영된 값). */
  draw_w: number;
  draw_h: number;
  /** 창 좌상단 기준 이미지 시작점. 채우기로 잘린 배경화면에서는 음수다. */
  ox: number;
  oy: number;
  repeat: boolean;
  /** "capture" = 화면에 그려진 바탕화면을 찍음 (라이브 배경화면 지원), "file" = 배경화면 파일. */
  source: "capture" | "file" | string;
  error: string | null;
}

/** 배경화면 레이어를 끈 상태 — 카드는 단색 표면만 그린다. */
function clear() {
  document.documentElement.style.setProperty("--wallpaper", "none");
  useSettings.getState().setWallpaperStatus(null, null);
}

function apply(wp: WallpaperSnapshot) {
  useSettings.getState().setWallpaperStatus(wp.source, wp.error);
  if (!wp.data_uri) {
    document.documentElement.style.setProperty("--wallpaper", "none");
    return;
  }
  const d = document.documentElement.style;
  d.setProperty("--wallpaper", `url("${wp.data_uri}")`);
  // 백엔드가 이미 Windows 의 배치(채우기/맞춤/늘이기/가운데/바둑판)와 작업영역 오프셋을
  // 반영해 창 좌표로 계산해 줬다. 여기서는 그대로 깔기만 하면 된다.
  // 축소된 블러본을 원래 크기로 늘려 쓰는 건 의도된 것이다 — 어차피 뭉갠 이미지다.
  d.setProperty("--wallpaper-w", `${wp.draw_w}px`);
  d.setProperty("--wallpaper-h", `${wp.draw_h}px`);
  d.setProperty("--wallpaper-ox", `${wp.ox}px`);
  d.setProperty("--wallpaper-oy", `${wp.oy}px`);
  d.setProperty("--wallpaper-repeat", wp.repeat ? "repeat" : "no-repeat");
}

/**
 * 블러된 배경화면을 CSS 변수로 올린다 (`--wallpaper`, `--wallpaper-w/h`, `--wallpaper-ox/oy`).
 * 각 카드는 자기 위치(`--wx/--wy`)만큼 배경을 밀어 자기 뒤 부분을 보여준다.
 *
 * `blurStrength` 가 0 이면 아무것도 하지 않고 **백엔드도 재운다** —
 * 켜져 있지 않은데 화면을 계속 캡처하면 그냥 낭비다.
 * `monitor` 가 바뀌면 크기·오프셋을 다시 구한다.
 */
export function useWallpaper(blurStrength: number, monitor: string | null) {
  useEffect(() => {
    if (blurStrength <= 0) {
      clear();
      // 백엔드 캡처 루프를 멈춘다
      invokeInOrder("wallpaper_set_active", { active: false });
      return;
    }
    let un: UnlistenFn | undefined;
    let cancelled = false;
    invoke<WallpaperSnapshot>("get_wallpaper", { passes: blurStrength })
      .then((wp) => { if (!cancelled) apply(wp); })
      .catch(console.warn);
    // 배경화면이 바뀌면 백엔드가 새 스냅샷을 보낸다 (캡처 2~10초, 파일 30초).
    // 바뀌지 않은 동안에는 아무것도 오지 않는다 — 백엔드가 지문으로 거른다.
    listen<WallpaperSnapshot>("wallpaper://update", (e) => apply(e.payload))
      .then((f) => { if (cancelled) f(); else un = f; })
      .catch(console.warn);
    return () => {
      cancelled = true;
      un?.();
      invokeInOrder("wallpaper_set_active", { active: false });
    };
    // 모니터가 바뀌면 크기·오프셋이 달라지므로 다시 받는다.
  }, [blurStrength, monitor]);
}
