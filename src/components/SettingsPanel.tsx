import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { contentScale, useSettings, type CardStyle, type Palette, type ThemeMode } from "../core/settings";
import { useRegistry, type RegistryEntry } from "../core/widgetRegistry";
import { useConfig, type FileStatus } from "../core/config";
import { fieldVisible, type SettingField, type WidgetDefinition } from "../core/widgetTypes";
import { UPDATER_ENABLED, useUpdater, type UpdateState } from "../core/updater";
import { UpdateDot } from "./UpdateDot";
import "./SettingsPanel.css";

/** 입력 중에는 건드리지 않고 blur/Enter 때만 적용하는 숫자 입력 */
function NumField({ value, min, onCommit }: { value: number; min: number; onCommit: (v: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => { setText(String(value)); }, [value]);
  const commit = () => { const n = Math.round(Number(text)); onCommit(Number.isFinite(n) ? Math.max(min, n) : value); };
  return <input type="number" min={min} step={8} value={text} onChange={(e) => setText(e.target.value)} onBlur={commit}
    onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} />;
}

/** 즉시 반영되는 슬라이더 — 값은 오른쪽에 숫자로 같이 보여준다. */
function Slider({ label, value, min, max, step, suffix = "", title, onChange }: {
  label: string; value: number; min: number; max: number; step: number;
  suffix?: string; title?: string; onChange: (v: number) => void;
}) {
  return (
    <label className="row" title={title}><span>{label}</span>
      <span className="pair">
        <input type="range" min={min} max={max} step={step} value={value}
          onChange={(e) => onChange(Number(e.target.value))} />
        <span className="dim slider-value">{value}{suffix}</span>
      </span>
    </label>
  );
}

/** 전역 설정 + 선택된 위젯의 스키마 기반 설정 폼. */
export function SettingsPanel() {
  const s = useSettings();
  const inst = s.instances.find((i) => i.id === s.selected);
  const defs = useRegistry((r) => r.defs);
  const entries = useRegistry((r) => r.entries);
  const sizePresets = useConfig((c) => c.config.layout.sizePresets);
  const def = inst ? defs[inst.widgetId] : undefined;
  const entry = inst ? entries[inst.widgetId] : undefined;
  const [monitors, setMonitors] = useState<{ name: string; primary: boolean; work: { w: number; h: number } }[]>([]);
  const [autoStatus, setAutoStatus] = useState<{ enabled: boolean; path: string | null; dev: boolean } | null>(null);
  const updater = useUpdater();
  const refreshAutoStatus = () => invoke<typeof autoStatus>("autostart_status").then(setAutoStatus).catch(() => setAutoStatus(null));
  useEffect(() => {
    invoke<typeof monitors>("list_monitors").then(setMonitors).catch(() => setMonitors([]));
    refreshAutoStatus();
  }, [s.settingsOpen]);

  if (!s.settingsOpen) return null;

  return (
    <div className="settings-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) s.closeSettings(); }}>
      <div className="settings">
        <header>
          <strong>{def ? `${def.title} 설정` : "deskboard 설정"}</strong>
          <span>
            {def && <button onClick={() => s.openSettings(null)} title="전체 설정">‹</button>}
            <button onClick={s.closeSettings}>✕</button>
          </span>
        </header>

        {def && inst ? (
          <>
            <section>
              <h4>크기 · 위치</h4>
              <div className="row"><span>프리셋</span>
                <span className="preset-row">
                  {sizePresets.map((p) => (
                    <button key={p.label} onClick={() => s.moveResize(inst.id, {
                      w: Math.max(def.minSize.w, Math.round(def.defaultSize.w * p.k)),
                      h: Math.max(def.minSize.h, Math.round(def.defaultSize.h * p.k)),
                    })}>{p.label}</button>
                  ))}
                </span>
              </div>
              <label className="row"><span>너비 × 높이 (px)</span>
                <span className="pair">
                  <NumField value={inst.w} min={def.minSize.w} onCommit={(w) => s.moveResize(inst.id, { w })} />
                  ×
                  <NumField value={inst.h} min={def.minSize.h} onCommit={(h) => s.moveResize(inst.id, { h })} />
                </span>
              </label>
              <label className="row"><span>위치 X, Y (px)</span>
                <span className="pair">
                  <NumField value={inst.x} min={0} onCommit={(x) => s.moveResize(inst.id, { x })} />
                  ,
                  <NumField value={inst.y} min={0} onCommit={(y) => s.moveResize(inst.id, { y })} />
                </span>
              </label>
              <div className="row dim"><span>내용 배율</span><span>{s.autoScale ? `${Math.round(contentScale(inst, true) * 100)}% (넘치면 자동 축소)` : "100% (넘치면 자동 축소)"}</span></div>
            </section>
            <section>
              <h4>색상</h4>
              <label className="row"><span>강조색</span>
                <span className="pair">
                  <input type="color" value={inst.accent ?? s.accent} onChange={(e) => s.setInstanceAccent(inst.id, e.target.value)} />
                  {inst.accent && <button onClick={() => s.setInstanceAccent(inst.id, null)}>전역 색 사용</button>}
                </span>
              </label>
            </section>
            <section>
              <h4>옵션</h4>
              {entry && <WidgetSource entry={entry} />}
              {(def.settingsSchema ?? []).length === 0 && <p className="dim">이 위젯은 옵션이 없습니다.</p>}
              {def.settingsSchema?.filter((f) => fieldVisible(f, inst.settings)).map((f) => (
                <Field key={f.key} f={f} value={inst.settings[f.key]} onChange={(v) => s.updateWidgetSettings(inst.id, { [f.key]: v })} />
              ))}
              {!def.singleton && <button className="danger" onClick={() => { s.removeWidget(inst.id); }}>위젯 제거</button>}
            </section>
          </>
        ) : (
          <>
            <section>
              <h4>모양</h4>
              <label className="row"><span>표면</span>
                <select value={s.themeMode} onChange={(e) => s.setThemeMode(e.target.value as ThemeMode)}>
                  <option value="translucent">반투명</option>
                  <option value="solid">단색</option>
                </select>
              </label>
              <label className="row" title="밝은 배경화면에서는 라이트가 읽기 좋습니다"><span>팔레트</span>
                <select value={s.palette} onChange={(e) => s.setPalette(e.target.value as Palette)}>
                  <option value="dark">다크</option>
                  <option value="light">라이트</option>
                  <option value="auto">시스템 설정 따르기</option>
                </select>
              </label>
              <label className="row"><span>카드 스타일</span>
                <select value={s.cardStyle} onChange={(e) => s.setCardStyle(e.target.value as CardStyle)}>
                  <option value="glass">글래스 (테두리 + 그림자)</option>
                  <option value="minimal">미니멀 (테두리만)</option>
                  <option value="borderless">보더리스 (배경만)</option>
                  <option value="none">카드 없음 (내용만)</option>
                </select>
              </label>
              <label className="row"><span>강조색</span>
                <input type="color" value={s.accent} onChange={(e) => s.setAccent(e.target.value)} />
              </label>
              <Slider label="카드 불투명도" value={s.surfaceOpacity} min={10} max={100} step={2}
                suffix="%" onChange={s.setSurfaceOpacity} />
              <Slider label="배경 블러" value={s.blurStrength} min={0} max={6} step={1}
                title="카드 뒤에 배경화면을 블러해서 깝니다. 0 이면 단색 표면만 쓰고 캡처도 하지 않습니다."
                suffix={s.blurStrength === 0 ? " (끔)" : ""} onChange={s.setBlurStrength} />
              <Slider label="모서리 반경" value={s.cornerRadius} min={0} max={32} step={2}
                suffix="px" onChange={s.setCornerRadius} />
              <Slider label="테두리 두께" value={s.borderWidth} min={1} max={6} step={1}
                title="카드 경계선의 두께. 진하기와 따로 조절합니다."
                suffix="px" onChange={s.setBorderWidth} />
              <Slider label="테두리 진하기" value={s.borderStrength} min={0} max={100} step={5}
                title="카드 경계선의 진하기. 배경화면이 복잡하면 올려서 카드를 또렷하게 만듭니다."
                suffix="%" onChange={s.setBorderStrength} />
              <div className="dim" style={{ fontSize: "var(--fs-meta)" }}>
                {s.blurStrength === 0
                  ? "꺼져 있습니다 — 화면을 캡처하지 않고 카드는 단색/반투명 표면만 씁니다."
                  : s.wallpaperError
                    ? `배경 블러: ${s.wallpaperError}`
                    : s.wallpaperSource === "capture"
                      ? "배경 블러: 화면 캡처 — 라이브 배경화면도 따라갑니다 (움직이면 2초, 멈춰 있으면 10초마다 확인)"
                      : s.wallpaperSource === "file"
                        ? "배경 블러: 배경화면 파일 (30초마다) — 화면 캡처가 막혀 있습니다"
                        : "배경 블러: 준비 중…"}
              </div>
              <label className="row" title="위젯을 키우면 내용도 같은 비율로 확대 (넘치는 내용은 항상 자동 축소)"><span>크기에 맞춰 내용 확대</span>
                <input type="checkbox" checked={s.autoScale} onChange={(e) => s.setAutoScale(e.target.checked)} />
              </label>
              <label className="row" title="대시보드는 이 모니터의 작업영역(작업표시줄 제외) 전체를 덮습니다"><span>표시 모니터</span>
                <select value={s.canvasMonitor ?? ""} onChange={(e) => s.setCanvasMonitor(e.target.value || null)}>
                  <option value="">주 모니터</option>
                  {monitors.map((m) => <option key={m.name} value={m.name}>{m.name.replace(/^\\\\\.\\/, "")} {m.work.w}×{m.work.h}{m.primary ? " (주)" : ""}</option>)}
                </select>
              </label>
              <label className="row"><span>편집 모드</span>
                <input type="checkbox" checked={!s.locked} onChange={(e) => s.setLocked(!e.target.checked)} />
              </label>
              <label className="row" title={import.meta.env.DEV ? "개발 실행 중에는 등록하지 않습니다" : "설치된 deskboard 를 로그인 시 자동 실행"}><span>Windows 시작 시 실행</span>
                <input type="checkbox" checked={s.autostart} onChange={(e) => { s.setAutostart(e.target.checked).catch(console.warn).finally(refreshAutoStatus); }} />
              </label>
              <div className="dim" style={{ fontSize: 11, wordBreak: "break-all" }}>
                {autoStatus?.dev ? "개발 실행에서는 등록하지 않습니다 (설치본에서 적용)"
                  : autoStatus?.enabled ? `등록됨: ${autoStatus.path}`
                  : "등록 안 됨"}
              </div>
            </section>
            <AddWidgets defs={defs} entries={entries} onAdd={s.addWidget} />
            <ConfigFiles />
            <section>
              <h4>업데이트<UpdateDot className="inline" /></h4>
              <UpdateSection u={updater} />
            </section>
            <section>
              <h4>배치된 위젯</h4>
              <button className="link" onClick={() => { if (confirm("모든 위젯을 기본 배치로 되돌릴까요?")) s.resetLayout(); }}>↺ 기본 레이아웃으로 초기화</button>
              {s.instances.map((i) => (
                <button key={i.id} className="link" onClick={() => s.openSettings(i.id)}>
                  {defs[i.widgetId]?.icon ?? "❔"} {defs[i.widgetId]?.title ?? i.widgetId} <span className="dim">{i.w}×{i.h} @ {i.x},{i.y}</span>
                </button>
              ))}
            </section>
          </>
        )}
      </div>
    </div>
  );
}

const openDir = (id: string | null) => invoke("widgets_open_dir", { id }).catch((e) => alert(String(e)));

/**
 * 위젯 추가 목록 — 내장 / 내가 만든 위젯으로 나눈다.
 * 깨진 사용자 위젯도 목록에 보여 준다: 왜 안 뜨는지 알아야 고칠 수 있다.
 */
function AddWidgets({ defs, entries, onAdd }: {
  defs: Record<string, WidgetDefinition>; entries: Record<string, RegistryEntry>; onAdd: (id: string) => void;
}) {
  const addable = Object.values(defs).filter((w) => !w.singleton && !w.hidden);
  const builtin = addable.filter((w) => w.root === "builtin");
  const user = addable.filter((w) => w.root === "user");
  const broken = Object.values(entries).filter((e) => e.status === "error");
  return (
    <section>
      <h4>위젯 추가</h4>
      <div className="widget-list">
        {builtin.map((w) => <button key={w.id} title={w.description} onClick={() => onAdd(w.id)}>{w.icon} {w.title}</button>)}
      </div>
      <h4>내가 만든 위젯</h4>
      {user.length > 0 && (
        <div className="widget-list">
          {user.map((w) => (
            <button key={w.id} title={w.overridesBuiltin ? "같은 이름의 내장 위젯을 덮어쓰고 있습니다" : w.description} onClick={() => onAdd(w.id)}>
              {w.icon} {w.title}{w.overridesBuiltin ? " ✎" : ""}
            </button>
          ))}
        </div>
      )}
      {broken.map((e) => (
        <p key={e.id} className="field-note" style={{ color: "var(--danger)" }}>⚠ {e.id}: {e.error}</p>
      ))}
      {user.length === 0 && broken.length === 0 && (
        <p className="dim field-note">widgets 폴더에 폴더를 하나 만들면 그게 위젯이 됩니다. 안의 README.md 를 보세요.</p>
      )}
      <p className="dim field-note">
        내가 만든 위젯의 코드는 대시보드와 같은 권한으로 실행됩니다 — 직접 쓰거나 믿을 수 있는 코드만 넣으세요.
      </p>
      <button className="link" onClick={() => openDir(null)}>📂 위젯 폴더 열기</button>
    </section>
  );
}

/** 선택한 위젯이 어디서 왔는지 + 고치는 길 (폴더 열기 / 내장 위젯을 복사해서 고치기). */
function WidgetSource({ entry }: { entry: RegistryEntry }) {
  const info = entry.info;
  if (!info) return null;
  const eject = () => invoke<string>("widgets_eject", { id: info.id })
    .then((dir) => alert(`복사했습니다 — 이제 이 사본이 내장 위젯 대신 쓰입니다:\n${dir}`))
    .catch((e) => alert(String(e)));
  return (
    <>
      <div className="row dim" style={{ fontSize: "var(--fs-meta)" }}>
        <span>{info.root === "builtin" ? "내장 위젯" : info.overridesBuiltin ? "내 사본 (내장 위젯을 덮어씀)" : "내가 만든 위젯"}</span>
        <span className="pair">
          <button className="link" onClick={() => openDir(info.id)}>폴더 열기</button>
          {info.root === "builtin" && (
            <button className="link" title="이 위젯을 내 widgets 폴더로 복사합니다. 사본을 고치면 바로 반영됩니다." onClick={eject}>복사해서 고치기</button>
          )}
        </span>
      </div>
      {entry.warnings.map((w, i) => <p key={i} className="field-note" style={{ color: "var(--warn)" }}>{w}</p>)}
    </>
  );
}

const FILE_HELP: Record<string, string> = {
  "config.jsonc": "기본값·배치·크기 프리셋·폴링 주기·내장 위젯 덮어쓰기",
  "strings.jsonc": "트레이 메뉴·편집 바 문구",
  "user.css": "테마 위에 덮는 스타일",
};

/** 손으로 고치는 설정 파일들의 상태. 깨진 파일은 무시되고 있다는 걸 여기서 알린다. */
function ConfigFiles() {
  const files = useConfig((c) => c.files);
  const label = (f: FileStatus) =>
    f.state === "ok" ? (f.messages.length ? "적용됨 (경고 있음)" : "적용됨") : f.state === "error" ? "오류 — 무시 중" : "없음 (기본값)";
  return (
    <section>
      <h4>사용자 설정 파일</h4>
      {files.map((f) => (
        <div key={f.file}>
          <div className="row" title={FILE_HELP[f.file]}>
            <span>{f.file}</span>
            <span className="dim" style={f.state === "error" ? { color: "var(--danger)" } : undefined}>{label(f)}</span>
          </div>
          {f.messages.map((m, i) => (
            <p key={i} className="dim field-note" style={{ color: f.state === "error" ? "var(--danger)" : "var(--warn)" }}>{m}</p>
          ))}
        </div>
      ))}
      <p className="dim field-note">같은 폴더의 *.sample.* 파일을 복사해 이름을 바꿔 쓰세요. 저장하면 바로 반영됩니다.</p>
      <button className="link" onClick={() => invoke("config_open_dir").catch((e) => alert(String(e)))}>📂 설정 폴더 열기</button>
    </section>
  );
}

/** 버전 표시 + 업데이트 확인·설치. 상태 하나가 곧 화면이다 (`core/updater.ts`). */
function UpdateSection({ u }: { u: { version: string; state: UpdateState; checkNow(): void; install(): void; restart(): void } }) {
  const s = u.state;
  return (
    <>
      <div className="row"><span>현재 버전</span><span className="dim">{u.version || "…"}</span></div>
      {!UPDATER_ENABLED ? (
        <p className="dim field-note">개발 실행에서는 업데이트를 확인하지 않습니다 (설치본에서 동작).</p>
      ) : (
        <>
          <div className="row">
            <span>
              {s.kind === "checking" ? "확인 중…"
                : s.kind === "none" ? "최신 버전입니다"
                : s.kind === "available" ? `새 버전 ${s.version}`
                : s.kind === "downloading" ? `받는 중… ${s.percent === null ? "" : `${s.percent}%`}`
                : s.kind === "ready" ? `${s.version} 설치 완료`
                : s.kind === "error" ? "확인하지 못했습니다"
                : "업데이트"}
            </span>
            <span className="pair">
              {s.kind === "available" && <button onClick={() => { u.install(); }}>지금 설치</button>}
              {s.kind === "ready" && <button onClick={u.restart}>다시 시작</button>}
              {(s.kind === "idle" || s.kind === "none" || s.kind === "error") &&
                <button onClick={() => { u.checkNow(); }}>업데이트 확인</button>}
            </span>
          </div>
          {s.kind === "error" && <p className="dim field-note">{s.message}</p>}
          {s.kind === "available" && s.notes && <p className="dim field-note">{s.notes}</p>}
          {s.kind === "ready" && <p className="dim field-note">다시 시작하면 새 버전으로 열립니다.</p>}
        </>
      )}
    </>
  );
}

function Field({ f, value, onChange }: { f: SettingField; value: unknown; onChange: (v: unknown) => void }) {
  switch (f.type) {
    case "note":
      return <p className="dim field-note">{f.label}</p>;
    case "boolean":
      return <label className="row"><span>{f.label}</span><input type="checkbox" checked={!!value} onChange={(e) => onChange(e.target.checked)} /></label>;
    case "number":
      return <label className="row"><span>{f.label}</span><input type="number" value={Number(value)} min={f.min} max={f.max} step={f.step} onChange={(e) => onChange(Number(e.target.value))} /></label>;
    case "text":
      return <label className="row"><span>{f.label}</span><input type="text" value={String(value ?? "")} placeholder={f.placeholder} onChange={(e) => onChange(e.target.value)} /></label>;
    case "select":
      return <label className="row"><span>{f.label}</span>
        <select value={String(value)} onChange={(e) => onChange(e.target.value)}>
          {f.options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select></label>;
    case "path": {
      const pick = async () => {
        const selected = await openDialog({
          directory: f.pick === "directory",
          multiple: false,
          filters: f.pick === "image" ? [{ name: "이미지", extensions: ["png", "jpg", "jpeg", "ico", "webp", "gif"] }] : undefined,
        }).catch(() => null);
        if (typeof selected === "string") onChange(selected);
      };
      return <label className="row"><span>{f.label}</span>
        <span className="pair">
          <input type="text" value={String(value ?? "")} readOnly title={String(value ?? "")} />
          <button onClick={pick}>찾아보기</button>
          {value ? <button onClick={() => onChange("")}>지우기</button> : null}
        </span></label>;
    }
  }
}
