import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invokeInOrder, useEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
import "./Github.css";

export interface GithubSettings extends Record<string, unknown> {
  repos: string;
  showNotifications: boolean;
  showChecks: boolean;
}

interface Check { repo: string; name: string; state: string; branch: string }
interface Snapshot {
  review_requests: number;
  notifications: number;
  checks: Check[];
  authed: boolean;
  error: string | null;
}

const STATE_ICON: Record<string, string> = {
  success: "●", failure: "●", cancelled: "●", running: "◐", skipped: "○", unknown: "○",
};

/**
 * classic 토큰 발급 화면. 스코프를 미리 채워 연다.
 *
 * fine-grained 토큰(`?type=beta`)을 쓰면 안 된다 — 리뷰 요청 PR 을 찾는 `/search/issues` 가
 * fine-grained 토큰에서는 403 을 돌려준다.
 *  - `repo`          : 비공개 저장소의 PR·Actions 를 보기 위해
 *  - `notifications` : 미확인 알림 개수
 */
const TOKEN_URL =
  "https://github.com/settings/tokens/new?scopes=repo,notifications&description=deskboard";

const reposOf = (s: GithubSettings) =>
  String(s.repos ?? "").split(",").map((r) => r.trim()).filter((r) => r.includes("/"));

export function Github({ settings, size, editing }: WidgetProps<GithubSettings>) {
  const repos = reposOf(settings);
  // 커맨드 결과와 provider 이벤트를 **한 상태에** 모은다. 둘을 따로 들고
  // `이벤트 ?? 커맨드결과` 로 고르면, 토큰이 없던 시절에 온 이벤트가 로그인 직후의
  // 커맨드 결과를 계속 가려서 "토큰을 넣어도 아무 변화가 없는" 것처럼 보인다.
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<Snapshot>("github_fetch").then(setSnap).catch((e) => setErr(String(e)));
  }, []);

  const key = repos.join(",");
  useEffect(() => {
    invokeInOrder("github_set_active", { active: true, repos: key ? key.split(",") : [] });
    refresh();
    return () => { invokeInOrder("github_set_active", { active: false, repos: [] }); };
  }, [key, refresh]);

  useEvent("github://changed", refresh);
  useEvent<Snapshot>("github://update", setSnap);

  const open = (url: string) => openUrl(url).catch(console.warn);

  if (!snap) return <div className="dim">읽는 중…</div>;

  if (!snap.authed) {
    return (
      <div className="gh-login">
        <div className="gh-login-title">GitHub 토큰</div>
        <p className="dim">
          <button className="link" onClick={() => open(TOKEN_URL)}>
            토큰 만들기
          </button>
          {" "}— 스코프가 미리 선택된 채로 열립니다. 토큰은 이 PC 계정으로만 풀 수 있게
          {" "}암호화해 저장합니다.
        </p>
        <div className="gh-login-row">
          <input
            type="password"
            value={token}
            placeholder="토큰 붙여넣기"
            onChange={(e) => setToken(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") save(); }}
          />
          <button onClick={save} disabled={busy || !token.trim()}>{busy ? "…" : "저장"}</button>
        </div>
        {/* 토큰이 거부되면 백엔드가 authed=false 로 되돌려 이 화면이 다시 뜬다 —
            왜 실패했는지 여기서 보여줘야 다시 넣어볼 수 있다. */}
        {(err ?? snap.error) && <div className="gh-error">{err ?? snap.error}</div>}
      </div>
    );
  }

  const checks = settings.showChecks ? snap.checks : [];
  const showChecks = checks.length > 0 && size.h >= 130;

  return (
    <div className="gh">
      <div className="gh-counts">
        <button className="gh-count" title="내가 리뷰해야 할 열린 PR"
          onClick={() => open("https://github.com/pulls/review-requested")}>
          <span className={`gh-num ${snap.review_requests > 0 ? "on" : ""}`}>{snap.review_requests}</span>
          <span className="gh-label dim">리뷰 요청</span>
        </button>
        {settings.showNotifications && (
          <button className="gh-count" title="미확인 알림"
            onClick={() => open("https://github.com/notifications")}>
            <span className={`gh-num ${snap.notifications > 0 ? "on" : ""}`}>{snap.notifications}</span>
            <span className="gh-label dim">알림</span>
          </button>
        )}
      </div>

      {snap.error && <div className="gh-error">{snap.error}</div>}

      {showChecks && (
        <div className="gh-checks">
          {checks.map((c, i) => (
            <button key={i} className="gh-check" title={`${c.repo} · ${c.name} · ${c.branch}`}
              onClick={() => open(`https://github.com/${c.repo}/actions`)}>
              <span className={`gh-dot state-${c.state}`}>{STATE_ICON[c.state] ?? "○"}</span>
              <span className="gh-repo">{c.repo.split("/")[1] ?? c.repo}</span>
              <span className="gh-branch dim">{c.branch}</span>
            </button>
          ))}
        </div>
      )}

      {settings.showChecks && checks.length === 0 && !snap.error && size.h >= 130 && (
        <div className="dim gh-hint">설정에 owner/repo 를 넣으면 CI 상태가 보입니다</div>
      )}

      {editing && (
        <button className="gh-logout link"
          onClick={() => invoke("github_logout").then(refresh).catch((e) => setErr(String(e)))}>
          토큰 지우기
        </button>
      )}
    </div>
  );

  function save() {
    const t = token.trim();
    if (!t) return;
    setBusy(true);
    setErr(null);
    invoke("github_set_token", { token: t })
      .then(() => { setToken(""); refresh(); })
      .catch((e) => setErr(String(e)))
      .finally(() => setBusy(false));
  }
}
