import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invokeInOrder, useEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
import { fitGrid, padWeek, ROWS, type Day } from "./heatmap";
import "./Github.css";

export interface GithubSettings extends Record<string, unknown> {
  repos: string;
  showContributions: boolean;
  showRepos: boolean;
  showNotifications: boolean;
}

interface Contributions { total: number; weeks: Day[][] }
interface RepoStat {
  repo: string;
  open_prs: number; review_prs: number;
  open_issues: number; assigned_issues: number;
  notifications: number;
  /** success | failure | running | unknown | none */
  ci: string;
  branch: string;
}
interface Snapshot {
  login: string;
  review_requests: number;
  assigned_issues: number;
  notifications: number;
  contributions: Contributions | null;
  repos: RepoStat[];
  authed: boolean;
  error: string | null;
}

/**
 * classic 토큰 발급 화면. 스코프를 미리 채워 연다.
 *
 * fine-grained 토큰(`?type=beta`)을 쓰면 안 된다 — 리뷰 요청 PR 을 찾는 검색이
 * fine-grained 토큰에서는 403 을 돌려준다.
 *  - `repo`          : 비공개 저장소의 PR·이슈·Actions 를 보기 위해
 *  - `notifications` : 미확인 알림
 *  - `read:user`     : **기여도 달력**(GraphQL `contributionsCollection`) — 없으면 잔디만 비어 보인다
 */
const TOKEN_URL =
  "https://github.com/settings/tokens/new?scopes=repo,notifications,read:user&description=deskboard";

const reposOf = (s: GithubSettings) =>
  String(s.repos ?? "").split(",").map((r) => r.trim()).filter((r) => r.includes("/"));

const GAP = 2;

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

  // 좁아지면 아래에서부터 접는다 — 남길 순서는 숫자 → 잔디 → 저장소 목록.
  const showGrass = settings.showContributions && !!snap.contributions && size.h >= 110;
  const showRepos = settings.showRepos && snap.repos.length > 0 && size.h >= 150;

  return (
    <div className="gh">
      <div className="gh-counts">
        <Count n={snap.review_requests} label="리뷰 요청" title="내가 리뷰해야 할 열린 PR"
          onClick={() => open("https://github.com/pulls/review-requested")} />
        <Count n={snap.assigned_issues} label="담당 이슈" title="나에게 배정된 열린 이슈"
          onClick={() => open("https://github.com/issues/assigned")} />
        {settings.showNotifications && (
          <Count n={snap.notifications} label="알림" title="미확인 알림"
            onClick={() => open("https://github.com/notifications")} />
        )}
      </div>

      {showGrass && <Grass c={snap.contributions!} size={size} login={snap.login} onOpen={open} />}

      {snap.error && <div className="gh-error" title={snap.error}>{snap.error}</div>}

      {showRepos && (
        <div className="gh-repos">
          {snap.repos.map((r) => <RepoRow key={r.repo} r={r} onOpen={open} />)}
        </div>
      )}

      {settings.showRepos && snap.repos.length === 0 && !snap.error && size.h >= 150 && (
        <div className="dim gh-hint">설정에 owner/repo 를 넣으면 저장소별 현황이 보입니다</div>
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

function Count({ n, label, title, onClick }: { n: number; label: string; title: string; onClick: () => void }) {
  return (
    <button className="gh-count" title={title} onClick={onClick}>
      <span className={`gh-num ${n > 0 ? "on" : ""}`}>{n}</span>
      <span className="gh-label dim">{label}</span>
    </button>
  );
}

/** 기여도 달력. 열 = 주, 행 = 요일. 폭이 모자라면 **최근 주부터** 남긴다. */
function Grass({ c, size, login, onOpen }: { c: Contributions; size: { w: number; h: number }; login: string; onOpen: (u: string) => void }) {
  const { cell, cols } = fitGrid(size, GAP);
  const weeks = c.weeks.slice(-cols);
  const from = weeks[0]?.[0]?.date;
  return (
    <div className="gh-grass">
      <div
        className="gh-grid"
        style={{
          gridTemplateRows: `repeat(${ROWS}, ${cell}px)`,
          gridAutoColumns: `${cell}px`,
          gap: `${GAP}px`,
        }}
      >
        {weeks.map((w, wi) =>
          padWeek(w).map((d, di) => (
            <i
              key={`${wi}-${di}`}
              className={`gh-cell lv${d?.level ?? 0}`}
              title={d ? `${d.date} · ${d.count}회` : ""}
            />
          )),
        )}
      </div>
      <button className="gh-grass-total link" title={`전체 기여 ${c.total}회`}
        onClick={() => onOpen(`https://github.com/${login}`)}>
        {from ? `${from} 이후 ` : ""}{weeks.reduce((s, w) => s + w.reduce((a, d) => a + d.count, 0), 0)}회
        {c.total ? <span className="dim"> · 1년 {c.total}</span> : null}
      </button>
    </div>
  );
}

/**
 * 저장소 한 줄. **눈길을 끄는 것만 색을 쓴다** — 내 리뷰를 기다리는 PR, 미확인 알림, 깨진 CI.
 * 나머지 숫자(열린 PR·이슈)는 배경 정보라 흐리게 둔다.
 */
function RepoRow({ r, onOpen }: { r: RepoStat; onOpen: (u: string) => void }) {
  const name = r.repo.split("/")[1] ?? r.repo;
  const url = `https://github.com/${r.repo}`;
  return (
    <div className="gh-repo-row">
      {r.ci !== "none" && (
        <button className={`gh-dot state-${r.ci}`} title={`${r.branch || "기본 브랜치"} CI: ${r.ci}`}
          onClick={() => onOpen(`${url}/actions`)}>●</button>
      )}
      <button className="gh-repo-name" title={r.repo} onClick={() => onOpen(url)}>{name}</button>
      <span className="gh-badges">
        {r.review_prs > 0 && (
          <button className="gh-badge alert" title={`내 리뷰를 기다리는 PR ${r.review_prs}개`}
            onClick={() => onOpen(`${url}/pulls?q=is:open+is:pr+review-requested:@me`)}>
            리뷰 {r.review_prs}
          </button>
        )}
        {r.notifications > 0 && (
          <button className="gh-badge warn" title={`이 저장소의 미확인 알림 ${r.notifications}개`}
            onClick={() => onOpen(`https://github.com/notifications?query=repo%3A${encodeURIComponent(r.repo)}`)}>
            알림 {r.notifications}
          </button>
        )}
        {r.assigned_issues > 0 && (
          <button className="gh-badge alert" title={`나에게 배정된 이슈 ${r.assigned_issues}개`}
            onClick={() => onOpen(`${url}/issues/assigned`)}>
            담당 {r.assigned_issues}
          </button>
        )}
        <button className="gh-badge dim" title={`열린 PR ${r.open_prs}개`} onClick={() => onOpen(`${url}/pulls`)}>
          PR {r.open_prs}
        </button>
        <button className="gh-badge dim" title={`열린 이슈 ${r.open_issues}개`} onClick={() => onOpen(`${url}/issues`)}>
          이슈 {r.open_issues}
        </button>
      </span>
    </div>
  );
}
