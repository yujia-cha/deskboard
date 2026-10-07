import { useCallback, useEffect, useState } from "react";
import { call as invoke, invokeInOrder, openSettings, openUrl, useEvent, type WidgetProps } from "deskboard";
import { ago, safeUrl, sourceOf, type ScrapItem } from "./format";
import "./Scrap.css";

export interface ScrapSettings extends Record<string, unknown> {
  backend: string;
  prompt: string;
  count: number;
  recency: string;
  language: string;
  refresh: string;
  dailyHour: number;
  model: string;
  effort: string;
  maxSearches: number;
  domainMode: string;
  domains: string;
  blockedDomains: string;
}

/** 백엔드(`providers/scrap`)가 인스턴스마다 들고 있는 상태. */
interface ScrapState {
  items: ScrapItem[];
  /** 마지막으로 성공한 시각 (unix ms) */
  fetchedAt: number | null;
  running: boolean;
  error: string | null;
  usage: { inputTokens: number; outputTokens: number; webSearches: number } | null;
  /** 어느 인증으로 불렀는가 */
  auth: "key" | "cli" | "rss" | null;
  model: string | null;
  /** 대체 백엔드로 가져왔다 등의 안내 */
  note: string | null;
  /** 저장된 결과가 지금 주제와 다른 주제로 찾은 것이다 */
  promptChanged: boolean;
}

interface AuthStatus { hasKey: boolean; hasLogin: boolean; cli: { found: boolean; path: string | null } }

const AUTH_LABEL = { key: "API 키", cli: "Claude Code (구독)", rss: "RSS (요약 없음)" } as const;

const KEY_URL = "https://console.anthropic.com/settings/keys";

const kTokens = (n: number) => (n >= 1000 ? `${Math.round(n / 100) / 10}k` : String(n));

export function Scrap({ instanceId, settings, size }: WidgetProps<ScrapSettings>) {
  const [state, setState] = useState<ScrapState | null>(null);
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [showKey, setShowKey] = useState(false);

  const loadAuth = useCallback(() => {
    invoke<AuthStatus>("scrap_status").then(setAuth).catch(() => setAuth({ hasKey: false, hasLogin: false, cli: { found: false, path: null } }));
  }, []);

  // 스케줄러에 이 인스턴스를 올린다 — 주기·주제가 바뀌면 다시 알린다. 떠 있는 동안만 자동 갱신한다.
  const settingsKey = JSON.stringify(settings);
  useEffect(() => {
    invokeInOrder("scrap_set_active", () => ({ instanceId, active: true, settings: JSON.parse(settingsKey) }));
    invoke<ScrapState>("scrap_get", { instanceId }).then(setState).catch(console.warn);
    return () => { invokeInOrder("scrap_set_active", { instanceId, active: false, settings: null }); };
  }, [instanceId, settingsKey]);
  useEffect(loadAuth, [loadAuth]);

  // 인증이 바뀌면(키 입력·로그인) 상태를 다시 읽고, 백엔드에 바로 판단하게 한다 —
  // RSS 로 받아 둔 결과가 있으면 주기를 기다리지 않고 Claude 로 한 번 다시 찾는다 (`upgrade_available`).
  const onAuthChanged = useCallback(() => {
    loadAuth();
    invoke("scrap_wake").catch(() => {});
  }, [loadAuth]);

  useEvent<ScrapState>(`scrap://update/${instanceId}`, setState);
  useEvent("scrap://auth", onAuthChanged);
  // Claude 한도 위젯에서 로그인/로그아웃해도 같이 따라간다
  useEvent("claude_usage://limits", onAuthChanged);

  const refresh = () => {
    invoke("scrap_refresh", { instanceId, settings }).catch((e) => setState((s) => (s ? { ...s, error: String(e) } : s)));
  };

  if (!auth || !state) return <div className="dim">읽는 중…</div>;

  if (showKey) {
    return <AuthPanel status={auth} onDone={() => { setShowKey(false); loadAuth(); }} />;
  }

  if (!settings.prompt.trim()) {
    return (
      <div className="scrap-empty">
        <p>무엇을 모을지 적어 주세요.</p>
        <p className="dim">예: "Rust 생태계 새 소식", "국내 AI 정책 동향", "이번 주 프런트엔드 릴리스"</p>
        <button className="primary" onClick={() => openSettings(instanceId)}>주제 정하기</button>
      </div>
    );
  }

  const compact = size.h < 220;
  return (
    <div className="scrap">
      <div className="scrap-head">
        <span className="scrap-topic" title={settings.prompt}>{settings.prompt}</span>
        <span className="dim scrap-when">{state.running ? "찾는 중…" : state.fetchedAt ? ago(state.fetchedAt) : ""}</span>
        <button title="지금 새로 찾기" disabled={state.running} onClick={refresh}>↻</button>
      </div>

      {state.error && <div className="scrap-error" title={state.error}>{state.error}</div>}
      {state.note && <div className="scrap-note-line dim" title={state.note}>{state.note}</div>}
      {state.promptChanged && !state.running && (
        <button className="scrap-stale" onClick={refresh}>주제가 바뀌었습니다 — 새로 찾기</button>
      )}

      <ol className="scrap-list">
        {state.items.length === 0 && !state.running && <li className="dim">아직 결과가 없습니다. ↻ 를 눌러 찾아보세요.</li>}
        {state.items.map((it, i) => {
          const url = safeUrl(it.url);
          return (
            <li key={`${i}-${it.url}`}>
              {/* 웹에서 온 글이다 — HTML 로 그리지 않고 텍스트로만, 링크는 http/https 만 */}
              {url
                ? <a href={url} onClick={(e) => { e.preventDefault(); openUrl(url).catch(console.warn); }} title={url}>{it.title}</a>
                : <span>{it.title}</span>}
              <div className="scrap-meta dim">
                {sourceOf(it)}{it.published ? ` · ${it.published}` : ""}
              </div>
              {!compact && it.summary && <p className="scrap-summary">{it.summary}</p>}
            </li>
          );
        })}
      </ol>

      <div className="scrap-foot dim">
        <span>
          {state.auth ? AUTH_LABEL[state.auth] ?? "" : ""}
          {state.usage ? `${state.usage.webSearches > 0 ? ` · 검색 ${state.usage.webSearches}회` : ""} · 토큰 ${kTokens(state.usage.inputTokens + state.usage.outputTokens)}` : ""}
          {state.auth === "rss" && !auth.hasLogin && !auth.hasKey ? " · 🔑 에서 Claude 로그인하면 요약까지 받아요" : ""}
        </span>
        <button title="가져오는 곳 설정" onClick={() => setShowKey(true)}>🔑</button>
      </div>
    </div>
  );
}

/** 가져오는 곳의 인증 — Claude 구독 로그인(Claude 한도 위젯과 공유, 브라우저 승인 → 코드 붙여넣기)과 API 키(Windows DPAPI 로 암호화해 저장, 이 PC 의 이 계정에서만 풀린다). */
function AuthPanel({ status, onDone }: { status: AuthStatus; onDone: () => void }) {
  const [key, setKey] = useState("");
  const [code, setCode] = useState("");
  const [started, setStarted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const run = (cmd: string, args: Record<string, unknown>, after: () => void) => {
    setBusy(true);
    setErr(null);
    invoke(cmd, args).then(after).catch((e) => setErr(String(e))).finally(() => setBusy(false));
  };
  const { hasKey, hasLogin, cli } = status;
  const saveKey = () => { if (key.trim()) run("scrap_set_key", { key: key.trim() }, () => { setKey(""); onDone(); }); };
  const finishLogin = () => { if (code.trim()) run("claude_login_finish", { code }, () => { setCode(""); setStarted(false); onDone(); }); };
  const startLogin = () => run("claude_login_start", {}, () => setStarted(true));
  return (
    <div className="scrap-key">
      <strong>Claude 구독 로그인 (추천, 무료)</strong>
      {hasLogin ? (
        <>
          <span className="scrap-key-row">
            <span className="scrap-note">로그인됨</span>
            <button disabled={busy} onClick={() => run("claude_logout", {}, onDone)}>로그아웃</button>
          </span>
          <p className="dim scrap-note">Claude 한도 위젯도 함께 로그아웃됩니다.</p>
        </>
      ) : !started ? (
        <button className="primary" disabled={busy} onClick={startLogin}>브라우저로 로그인</button>
      ) : (
        <>
          <div className="scrap-key-row">
            <input value={code} placeholder="코드 붙여넣기" autoFocus onChange={(e) => setCode(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") finishLogin(); }} />
            <button className="primary" disabled={busy || !code.trim()} onClick={finishLogin}>확인</button>
          </div>
          <p className="dim scrap-note">승인 페이지에 나온 코드를 붙여 넣으세요</p>
        </>
      )}
      <p className="dim scrap-note scrap-path" title={cli.path ?? undefined}>
        {cli.found ? cli.path : "Claude 데스크톱 앱의 Claude Code 를 찾지 못했습니다 — 이 경우 RSS 로 가져옵니다"}
      </p>

      <strong>Claude API 키 (선택, 유료)</strong>
      <p className="dim">
        웹 검색과 요약에 Claude API 를 씁니다.{" "}
        <a href={KEY_URL} onClick={(e) => { e.preventDefault(); openUrl(KEY_URL).catch(console.warn); }}>키 만들기</a>
      </p>
      <div className="scrap-key-row">
        <input type="password" value={key} placeholder={hasKey ? "저장된 키가 있습니다 — 바꾸려면 입력" : "sk-ant-…"}
          onChange={(e) => setKey(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") saveKey(); }} />
        <button className="primary" disabled={busy || !key.trim()} onClick={saveKey}>저장</button>
      </div>
      {hasKey && <span className="scrap-key-row"><button onClick={() => run("scrap_clear_key", {}, onDone)}>저장된 키 지우기</button></span>}

      {err && <p className="scrap-error" title={err}>{err}</p>}
      <p className="dim scrap-note">'자동' 은 API 키 → Claude 로그인(Claude Code) → RSS 순으로 쓸 수 있는 것부터 시도하고, 실패하면 다음으로 넘어갑니다. 설정에서 한 가지로 고정할 수도 있습니다.</p>
      <span className="scrap-key-row"><button onClick={onDone}>닫기</button></span>
    </div>
  );
}
