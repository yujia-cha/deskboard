import type { WidgetDefinition } from "../types";
import { Playtime, type PlaytimeSettings } from "./Playtime";

export const playtimeWidget: WidgetDefinition<PlaytimeSettings> = {
  id: "playtime",
  title: "플레이타임",
  icon: "🎮",
  component: Playtime,
  defaultSize: { w: 320, h: 180 },
  minSize: { w: 190, h: 90 },
  settingsSchema: [
    { key: "range", label: "기간", type: "select", default: "today",
      options: [{ value: "today", label: "오늘" }, { value: "week", label: "최근 7일" }] },
    // 게임은 분류 규칙이 알아서 잡는다. 그 밖의 프로그램만 여기에 등록한다.
    // 편집 모드의 "＋ 셀 프로그램 고르기" 로 최근 쓴 것 중에서 골라도 된다.
    { key: "extra", label: "함께 셀 프로그램 (쉼표로 여러 개)", type: "text", default: "",
      placeholder: "code, chrome, discord" },
    { key: "topCount", label: "차트에 표시할 개수", type: "number", default: 4, min: 1, max: 10, step: 1 },
    { key: "showSessions", label: "오늘의 구간 표시", type: "boolean", default: true },
  ],
};
