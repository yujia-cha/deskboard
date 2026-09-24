import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  addExtra, daysAgo, durationShort, localDay, parseExtras,
  pickCandidates, prettyExe, removeExtra, useUsage,
} from "../../core/activity";

/**
 * "무엇을 셀지" 고르는 화면.
 *
 * 프로그램을 찾는 일이 이 화면의 전부다. 그래서:
 * - **범위가 다르다.** 위젯 본문은 오늘/7일을 보지만 여기서는 **최근 30일**을 뒤진다.
 *   어제 한 번 쓴 프로그램이 오늘 목록에 없다는 이유로 못 찾는 일이 없어야 한다.
 * - **검색창이 먼저다.** 실행 파일 이름(`code`)과 사람 이름(`VS Code`) 양쪽에 걸린다.
 * - **실행 파일 이름을 같이 보여 준다.** 비슷한 이름이 여럿일 때 이것만이 구별 수단이다.
 *
 * 30일 집계는 이 화면이 떠 있는 동안에만 읽는다 — 그래서 별도 컴포넌트다.
 */
export function ProgramPicker({ value, onChange, onClose }: {
  value: string;
  onChange: (next: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const { data } = useUsage(daysAgo(29), localDay());
  const extras = parseExtras(value);
  const rows = data?.rows ?? [];

  // 검색 전 후보 수 — "없음" 과 "검색어에 안 걸림" 은 다른 상황이다.
  const all = useMemo(() => pickCandidates(rows, extras, Infinity), [rows, value]); // eslint-disable-line react-hooks/exhaustive-deps
  const hits = useMemo(() => pickCandidates(rows, extras, 60, query), [rows, value, query]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="pt-pick">
      <div className="pt-pick-head">
        <span>셀 프로그램 고르기</span>
        <button className="link" onClick={onClose}>닫기</button>
      </div>

      <input
        className="pt-search"
        type="search"
        value={query}
        placeholder="프로그램 검색 (최근 30일)"
        onChange={(e) => setQuery(e.target.value)}
        autoFocus
      />

      {extras.length > 0 && (
        <div className="pt-pick-group">
          <span className="dim">등록됨 — 눌러서 빼기</span>
          <div className="pt-chips">
            {extras.map((e) => (
              <button key={e} className="pt-chip on" title={`${e}.exe`}
                onClick={() => onChange(removeExtra(value, e))}>
                {prettyExe(e)} ✕
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="pt-pick-group pt-pick-list">
        <span className="dim">
          {all.length === 0
            ? "아직 기록된 프로그램이 없습니다"
            : hits.length === 0
              ? `"${query}" 와 맞는 프로그램이 없습니다`
              : "눌러서 추가 — 많이 쓴 순서"}
        </span>
        {hits.map((c) => (
          <div key={c.exe} className="pt-row" role="button" tabIndex={0}
            onClick={() => onChange(addExtra(value, c.exe))}
            onKeyDown={(e) => {
              // 안쪽 🎮 버튼에서 누른 Enter 가 올라와 목록에까지 더하지 않게 — 행 자신일 때만.
              if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
              e.preventDefault();
              onChange(addExtra(value, c.exe));
            }}>
            <span className="pt-row-name">{prettyExe(c.exe)}</span>
            {prettyExe(c.exe).toLowerCase() !== c.exe.toLowerCase() && (
              <span className="dim pt-row-exe">{c.exe}.exe</span>
            )}
            <span className="dim pt-row-time">{durationShort(c.seconds)}</span>
            <button
              className="pt-row-game"
              title="게임으로 분류 — 모든 플레이타임 위젯에 자동으로 셈"
              onClick={(e) => {
                e.stopPropagation();
                invoke("activity_set_category", { exe: c.exe, category: "game" }).catch(console.warn);
              }}
            >
              🎮
            </button>
          </div>
        ))}
      </div>

      <span className="dim pt-pick-note">게임은 등록하지 않아도 자동으로 셉니다.</span>
      <span className="dim pt-pick-note">🎮 를 누르면 게임으로 분류합니다.</span>
    </div>
  );
}
