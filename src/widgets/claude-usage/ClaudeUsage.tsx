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
  ok: boolean; logged_in: boolean; error: string | null;
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

  if (!limits || limits.fetched_at === 0) return <div className="dim">한도 조회 중…</div>;
  if (!limits.logged_in) return <Login />;

  const windows: { key: string; label: string; w: LimitWindow | null }[] = [
    { key: "5h", label: "5시간", w: limits.five_hour },
    { key: "7d", label: "주간 · 전체 모델", w: limits.seven_day },
    ...(settings.showOpus && limits.seven_day_opus ? [{ key: "7do", label: "주간 · Opus", w: limits.seven_day_opus }] : []),
  ];
  const gaugeSize = Math.max(44, Math.min(120, (size.w - 16) / windows.length - 20, size.h - 46 - (settings.showLocalCost ? 22 : 0)));
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
          : <span>{ago <= 0 ? "방금 동기화" : `${ago}분 전 동기화`}</span>}
        <span>
          <button title="새로고침" onClick={() => invoke("refresh_claude_limits").catch(() => {})}>↻</button>
          <button title="로그아웃" onClick={() => invoke("claude_logout").catch(() => {})}>⏻</button>
        </span>
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

/** 브라우저 승인 → 코드 붙여넣기 방식 로그인 (CLI 불필요). */
function Login() {
  const [started, setStarted] = useState(false);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const start = async () => {
    setErr(null);
    try { await invoke<string>("claude_login_start"); setStarted(true); } catch (e) { setErr(String(e)); }
  };
  const finish = async () => {
    if (!code.trim()) return;
    setBusy(true); setErr(null);
    try { await invoke("claude_login_finish", { code }); setCode(""); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="cu-login">
      {!started ? (
        <>
          <div className="dim">claude.ai 계정으로 로그인하면 5시간 / 주간 한도를 표시합니다.</div>
          <button className="primary" onClick={start}>브라우저로 로그인</button>
        </>
      ) : (
        <>
          <div className="dim">브라우저에서 승인하면 코드가 표시됩니다. 복사해서 붙여넣으세요.</div>
          <div className="cu-login-row">
            <input placeholder="코드 붙여넣기" value={code} onChange={(e) => setCode(e.target.value)} onKeyDown={(e) => e.key === "Enter" && finish()} autoFocus />
            <button className="primary" onClick={finish} disabled={busy || !code.trim()}>{busy ? "…" : "연결"}</button>
          </div>
          <button className="link" onClick={start}>승인 페이지 다시 열기</button>
        </>
      )}
      {err && <div className="cu-error" title={err}>⚠ {err}</div>}
    </div>
  );
}
