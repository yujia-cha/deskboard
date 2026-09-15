import { createPortal } from "react-dom";
import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings, type Rect } from "../../core/settings";
import { useFileDrop } from "../../core/fileDrop";
import { ContextMenu } from "../../components/ContextMenu";
import { computePopupRect } from "./placement";
import type { FolderEntry, FolderSettings } from "./Folder";
import "./FolderPopup.css";

interface Props {
  instanceId: string;
  anchorEl: HTMLElement | null;
  dir: string;
  entries: FolderEntry[];
  settings: FolderSettings;
  onClose: () => void;
  onRefresh: () => void;
  /** 이 폴더 인스턴스의 강조색 덮어쓰기. body 로 portal 되므로 CSS 상속이 끊겨 직접 전달한다. */
  accent?: string;
}

/** 펼침 팝업 — WidgetFrame 의 overflow/zoom 에 갇히지 않도록 document.body 에 portal 로 그린다. */
export function FolderPopup({ instanceId, anchorEl, dir, entries, settings, onClose, onRefresh, accent }: Props) {
  const setOverlayRect = useSettings((s) => s.setOverlayRect);
  const [rect, setRect] = useState<Rect | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; entry: FolderEntry } | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const popupId = `${instanceId}-popup`;

  const contentSize = useMemo(() => {
    if (settings.layout === "list") {
      return { w: 200, h: Math.min(320, Math.max(48, 16 + entries.length * 32)) };
    }
    const cols = Math.max(2, Math.min(8, settings.columns || 4));
    const rows = Math.max(1, Math.ceil(entries.length / cols));
    return { w: cols * 72 + 16, h: Math.min(360, rows * 84 + 16) };
  }, [settings.layout, settings.columns, entries.length]);

  useEffect(() => {
    if (!anchorEl) return;
    const update = () => {
      const a = anchorEl.getBoundingClientRect();
      setRect(computePopupRect(
        { x: a.left, y: a.top, w: a.width, h: a.height },
        contentSize,
        { w: window.innerWidth, h: window.innerHeight },
      ));
    };
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [anchorEl, contentSize]);

  useEffect(() => {
    if (!rect) return;
    setOverlayRect(popupId, rect);
    return () => setOverlayRect(popupId, null);
  }, [rect, popupId, setOverlayRect]);

  const closeMenu = useCallback(() => setMenu(null), []);

  const rectGetter = useCallback(() => rect, [rect]);
  useFileDrop(popupId, rectGetter, {
    onHover: setDragOver,
    onDrop: (paths) => {
      if (paths.length === 0) return;
      invoke("folder_add", { dir, paths, mode: settings.dropMode }).then(onRefresh).catch(console.warn);
      setDragOver(false);
    },
  });

  const launch = async (entry: FolderEntry) => {
    try {
      await invoke("folder_launch", { dir, path: entry.path });
      if (settings.closeOnLaunch) onClose();
    } catch (e) { console.warn(e); }
  };
  const reveal = (entry: FolderEntry) => invoke("folder_reveal", { dir, path: entry.path }).catch(console.warn);
  const takeOut = (entry: FolderEntry) => invoke("folder_take_out", { dir, path: entry.path }).then(onRefresh).catch(console.warn);
  const recycle = (entry: FolderEntry) => invoke("folder_recycle", { dir, path: entry.path }).then(onRefresh).catch(console.warn);

  if (!rect) return null;

  return createPortal(
    <div
      data-dismiss-scope={instanceId}
      className={`folder-popup ${settings.layout} ${dragOver ? "drag-over" : ""}`}
      style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, ...(accent ? { "--accent": accent } as CSSProperties : {}) }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {entries.length === 0 && <div className="folder-popup-empty">비어 있음 — 파일을 끌어다 놓으세요</div>}
      {entries.map((e) => (
        <button
          key={e.path}
          className="folder-item"
          onClick={() => launch(e)}
          onContextMenu={(ev) => { ev.preventDefault(); ev.stopPropagation(); setMenu({ x: ev.clientX, y: ev.clientY, entry: e }); }}
          title={e.name}
        >
          <span className="folder-item-icon">{e.icon ? <img src={e.icon} alt="" /> : <span>{e.kind === "dir" ? "📁" : "📄"}</span>}</span>
          {settings.showItemNames && <span className="folder-item-name">{e.name}</span>}
        </button>
      ))}
      {menu && (
        <ContextMenu
          id={`${instanceId}-item-menu`}
          parentScope={instanceId}
          at={menu}
          accent={accent}
          items={[
            { label: "탐색기에서 위치 열기", onSelect: () => reveal(menu.entry) },
            { label: "바탕화면으로 빼기", onSelect: () => takeOut(menu.entry) },
            { label: "휴지통으로", onSelect: () => recycle(menu.entry) },
          ]}
          onClose={closeMenu}
        />
      )}
    </div>,
    document.body,
  );
}
