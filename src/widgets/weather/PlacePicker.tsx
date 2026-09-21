import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../../core/settings";

interface Place { name: string; country: string; lat: number; lon: number }

/**
 * 도시 검색 → 좌표 저장. Open-Meteo Geocoding 이라 가입·키가 필요 없다.
 *
 * 설정 스키마(`SettingField`)는 값 하나짜리 입력만 다루는데 여기서는 한 번 고르면
 * 이름·위도·경도 세 개가 같이 정해져야 해서, 공용 스키마를 복잡하게 만드는 대신
 * 위젯 안에 둔다.
 */
export function PlacePicker({ instanceId, onDone }: { instanceId: string; onDone?: () => void }) {
  const update = useSettings((s) => s.updateWidgetSettings);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<Place[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = async () => {
    const q = query.trim();
    if (!q) return;
    setBusy(true);
    setError(null);
    try {
      setHits(await invoke<Place[]>("weather_search", { query: q }));
    } catch (e) {
      setError(String(e));
      setHits(null);
    } finally {
      setBusy(false);
    }
  };

  const choose = (p: Place) => {
    update(instanceId, { place: p.name, lat: p.lat, lon: p.lon });
    setHits(null);
    setQuery("");
    onDone?.();
  };

  return (
    <div className="wx-picker">
      <div className="wx-picker-row">
        <input
          type="text"
          value={query}
          placeholder="도시 이름 (서울, Busan…)"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") search(); }}
        />
        <button onClick={search} disabled={busy || !query.trim()}>{busy ? "…" : "검색"}</button>
      </div>
      {error && <div className="wx-error">{error}</div>}
      {hits?.length === 0 && <div className="dim">결과가 없습니다</div>}
      {hits && hits.length > 0 && (
        <div className="wx-picker-hits">
          {hits.map((p) => (
            <button key={`${p.lat},${p.lon}`} onClick={() => choose(p)}>
              {p.name} <span className="dim">{p.country}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
