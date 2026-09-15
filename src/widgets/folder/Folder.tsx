import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../../core/ipc";
import { useSettings, type Rect } from "../../core/settings";
import { useFileDrop } from "../../core/fileDrop";
import { useDismiss } from "../../core/dismiss";
import type { WidgetProps } from "../types";
import { FolderPopup } from "./FolderPopup";
import "./Folder.css";

export interface FolderSettings extends Record<string, unknown> {
  name: string;
  image: string;
  dir: string;
  layout: "list" | "grid";
  columns: number;
  showName: boolean;
  showItemNames: boolean;
  dropMode: "move" | "copy";
  closeOnLaunch: boolean;
}

export interface FolderEntry {
  name: string;
  path: string;
  kind: "dir" | "file";
  icon: string | null;
}

/** 디스코드식 바로가기 폴더 — 실제 디렉터리에 연결된 위젯. */
export function Folder({ instanceId, settings, size }: WidgetProps<FolderSettings>) {
  const updateWidgetSettings = useSettings((s) => s.updateWidgetSettings);
  const setOverlayRect = useSettings((s) => s.setOverlayRect);
  const accent = useSettings((s) => s.instances.find((i) => i.id === instanceId)?.accent);
  const [dir, setDir] = useState(settings.dir);
  const [entries, setEntries] = useState<FolderEntry[]>([]);
  const [customImage, setCustomImage] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const iconRef = useRef<HTMLButtonElement>(null);

  // 최초 표시 시 기본 디렉터리 생성(%APPDATA%/.../folders/<instanceId>) 후 설정에 저장
  useEffect(() => {
    let cancelled = false;
    invoke<string>("folder_ensure_dir", { instanceId, dir: settings.dir || null }).then((d) => {
      if (cancelled) return;
      setDir(d);
      if (d !== settings.dir) updateWidgetSettings(instanceId, { dir: d });
    }).catch(console.warn);
    return () => { cancelled = true; };
    // 인스턴스당 한 번만 — 사용자가 설정에서 dir 을 바꾸면 아래 effect 가 별도로 반영한다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);
  useEffect(() => { if (settings.dir && settings.dir !== dir) setDir(settings.dir); }, [settings.dir, dir]);

  const refresh = useCallback(() => {
    if (!dir) return;
    invoke<FolderEntry[]>("folder_list", { dir }).then(setEntries).catch(console.warn);
  }, [dir]);

  useEffect(() => { refresh(); }, [refresh]);
  useEffect(() => { if (dir) invoke("folder_watch", { dir }).catch(console.warn); }, [dir]);
  useEvent("folders://changed", refresh);

  useEffect(() => {
    if (!settings.image) { setCustomImage(null); return; }
    let cancelled = false;
    invoke<string>("folder_read_image", { path: settings.image }).then((d) => { if (!cancelled) setCustomImage(d); }).catch(() => { if (!cancelled) setCustomImage(null); });
    return () => { cancelled = true; };
  }, [settings.image]);

  // 아이콘·팝업 밖을 누르거나(바탕화면 포함) Esc 면 닫는다.
  useDismiss(instanceId, open, useCallback(() => setOpen(false), []));
  // 위젯이 사라지면 등록한 오버레이 히트 영역도 정리
  useEffect(() => () => setOverlayRect(instanceId, null), [instanceId, setOverlayRect]);

  const rectGetter = useCallback((): Rect | null => {
    if (open) return null; // 펼쳐진 동안은 팝업이 드롭을 받는다
    const el = iconRef.current;
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  }, [open]);
  useFileDrop(instanceId, rectGetter, {
    onHover: setDragOver,
    onDrop: (paths) => {
      if (!dir || paths.length === 0) return;
      invoke("folder_add", { dir, paths, mode: settings.dropMode }).then(refresh).catch(console.warn);
      setDragOver(false);
    },
  });

  const preview = entries.slice(0, 4);
  const iconSize = Math.max(28, Math.min(size.w, size.h - (settings.showName ? 20 : 0)));

  return (
    <div className="folder-widget">
      <button
        ref={iconRef}
        data-dismiss-scope={instanceId}
        className={`folder-icon ${dragOver ? "drag-over" : ""}`}
        style={{ width: iconSize, height: iconSize }}
        onClick={() => setOpen((v) => !v)}
        title={settings.name}
      >
        {customImage ? (
          <img src={customImage} alt="" />
        ) : preview.length > 0 ? (
          <span className="folder-preview">
            {preview.map((e) => (
              <span key={e.path} className="folder-preview-cell">{e.icon ? <img src={e.icon} alt="" /> : <span>{e.kind === "dir" ? "📁" : "📄"}</span>}</span>
            ))}
          </span>
        ) : (
          <span className="folder-emoji">📁</span>
        )}
      </button>
      {settings.showName && <div className="folder-name">{settings.name}</div>}
      {open && dir && (
        <FolderPopup
          instanceId={instanceId}
          anchorEl={iconRef.current}
          dir={dir}
          entries={entries}
          settings={settings}
          onClose={() => setOpen(false)}
          onRefresh={refresh}
          accent={accent}
        />
      )}
    </div>
  );
}
