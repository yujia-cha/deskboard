import { useEffect, useState } from "react";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useSettings } from "../core/settings";
import { WIDGETS, widgetById } from "../widgets/registry";
import type { SettingField } from "../widgets/types";
import "./SettingsPanel.css";

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
          <button onClick={s.closeSettings}>✕</button>
        </header>

        {def && inst ? (
          <section>
            {(def.settingsSchema ?? []).length === 0 && <p className="dim">이 위젯은 설정이 없습니다.</p>}
            {def.settingsSchema?.map((f) => (
              <Field key={f.key} f={f} value={inst.settings[f.key]} onChange={(v) => s.updateWidgetSettings(inst.id, { [f.key]: v })} />
            ))}
            <button className="danger" onClick={() => { s.removeWidget(inst.id); }}>위젯 제거</button>
          </section>
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
                  {widgetById(i.widgetId)?.title} <span className="dim">({i.x},{i.y})</span>
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
