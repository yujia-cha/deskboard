import { useEffect, useState } from "react";
import { invokeInOrder, useProviderEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
import { PlacePicker } from "./PlacePicker";
import "./Weather.css";

export interface WeatherSettings extends Record<string, unknown> {
  place: string;
  lat: number;
  lon: number;
  showHourly: boolean;
  showFeels: boolean;
}

interface Hour { hour: number; temp_c: number; pop: number; code: number }
interface Sample {
  temp_c: number; feels_c: number; pop: number; code: number; text: string; icon: string;
  is_day: boolean; high_c: number; low_c: number; hourly: Hour[]; error: string | null;
}

const round = (n: number) => (Number.isFinite(n) ? Math.round(n) : null);
const deg = (n: number) => { const r = round(n); return r === null ? "—" : `${r}°`; };

export function Weather({ instanceId, settings, size, editing }: WidgetProps<WeatherSettings>) {
  const w = useProviderEvent<Sample>("weather://update");
  const [picking, setPicking] = useState(false);

  // 위젯이 떠 있는 동안만 백엔드가 날씨를 받아온다.
  useEffect(() => {
    const lat = Number(settings.lat);
    const lon = Number(settings.lon);
    const ok = Number.isFinite(lat) && Number.isFinite(lon) && (lat !== 0 || lon !== 0);
    invokeInOrder("weather_set_active", { active: ok, lat: ok ? lat : null, lon: ok ? lon : null });
    return () => { invokeInOrder("weather_set_active", { active: false }); };
  }, [settings.lat, settings.lon]);

  const hasPlace = Number.isFinite(Number(settings.lat)) &&
    (Number(settings.lat) !== 0 || Number(settings.lon) !== 0);

  // 위치가 없거나, 편집 모드에서 바꾸려고 누른 동안에는 검색 UI 를 보여준다.
  if (!hasPlace || picking) {
    return (
      <div className="wx-setup">
        <div className="wx-setup-title">{hasPlace ? "위치 바꾸기" : "위치를 정해 주세요"}</div>
        <PlacePicker instanceId={instanceId} onDone={() => setPicking(false)} />
        {hasPlace && <button className="link" onClick={() => setPicking(false)}>취소</button>}
      </div>
    );
  }

  if (!w) return <div className="dim">날씨를 가져오는 중…</div>;
  if (w.error) return <div className="wx-error">{w.error}</div>;

  // 좁으면 시간별 예보를 줄이거나 숨긴다
  const hourSlots = Math.max(0, Math.min(w.hourly.length, Math.floor((size.w - 110) / 42)));
  const showHourly = settings.showHourly && hourSlots >= 3 && size.h >= 130;

  return (
    <div className="wx">
      {editing && (
        <button className="wx-change link" onClick={() => setPicking(true)}>
          📍 {String(settings.place || "위치 바꾸기")}
        </button>
      )}
      <div className="wx-now">
        <span className="wx-icon" title={w.text}>{w.icon}</span>
        <span className="wx-temps">
          <span className="wx-temp">{deg(w.temp_c)}</span>
          <span className="wx-text">{w.text}</span>
          {settings.showFeels && <span className="wx-feels dim">체감 {deg(w.feels_c)}</span>}
        </span>
        <span className="wx-range dim">
          <span className="wx-hi">{deg(w.high_c)}</span>
          <span className="wx-lo">{deg(w.low_c)}</span>
          {w.pop > 0 && <span className="wx-pop">💧{w.pop}%</span>}
        </span>
      </div>

      {showHourly && (
        <div className="wx-hours">
          {w.hourly.slice(0, hourSlots).map((h, i) => (
            <div className="wx-hour" key={`${h.hour}-${i}`}>
              <span className="wx-hour-time dim">{i === 0 ? "지금" : `${h.hour}시`}</span>
              <span className="wx-hour-temp">{deg(h.temp_c)}</span>
              <span className={`wx-hour-pop ${h.pop >= 40 ? "on" : "dim"}`}>
                {h.pop > 0 ? `${h.pop}%` : " "}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
