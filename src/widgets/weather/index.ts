import type { WidgetDefinition } from "../types";
import { Weather, type WeatherSettings } from "./Weather";

export const weatherWidget: WidgetDefinition<WeatherSettings> = {
  id: "weather",
  title: "날씨",
  icon: "🌤️",
  component: Weather,
  defaultSize: { w: 300, h: 150 },
  minSize: { w: 180, h: 80 },
  settingsSchema: [
    // 도시 검색은 아래 place 필드에 이름을 넣으면 설정 패널이 좌표를 채워준다.
    { key: "place", label: "도시 (이름을 넣고 검색)", type: "text", default: "", placeholder: "서울, Busan, New York" },
    { key: "lat", label: "위도", type: "number", default: 0, min: -90, max: 90, step: 0.0001 },
    { key: "lon", label: "경도", type: "number", default: 0, min: -180, max: 180, step: 0.0001 },
    { key: "showHourly", label: "시간별 예보", type: "boolean", default: true },
    { key: "showFeels", label: "체감 온도", type: "boolean", default: true },
  ],
};
