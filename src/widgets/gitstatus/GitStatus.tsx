import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { invokeInOrder, useProviderEvent } from "../../core/ipc";
import type { WidgetProps } from "../types";
import "./GitStatus.css";

export interface GitStatusSettings extends Record<string, unknown> {
  root: string;
  root2: string;
  onlyDirty: boolean;
  showClean: boolean;
  maxRepos: number;
}

interface Repo {
  name: string; path: string; branch: string;
  dirty: number; ahead: number; behind: number; tracked: boolean; last_commit: string;
}
interface Snapshot { repos: Repo[]; error: string | null }

const rootsOf = (s: GitStatusSettings) =>
  [String(s.root ?? ""), String(s.root2 ?? "")].map((r) => r.trim()).filter(Boolean);

/** "3일 전" 처럼. 형식이 이상하면 빈 문자열. */
function ago(iso: string): string {
  if (!iso) return "";
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return "";
  const mins = Math.floor((Date.now() - t) / 60000);
  if (mins < 1) return "방금";
  if (mins < 60) return `${mins}분 전`;
  const h = Math.floor(mins / 60);
  if (h < 24) return `${h}시간 전`;
  const d = Math.floor(h / 24);
  return d < 30 ? `${d}일 전` : `${Math.floor(d / 30)}달 전`;
}

export function GitStatus({ settings, size }: WidgetProps<GitStatusSettings>) {
  const roots = rootsOf(settings);
  const pushed = useProviderEvent<Snapshot>("git://changed");
  const [initial, setInitial] = useState<Snapshot | null>(null);

  useEffect(() => {
    invokeInOrder("git_set_roots", { active: roots.length > 0, roots });
    if (roots.length > 0) {
      invoke<Snapshot>("git_status", { roots, depth: 2 }).then(setInitial).catch(console.warn);
    }
    return () => { invokeInOrder("git_set_roots", { active: false, roots: [] }); };
    // roots 는 문자열 배열이라 join 으로 비교한다
  }, [roots.join("|")]);

  const snap = pushed ?? initial;

  if (roots.length === 0) {
    return (
      <div className="gs-setup dim">
        위젯 설정에서 저장소가 모여 있는 폴더를 지정하세요.
        {" "}그 아래 2단계까지 훑어 `.git` 을 찾습니다.
      </div>
    );
  }
  if (!snap) return <div className="dim">저장소를 읽는 중…</div>;
  if (snap.error) return <div className="gs-error">{snap.error}</div>;

  const pending = snap.repos.filter((r) => r.dirty > 0 || r.ahead > 0 || r.behind > 0);
  const list = settings.onlyDirty ? pending : snap.repos;
  const shown = list.slice(0, Math.max(1, Math.round(settings.maxRepos) || 6));
  const showBranch = size.w >= 260;

  return (
    <div className="gs">
      <div className="gs-head">
        <span className="gs-count">
          {pending.length > 0
            ? <><strong>{pending.length}</strong> 개 손댈 것</>
            : "전부 깨끗합니다"}
        </span>
        <span className="dim gs-total">{snap.repos.length}개</span>
      </div>

      {shown.length === 0 ? (
        <div className="dim gs-empty">
          {settings.onlyDirty ? "변경된 저장소가 없습니다" : "저장소를 찾지 못했습니다"}
        </div>
      ) : (
        <div className="gs-list">
          {shown.map((r) => (
            <button key={r.path} className="gs-row" title={`${r.path}\n${r.branch}${r.last_commit ? ` · 마지막 커밋 ${ago(r.last_commit)}` : ""}`}
              onClick={() => invoke("git_open", { path: r.path }).catch(console.warn)}>
              <span className="gs-name">{r.name}</span>
              {showBranch && <span className="gs-branch dim">{r.branch}</span>}
              <span className="gs-marks">
                {r.dirty > 0 && <span className="gs-dirty" title="커밋 안 한 변경">●{r.dirty}</span>}
                {r.ahead > 0 && <span className="gs-ahead" title="푸시 안 한 커밋">↑{r.ahead}</span>}
                {r.behind > 0 && <span className="gs-behind" title="받아야 할 커밋">↓{r.behind}</span>}
                {!r.tracked && <span className="dim" title="원격 추적 브랜치 없음">—</span>}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
