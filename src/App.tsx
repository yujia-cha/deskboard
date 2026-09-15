import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./core/settings";
import { useEvent } from "./core/ipc";
import { Canvas } from "./components/Canvas";
import { SettingsPanel } from "./components/SettingsPanel";
import "./core/theme.css";
import "./App.css";

export default function App() {
  const load = useSettings((s) => s.load);
  const loaded = useSettings((s) => s.loaded);
  const locked = useSettings((s) => s.locked);
  const settingsOpen = useSettings((s) => s.settingsOpen);
  const instances = useSettings((s) => s.instances);
  const overlayRects = useSettings((s) => s.overlayRects);
  const setLocked = useSettings((s) => s.setLocked);
  const toggleTheme = useSettings((s) => s.toggleTheme);
  const openSettings = useSettings((s) => s.openSettings);

  useEffect(() => { load(); }, [load]);

  // 트레이 메뉴 → 프론트 상태
  useEvent("ui://toggle_lock", useCallback(() => setLocked(!useSettings.getState().locked), [setLocked]));
  useEvent("ui://toggle_theme", toggleTheme);
  useEvent("ui://settings", useCallback(() => openSettings(null), [openSettings]));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setLocked(true); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setLocked]);

  // 히트 영역: 잠금 상태에서 위젯 밖 클릭은 바탕화면(아이콘)으로 통과시킨다.
  useEffect(() => {
    if (!loaded) return;
    const rects = [
      ...instances.map((i) => ({ x: i.x, y: i.y, w: i.w, h: i.h })),
      ...Object.values(overlayRects),
    ];
    invoke("set_hit_regions", { rects, enabled: locked && !settingsOpen }).catch(console.warn);
  }, [loaded, instances, overlayRects, locked, settingsOpen]);

  if (!loaded) return null;
  return (
    <>
      <Canvas />
      {!locked && (
        <div className="edit-bar">
          편집 모드 — 헤더 드래그: 이동 · 모서리: 크기 · Esc: 잠금
          <button onClick={() => openSettings(null)}>⚙ 설정</button>
          <button onClick={() => setLocked(true)}>🔒 잠금</button>
        </div>
      )}
      <SettingsPanel />
    </>
  );
}
