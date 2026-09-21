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
  /**
   * 이 위젯이 보는 폴더가 어디서 왔는가.
   * - `managed`: deskboard 가 인스턴스마다 만들어 주는 전용 폴더 (`dir` 은 쓰지 않는다)
   * - `link`   : 사용자가 고른 기존 폴더 (`dir` 이 그 경로다)
   */
  source: "managed" | "link";
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

/**
 * 디스코드식 바로가기 폴더 — 실제 디렉터리 하나에 연결된 위젯.
 *
 * 폴더를 정하는 길이 **둘**이고, 둘은 결과가 다르다 (`settings.source`):
 * - `managed` — deskboard 가 이 인스턴스만의 빈 폴더를 만들어 준다. 드롭한 파일이 그리로 모인다.
 * - `link`    — 사용자가 이미 쓰는 폴더를 그대로 본다. 여기서 지우면 원본이 지워진다.
 *
 * 그래서 `dir` 은 **연결 모드의 입력값일 뿐**이다. 전용 폴더의 경로는 설정에 적지 않는다 —
 * 적어 두면 다른 PC 로 `settings.json` 을 옮겼을 때 남의 계정 경로가 따라온다.
 */
export function Folder({ instanceId, settings, size, editing }: WidgetProps<FolderSettings>) {
  const setOverlayRect = useSettings((s) => s.setOverlayRect);
  const openSettings = useSettings((s) => s.openSettings);
  const accent = useSettings((s) => s.instances.find((i) => i.id === instanceId)?.accent);
  const [dir, setDir] = useState("");
  const [dirError, setDirError] = useState<string | null>(null);
  const [entries, setEntries] = useState<FolderEntry[]>([]);
  const [customImage, setCustomImage] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const iconRef = useRef<HTMLButtonElement>(null);

  // 볼 폴더를 정한다. 전용 폴더는 없으면 만들고, 연결 폴더는 **만들지 않고 확인만** 한다 —
  // 오타나 남의 PC 경로를 지어 버리면 빈 껍데기가 생기고 사용자는 왜 비었는지 알 수 없다.
  useEffect(() => {
    let cancelled = false;
    invoke<string>("folder_ensure_dir", { instanceId, source: settings.source, dir: settings.dir || null })
      .then((d) => { if (!cancelled) { setDir(d); setDirError(null); } })
      .catch((e) => { if (!cancelled) { setDir(""); setDirError(String(e)); } });
    return () => { cancelled = true; };
  }, [instanceId, settings.source, settings.dir]);

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

  // 연결할 폴더를 아직 고르지 않았거나 그 폴더가 사라졌다 — 빈 아이콘 대신 이유를 보여준다.
  if (settings.source === "link" && (!settings.dir || dirError)) {
    return (
      <button className="folder-unset" title={dirError ?? settings.dir} onClick={() => openSettings(instanceId)}>
        <span className="folder-emoji">🔗</span>
        <span>{!settings.dir ? "연결할 폴더 고르기" : "폴더를 찾을 수 없습니다"}</span>
        {editing && <span className="dim">설정에서 바꾸기</span>}
      </button>
    );
  }

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
