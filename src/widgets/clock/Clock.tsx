import { useEffect, useState } from "react";
import type { WidgetProps } from "../types";
import "./Clock.css";

export interface ClockSettings extends Record<string, unknown> {
  hour12: boolean; seconds: boolean; dateFormat: "long" | "short" | "none"; timeZone: string;
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
    time = new Intl.DateTimeFormat("ko-KR", {
      hour: "2-digit", minute: "2-digit", second: settings.seconds ? "2-digit" : undefined,
      hour12: settings.hour12, timeZone: tz,
    }).format(now);
    if (settings.dateFormat === "long")
      date = new Intl.DateTimeFormat("ko-KR", { dateStyle: "full", timeZone: tz }).format(now);
    else if (settings.dateFormat === "short")
      date = new Intl.DateTimeFormat("sv-SE", { timeZone: tz }).format(now) +
        " (" + new Intl.DateTimeFormat("ko-KR", { weekday: "short", timeZone: tz }).format(now) + ")";
  } catch { time = "시간대 오류"; }

  const fontSize = Math.min(size.h * 0.45, size.w / (settings.seconds ? 5.2 : 3.8));
  return (
    <div className="clock">
      <div className="clock-time" style={{ fontSize }}>{time}</div>
      {date && <div className="clock-date">{date}</div>}
    </div>
  );
}
