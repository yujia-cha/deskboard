import { useEffect, useState } from "react";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { contentScale, useSettings } from "../core/settings";
import { WIDGETS, widgetById } from "../widgets/registry";
import type { SettingField } from "../widgets/types";
import "./SettingsPanel.css";

/** 입력 중에는 건드리지 않고 blur/Enter 때만 적용하는 숫자 입력 */
function NumField({ value, min, onCommit }: { value: number; min: number; onCommit: (v: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => { setText(String(value)); }, [value]);
  const commit = () => { const n = Math.round(Number(text)); onCommit(Number.isFinite(n) ? Math.max(min, n) : value); };
  return <input type="number" min={min} step={8} value={text} onChange={(e) => setText(e.target.value)} onBlur={commit}
    onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} />;
}

const SIZE_PRESETS = [{ label: "작게", k: 0.75 }, { label: "기본", k: 1 }, { label: "크게", k: 1.4 }, { label: "아주 크게", k: 1.8 }];

/** 전역 설정 + 선택된 위젯의 스키마 기반 설정 폼. */
export function SettingsPanel() {
  const s = useSettings();
  const inst = s.instances.find((i) => i.id === s.selected);
  const def = inst ? widgetById(inst.widgetId) : undefined;
  const [autostart, setAutostart] = useState<boolean | null>(null);
  useEffect(() => { isEnabled().then(setAutostart).catch(() => setAutostart(null)); }, []);

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
              <div className="row dim"><span>내용 배율</span><span>{s.autoScale ? `${Math.round(contentScale(inst, true) * 100)}% (자동)` : "100% (자동 맞춤 꺼짐)"}</span></div>
            </section>
            <section>
              <h4>옵션</h4>
              {(def.settingsSchema ?? []).length === 0 && <p className="dim">이 위젯은 옵션이 없습니다.</p>}
              {def.settingsSchema?.map((f) => (
                <Field key={f.key} f={f} value={inst.settings[f.key]} onChange={(v) => s.updateWidgetSettings(inst.id, { [f.key]: v })} />
              ))}
              <button className="danger" onClick={() => { s.removeWidget(inst.id); }}>위젯 제거</button>
            </section>
          </>
        ) : (
          <>
            <section>
              <h4>모양</h4>
              <label className="row"><span>테마</span>
                <select value={s.themeMode} onChange={(e) => s.setThemeMode(e.target.value as "solid" | "translucent")}>
                  <option value="translucent">반투명 (Acrylic)</option>
                  <option value="solid">단색</option>
                </select>
              </label>
              <label className="row"><span>강조색</span>
                <input type="color" value={s.accent} onChange={(e) => s.setAccent(e.target.value)} />
              </label>
              <label className="row" title="위젯을 키우거나 줄이면 내용도 같은 비율로 확대/축소"><span>내용 자동 맞춤</span>
                <input type="checkbox" checked={s.autoScale} onChange={(e) => s.setAutoScale(e.target.checked)} />
              </label>
              <label className="row"><span>편집 모드</span>
                <input type="checkbox" checked={!s.locked} onChange={(e) => s.setLocked(!e.target.checked)} />
              </label>
              <label className="row"><span>Windows 시작 시 실행</span>
                <input type="checkbox" checked={!!autostart} disabled={autostart === null}
                  onChange={async (e) => {
                    const on = e.target.checked;
                    try { if (on) await enable(); else await disable(); setAutostart(on); } catch (err) { console.warn(err); }
                  }} />
              </label>
            </section>
            <section>
              <h4>위젯 추가</h4>
              <div className="widget-list">
                {WIDGETS.map((w) => (
                  <button key={w.id} onClick={() => { s.addWidget(w.id); }}>{w.icon} {w.title}</button>
                ))}
              </div>
            </section>
            <section>
              <h4>배치된 위젯</h4>
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

function Field({ f, value, onChange }: { f: SettingField; value: unknown; onChange: (v: unknown) => void }) {
  switch (f.type) {
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
  }
}
