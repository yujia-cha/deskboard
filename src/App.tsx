import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./core/settings";
import { invokeInOrder, useEvent } from "./core/ipc";
import { useWallpaper } from "./core/wallpaper";
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
  const blurStrength = useSettings((s) => s.blurStrength);
  const canvasMonitor = useSettings((s) => s.canvasMonitor);
  const setLocked = useSettings((s) => s.setLocked);
  const toggleTheme = useSettings((s) => s.toggleTheme);
  const openSettings = useSettings((s) => s.openSettings);

  // 웹뷰 안에서 입력 요소가 포커스를 쥐었는지 백엔드에 알린다.
  //
  // 시계·게이지처럼 칠 것이 없는 위젯을 눌렀을 때까지 대시보드가 키보드 포커스를 쥐고 있으면
  // Windows 의 IME 가 "사용하지 않음" 으로 넘어가 한/영 키가 갈 곳을 잃는다. 백엔드가 그때만
  // 원래 창에 포커스를 돌려줄 수 있게, 여기서 진짜 입력 여부를 알려 준다.
  useEffect(() => {
    const editable = (el: Element | null) =>
      !!el && (el.matches("input, textarea, select") || (el as HTMLElement).isContentEditable);
    // focusout 은 새 포커스가 정해지기 **전에** 오므로 한 틱 뒤에 읽는다.
    const report = () => setTimeout(
      () => invokeInOrder("ui_set_text_focus", { active: editable(document.activeElement) }), 0);
    document.addEventListener("focusin", report);
    document.addEventListener("focusout", report);
    return () => {
      document.removeEventListener("focusin", report);
      document.removeEventListener("focusout", report);
    };
  }, []);

  useEffect(() => { load(); }, [load]);

  // 카드 뒤에 깔 블러된 배경화면 (진짜 반투명)
  useWallpaper(loaded ? blurStrength : 0, canvasMonitor);

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
