import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProviderData } from "../../core/ipc";
import { Gauge } from "../../components/Gauge";
import type { WidgetProps } from "../types";
import "./ClaudeUsage.css";

export interface ClaudeUsageSettings extends Record<string, unknown> {
  showOpus: boolean;
  showLocalCost: boolean;
  krwRate: number;
}

interface LimitWindow { utilization: number; resets_at: string | null }
interface Limits {
  ok: boolean; error: string | null; subscription: string | null;
  five_hour: LimitWindow | null; seven_day: LimitWindow | null; seven_day_opus: LimitWindow | null; seven_day_sonnet: LimitWindow | null;
  fetched_at: number;
}
interface Totals { requests: number; input: number; output: number; cache_write: number; cache_read: number; cost_usd: number }
interface UsageSummary { today: Totals; week: Totals }

export function fmtTokens(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return String(n);
}
export function fmtUsd(v: number): string {
  return v >= 100 ? `$${v.toFixed(0)}` : v >= 10 ? `$${v.toFixed(1)}` : `$${v.toFixed(2)}`;
}
export function shortModel(m: string): string {
  return m.replace(/^claude-/, "").replace(/-\d{8}$/, "");
}
/** 리셋까지 남은 시간. "2시간 10분", "3일 4시간" */
export function untilReset(iso: string | null, now = Date.now()): string {
  if (!iso) return "";
  const ms = new Date(iso).getTime() - now;
  if (!Number.isFinite(ms) || ms <= 0) return "곧 리셋";
  const m = Math.floor(ms / 60000), h = Math.floor(m / 60), d = Math.floor(h / 24);
  if (d >= 1) return `${d}일 ${h % 24}시간 후 리셋`;
  if (h >= 1) return `${h}시간 ${m % 60}분 후 리셋`;
  return `${m}분 후 리셋`;
}

export function ClaudeUsage({ settings, size }: WidgetProps<ClaudeUsageSettings>) {
  const limits = useProviderData<Limits>("claude_usage://limits", "get_claude_limits");
  const usage = useProviderData<UsageSummary>("claude_usage://update", "get_claude_usage");
  const [now, setNow] = useState(Date.now());
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), 30_000); return () => clearInterval(t); }, []);

  if (!limits || (!limits.ok && !limits.error)) return <div className="dim">한도 조회 중…</div>;

  const windows: { key: string; label: string; w: LimitWindow | null }[] = [
    { key: "5h", label: "5시간", w: limits.five_hour },
    { key: "7d", label: "주간 · 전체 모델", w: limits.seven_day },
    ...(settings.showOpus && limits.seven_day_opus ? [{ key: "7do", label: "주간 · Opus", w: limits.seven_day_opus }] : []),
  ];
  const gaugeSize = Math.max(44, Math.min(120, (size.w - 16) / windows.length - 20, size.h - 46));
  const ago = Math.round((now - limits.fetched_at) / 60000);

  return (
    <div className="cu">
      <div className="cu-gauges">
        {windows.map(({ key, label, w }) => (
          <div key={key} className="cu-gauge">
            <Gauge value={w ? w.utilization : null} label={label} size={gaugeSize} stroke={Math.max(6, gaugeSize * 0.1)} />
            <div className="cu-reset dim">{w ? untilReset(w.resets_at, now) : "—"}</div>
          </div>
        ))}
      </div>
      <div className="cu-foot dim">
        {limits.error
          ? <span className="cu-error" title={limits.error}>⚠ {limits.error}</span>
          : <span>{limits.subscription ? `${limits.subscription} · ` : ""}{ago <= 0 ? "방금 동기화" : `${ago}분 전 동기화`}</span>}
        <button title="새로고침" onClick={() => invoke("refresh_claude_limits").catch(() => {})}>↻</button>
      </div>
      {settings.showLocalCost && usage && (
        <div className="cu-local dim">
          오늘 {fmtUsd(usage.today.cost_usd)}{settings.krwRate > 0 && ` (₩${Math.round(usage.today.cost_usd * settings.krwRate).toLocaleString()})`} · {fmtTokens(usage.today.output)} out
          &nbsp;·&nbsp; 7일 {fmtUsd(usage.week.cost_usd)}
        </div>
      )}
    </div>
  );
}
