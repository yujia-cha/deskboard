import { useEffect, useState } from "react";
import type { WidgetProps } from "../types";
import { DotDigits, fitCell, type GridStyle } from "./DotDigits";
import { WordClock } from "./WordClock";
import { BinaryClock, FibonacciClock } from "./OddClocks";
import "./Clock.css";

export type DigitStyle = GridStyle | "system" | "word" | "binary" | "fibonacci";

export interface ClockSettings extends Record<string, unknown> {
  hour12: boolean; seconds: boolean; dateFormat: "long" | "short" | "mono" | "none"; timeZone: string;
  digitStyle: DigitStyle; dotChar: string; blockColor: string; glow: number; wordLang: "en" | "ko";
}

/** 격자 렌더러를 쓰는 스타일들 — 글리프만 다르고 파이프라인이 같다. */
const GRID_STYLES: GridStyle[] = ["blocks", "dots", "matrix", "neon"];
const isGrid = (s: DigitStyle): s is GridStyle => (GRID_STYLES as string[]).includes(s);

export function Clock({ settings, size }: WidgetProps<ClockSettings>) {
  const [now, setNow] = useState(() => new Date());
  // 5분 단위로만 바뀌는 시계들은 자주 깨울 필요가 없다.
  const coarse = settings.digitStyle === "word" || settings.digitStyle === "fibonacci";
  useEffect(() => {
    const period = coarse ? 10_000 : settings.seconds ? 1000 : 10_000;
    const t = setInterval(() => setNow(new Date()), period);
    return () => clearInterval(t);
  }, [settings.seconds, coarse]);

  const tz = settings.timeZone?.trim() || undefined;
  const color = settings.blockColor?.trim() || undefined;

  // 타임존이 지정되면 그 지역의 벽시계 시각을 쓰는 Date 를 만든다
  // (워드/바이너리/피보나치는 시·분·초 숫자를 직접 읽기 때문에 필요하다).
  let local = now;
  if (tz) {
    try {
      local = new Date(now.toLocaleString("en-US", { timeZone: tz }));
      if (Number.isNaN(local.getTime())) local = now;
    } catch { local = now; }
  }

  let time: string, date = "";
  try {
    time = new Intl.DateTimeFormat("en-GB", {
      hour: "2-digit", minute: "2-digit", second: settings.seconds ? "2-digit" : undefined,
      hour12: settings.hour12, timeZone: tz,
    }).format(now);
    if (settings.dateFormat === "long")
      date = new Intl.DateTimeFormat("ko-KR", { dateStyle: "full", timeZone: tz }).format(now);
    else if (settings.dateFormat === "short")
      date = new Intl.DateTimeFormat("sv-SE", { timeZone: tz }).format(now) +
        " (" + new Intl.DateTimeFormat("ko-KR", { weekday: "short", timeZone: tz }).format(now) + ")";
    else if (settings.dateFormat === "mono")
      date = new Intl.DateTimeFormat("en-US", { weekday: "short", month: "short", day: "numeric", timeZone: tz }).format(now)
        + (settings.hour12 ? " " + new Intl.DateTimeFormat("en-US", { hour: "numeric", hour12: true, timeZone: tz }).format(now).replace(/[^AP]*([AP]M)/i, "$1") : "");
  } catch { time = "시간대 오류"; }

  const dateH = date ? 22 : 0;
  const faceH = size.h - dateH - 8;
  const dateEl = date && <div className={`clock-date ${settings.dateFormat === "mono" ? "mono" : ""}`}>{date}</div>;

  // --- 숫자가 아닌 시계들 ---
  if (settings.digitStyle === "word")
    return (
      <div className="clock">
        <WordClock date={local} lang={settings.wordLang === "ko" ? "ko" : "en"}
          size={{ w: size.w, h: faceH }} color={color} />
        {dateEl}
      </div>
    );
  if (settings.digitStyle === "binary")
    return (
      <div className="clock">
        <BinaryClock date={local} size={{ w: size.w, h: faceH }} seconds={settings.seconds} color={color} />
        {dateEl}
      </div>
    );
  if (settings.digitStyle === "fibonacci")
    return (
      <div className="clock">
        <FibonacciClock date={local} size={{ w: size.w, h: faceH }} color={color} />
        {dateEl}
      </div>
    );

  // --- 격자 숫자 (블록·도트·매트릭스·네온) ---
  const m = time.match(/^(.*?)\s?(am|pm)$/i);
  const digits = (m ? m[1] : time).trim();

  if (isGrid(settings.digitStyle) && /^[\d: ]+$/.test(digits)) {
    const style = settings.digitStyle;
    const showAmPm = !!m && settings.dateFormat !== "mono";
    const cell = fitCell(digits, size.w - 8 - (showAmPm ? 28 : 0), faceH, style);
    return (
      <div className={`clock clock-${style}`}>
        <div className="clock-dots">
          <DotDigits text={digits} cell={cell} style={style} dot={settings.dotChar?.trim() || "■"}
            color={color} glow={(settings.glow ?? 60) / 100} />
          {showAmPm && <span className="clock-ampm">{m![2].toUpperCase()}</span>}
        </div>
        {dateEl}
      </div>
    );
  }

  const fontSize = Math.min(faceH * 0.9, size.w / (settings.seconds ? 4.6 : 3.2) / (settings.hour12 ? 1.25 : 1));
  return (
    <div className="clock">
      <div className="clock-time" style={{ fontSize }}>{time}</div>
      {dateEl}
    </div>
  );
}
