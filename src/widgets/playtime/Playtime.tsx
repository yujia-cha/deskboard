import { useState } from "react";
import { Donut } from "../../components/Donut";
import { useSettings } from "../../core/settings";
import {
  daysAgo, duration, durationShort, hhmm, localDay, parseExtras,
  pickTracked, prettyExe, share, sliceColor, useSessions, useUsage,
} from "../../core/activity";
import { ProgramPicker } from "./ProgramPicker";
import type { WidgetProps } from "../types";
import "./Playtime.css";

export interface PlaytimeSettings extends Record<string, unknown> {
  range: "today" | "week";
  /** 게임 외에 추가로 볼 프로그램 (쉼표 구분). */
  extra: string;
  showSessions: boolean;
  topCount: number;
}

/**
 * 플레이타임 — 게임은 자동으로, 그 밖의 프로그램은 등록한 것만 함께 센다.
 *
 * 게임만 따로, 앱만 따로 두 위젯으로 나누면 같은 데이터를 두 번 보게 된다.
 * 하나로 합치고 "무엇을 셀지"만 사용자가 정하게 했다.
 */
export function Playtime({ instanceId, settings, size, editing }: WidgetProps<PlaytimeSettings>) {
  const update = useSettings((s) => s.updateWidgetSettings);
  const today = localDay();
  const from = settings.range === "week" ? daysAgo(6) : today;
  const { data, error } = useUsage(from, today);
  const { data: sessions } = useSessions(today, 12);
  const [picking, setPicking] = useState(false);

  const extras = parseExtras(String(settings.extra ?? ""));

  if (error) return <div className="pt-error">{error}</div>;
  if (!data) return <div className="dim">기록을 읽는 중…</div>;

  const tracked = pickTracked(data.rows, extras);
  const total = tracked.reduce((n, r) => n + r.seconds, 0);
  const top = tracked.slice(0, Math.max(1, Math.round(settings.topCount) || 4));
  // 목록에 없는 나머지는 하나로 묶어 도넛이 100% 를 채우게 한다
  const restSeconds = total - top.reduce((n, r) => n + r.seconds, 0);

  const slices = [
    ...top.map((r, i) => ({ value: r.seconds, color: sliceColor(i), label: prettyExe(r.exe) })),
    ...(restSeconds > 0 ? [{ value: restSeconds, color: "var(--track)", label: "그 외" }] : []),
  ];

  const ring = Math.max(52, Math.min(size.h - 26, size.w * 0.38, 120));
  const showDonut = size.w >= 220 && size.h >= 110;
  const trackedSessions = (sessions ?? []).filter((s) =>
    s.category === "game" || extras.includes(s.exe.toLowerCase()));
  const showSessions = settings.showSessions && size.h >= 210 && trackedSessions.length > 0;

  // --- 등록 화면 ---
  if (picking) {
    return (
      <ProgramPicker
        value={String(settings.extra ?? "")}
        onChange={(extra) => update(instanceId, { extra })}
        onClose={() => setPicking(false)}
      />
    );
  }

  if (tracked.length === 0) {
    return (
      <div className="pt-empty dim">
        <div>아직 기록이 없습니다.</div>
        <div>게임은 자동으로 세고, 그 밖의 프로그램은 등록하면 함께 셉니다.</div>
        <button className="link" onClick={() => setPicking(true)}>＋ 프로그램 찾아서 등록</button>
      </div>
    );
  }

  return (
    <div className="pt">
      <div className="pt-main">
        {showDonut && (
          <Donut slices={slices} size={ring} stroke={Math.max(8, ring * 0.15)}
            center={durationShort(total)} sub={settings.range === "week" ? "7일" : "오늘"} />
        )}
        <div className="pt-legend">
          {top.map((r, i) => (
            <div className="pt-item" key={r.exe} title={`${r.exe} · ${duration(r.seconds)}`}>
              <span className="pt-dot" style={{ background: sliceColor(i) }} />
              <span className="pt-name">{prettyExe(r.exe)}</span>
              <span className="pt-value">{durationShort(r.seconds)}</span>
              <span className="pt-pct dim">{Math.round(share(r.seconds, total) * 100)}%</span>
            </div>
          ))}
          {restSeconds > 0 && (
            <div className="pt-item dim">
              <span className="pt-dot" style={{ background: "var(--track)" }} />
              <span className="pt-name">그 외</span>
              <span className="pt-value">{durationShort(restSeconds)}</span>
              <span className="pt-pct">{Math.round(share(restSeconds, total) * 100)}%</span>
            </div>
          )}
        </div>
      </div>

      {showSessions && (
        <div className="pt-sessions">
          {trackedSessions.slice(0, 3).map((s, i) => (
            <div className="pt-session" key={i}>
              <span className="dim">{hhmm(s.started_at)}–{hhmm(s.ended_at)}</span>
              <span className="pt-session-name">{prettyExe(s.exe)}</span>
              <span className="dim">{durationShort(s.seconds)}</span>
            </div>
          ))}
        </div>
      )}

      {/* 프로그램을 더하는 길은 편집 모드에 가두지 않는다 — 평소에는 비켜 있다가 커서를 올리면 보인다. */}
      <button className={`pt-edit link ${editing ? "always" : ""}`} onClick={() => setPicking(true)}>
        ＋ 셀 프로그램 고르기
      </button>
    </div>
  );
}
