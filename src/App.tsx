import { useCallback, useEffect } from "react";
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

  if (!loaded) return null;
  return (
    <>
      <Canvas />
      {!locked && (
        <div className="edit-bar">
          편집 모드 — 빈 곳 드래그: 창 이동 · Esc: 잠금
          <button onClick={() => openSettings(null)}>⚙ 설정</button>
          <button onClick={() => setLocked(true)}>🔒 잠금</button>
        </div>
      )}
      <SettingsPanel />
    </>
  );
}
