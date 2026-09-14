import { useProviderData } from "../../core/ipc";
import { Sparkline } from "../../components/Sparkline";
import type { WidgetProps } from "../types";
import "./ClaudeUsage.css";

export interface ClaudeUsageSettings extends Record<string, unknown> {
  period: "today" | "week" | "month" | "session";
  showModels: boolean;
  showChart: boolean;
  krwRate: number; // 0 이면 USD 만 표시
}

interface Totals { requests: number; input: number; output: number; cache_write: number; cache_read: number; cost_usd: number }
interface UsageSummary {
  today: Totals; week: Totals; month: Totals; all_time: Totals; current_session: Totals;
  daily: (Totals & { date: string })[];
  by_model: (Totals & { model: string; priced: boolean })[];
  current_session_id: string; last_activity: string | null; unpriced_models: string[]; transcripts_dir: string;
}

const PERIOD_LABEL = { today: "오늘", week: "최근 7일", month: "최근 30일", session: "현재 세션" } as const;

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

export function ClaudeUsage({ settings, size }: WidgetProps<ClaudeUsageSettings>) {
  const s = useProviderData<UsageSummary>("claude_usage://update", "get_claude_usage");
  if (!s) return <div className="dim">불러오는 중…</div>;

  const t: Totals = settings.period === "session" ? s.current_session : s[settings.period];
  const compact = size.h < 200;
  const last7 = s.daily.slice(-7);
  const week = s.daily.slice(-7).map((d) => d.cost_usd);
  const ago = s.last_activity ? relTime(new Date(s.last_activity)) : "기록 없음";
  const maxModelCost = Math.max(1e-9, ...s.by_model.map((m) => m.cost_usd));

  return (
    <div className="cu">
      <div className="cu-head">
        <div>
          <div className="cu-cost">{fmtUsd(t.cost_usd)}
            {settings.krwRate > 0 && <span className="cu-krw"> ≈ ₩{Math.round(t.cost_usd * settings.krwRate).toLocaleString()}</span>}
          </div>
          <div className="dim">{PERIOD_LABEL[settings.period]} · {t.requests.toLocaleString()}회 · 마지막 {ago}</div>
        </div>
        <div className="cu-tokens">
          <div><span className="dim">입력</span> {fmtTokens(t.input)}</div>
          <div><span className="dim">출력</span> {fmtTokens(t.output)}</div>
          <div><span className="dim">캐시</span> {fmtTokens(t.cache_read)}<span className="dim">r</span> {fmtTokens(t.cache_write)}<span className="dim">w</span></div>
        </div>
      </div>

      {!compact && settings.showChart && (
        <div className="cu-chart" title={last7.map((d) => `${d.date}: ${fmtUsd(d.cost_usd)}`).join("\n")}>
          <Sparkline values={week} height={34} />
          <div className="cu-days">{last7.map((d) => <span key={d.date}>{d.date.slice(8)}</span>)}</div>
        </div>
      )}

      {!compact && settings.showModels && (
        <div className="cu-models">
          {s.by_model.slice(0, 4).map((m) => (
            <div key={m.model} className="cu-model" title={`${m.model}: ${fmtTokens(m.output)} out / ${m.requests}회`}>
              <span className="cu-model-name">{shortModel(m.model)}{!m.priced && " (가격 미등록)"}</span>
              <span className="cu-bar"><span style={{ width: `${(m.cost_usd / maxModelCost) * 100}%` }} /></span>
              <span className="cu-model-cost">{fmtUsd(m.cost_usd)}</span>
            </div>
          ))}
          {s.by_model.length === 0 && <div className="dim">최근 30일 기록 없음 ({s.transcripts_dir})</div>}
        </div>
      )}
    </div>
  );
}

function relTime(d: Date): string {
  const m = Math.round((Date.now() - d.getTime()) / 60000);
  if (m < 1) return "방금";
  if (m < 60) return `${m}분 전`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h}시간 전`;
  return `${Math.round(h / 24)}일 전`;
}
