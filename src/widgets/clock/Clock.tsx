import { useEffect, useState } from "react";
import type { WidgetProps } from "../types";
import { DotDigits, fitCell } from "./DotDigits";
import "./Clock.css";

export interface ClockSettings extends Record<string, unknown> {
  hour12: boolean; seconds: boolean; dateFormat: "long" | "short" | "none"; timeZone: string;
  digitStyle: "system" | "dots"; dotChar: string;
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
  } catch { time = "시간대 오류"; }

  const dateH = date ? 20 : 0;
  const dots = settings.digitStyle === "dots" && /^[\d: ]+$/.test(time.replace(/\s?(am|pm)$/i, ""));
  if (dots) {
    // 12시간제의 am/pm 은 도트로 표현하지 않고 작은 글자로 옆에 붙인다
    const m = time.match(/^(.*?)\s?(am|pm)$/i);
    const digits = (m ? m[1] : time).trim();
    const cell = fitCell(digits, size.w - (m ? 28 : 0), size.h - dateH - 8);
    const dot = settings.dotChar?.trim() || "■";
    return (
      <div className="clock">
        <div className="clock-dots">
          <DotDigits text={digits} cell={cell} dot={dot} />
          {m && <span className="clock-ampm">{m[2].toUpperCase()}</span>}
        </div>
        {date && <div className="clock-date">{date}</div>}
      </div>
    );
  }

  const fontSize = Math.min((size.h - dateH) * 0.9, size.w / (settings.seconds ? 4.6 : 3.2) / (settings.hour12 ? 1.25 : 1));
  return (
    <div className="clock">
      <div className="clock-time" style={{ fontSize }}>{time}</div>
      {date && <div className="clock-date">{date}</div>}
    </div>
  );
}
