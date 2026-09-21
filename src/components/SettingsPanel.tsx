import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { contentScale, useSettings, type CardStyle, type Palette, type ThemeMode } from "../core/settings";
import { WIDGETS, widgetById } from "../widgets/registry";
import { fieldVisible, type SettingField } from "../widgets/types";
import { UPDATER_ENABLED, useUpdater } from "../core/updater";
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

const SIZE_PRESETS = [{ label: "작게", k: 0.75 }, { label: "기본", k: 1 }, { label: "크게", k: 1.4 }, { label: "아주 크게", k: 1.8 }];

/** 전역 설정 + 선택된 위젯의 스키마 기반 설정 폼. */
export function SettingsPanel() {
  const s = useSettings();
  const inst = s.instances.find((i) => i.id === s.selected);
  const def = inst ? widgetById(inst.widgetId) : undefined;
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
                  {SIZE_PRESETS.map((p) => (
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
            <section>
              <h4>위젯 추가</h4>
              <div className="widget-list">
                {WIDGETS.filter((w) => !w.singleton).map((w) => (
                  <button key={w.id} onClick={() => { s.addWidget(w.id); }}>{w.icon} {w.title}</button>
                ))}
              </div>
            </section>
            <section>
              <h4>업데이트</h4>
              <UpdateSection u={updater} />
            </section>
            <section>
              <h4>배치된 위젯</h4>
              <button className="link" onClick={() => { if (confirm("모든 위젯을 기본 배치로 되돌릴까요?")) s.resetLayout(); }}>↺ 기본 레이아웃으로 초기화</button>
              {s.instances.map((i) => (
                <button key={i.id} className="link" onClick={() => s.openSettings(i.id)}>
                  {widgetById(i.widgetId)?.icon} {widgetById(i.widgetId)?.title} <span className="dim">{i.w}×{i.h} @ {i.x},{i.y}</span>
                </button>
              ))}
            </section>
          </>
        )}
      </div>
    </div>
  );
}

/** 버전 표시 + 업데이트 확인·설치. 상태 하나가 곧 화면이다 (`core/updater.ts`). */
function UpdateSection({ u }: { u: ReturnType<typeof useUpdater> }) {
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
