import { useEffect, useState } from "react";
import type { WidgetProps } from "../types";
import { DotDigits, fitCell } from "./DotDigits";
import "./Clock.css";

export interface ClockSettings extends Record<string, unknown> {
  hour12: boolean; seconds: boolean; dateFormat: "long" | "short" | "mono" | "none"; timeZone: string;
  digitStyle: "blocks" | "dots" | "system"; dotChar: string; blockColor: string;
}

export function Clock({ settings, size }: WidgetProps<ClockSettings>) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), settings.seconds ? 1000 : 10_000);
    return () => clearInterval(t);
  }, [settings.seconds]);

  const tz = settings.timeZone?.trim() || undefined;
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

  const style = settings.digitStyle === "system" ? "system" : settings.digitStyle === "dots" ? "dots" : "blocks";
  const dateH = date ? 22 : 0;
  const m = time.match(/^(.*?)\s?(am|pm)$/i);
  const digits = (m ? m[1] : time).trim();

  if (style !== "system" && /^[\d: ]+$/.test(digits)) {
    const showAmPm = !!m && settings.dateFormat !== "mono";
    const cell = fitCell(digits, size.w - 8 - (showAmPm ? 28 : 0), size.h - dateH - 8, style);
    return (
      <div className={`clock clock-${style}`}>
        <div className="clock-dots">
          <DotDigits text={digits} cell={cell} style={style} dot={settings.dotChar?.trim() || "■"} color={settings.blockColor?.trim() || undefined} />
          {showAmPm && <span className="clock-ampm">{m![2].toUpperCase()}</span>}
        </div>
        {date && <div className={`clock-date ${settings.dateFormat === "mono" ? "mono" : ""}`}>{date}</div>}
      </div>
    );
  }

  const fontSize = Math.min((size.h - dateH) * 0.9, size.w / (settings.seconds ? 4.6 : 3.2) / (settings.hour12 ? 1.25 : 1));
  return (
    <div className="clock">
      <div className="clock-time" style={{ fontSize }}>{time}</div>
      {date && <div className={`clock-date ${settings.dateFormat === "mono" ? "mono" : ""}`}>{date}</div>}
    </div>
  );
}
