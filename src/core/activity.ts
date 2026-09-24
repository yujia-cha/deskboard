import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { invokeInOrder, useEvent } from "./ipc";

/**
 * 활동 추적 공용 코드.
 *
 * 위젯 셋(플레이타임·앱 사용시간·일일 회고)이 같은 provider 를 본다. 하나라도 떠 있으면
 * 폴링이 돌고, 전부 사라지면 멈춘다 — 그래서 켜고 끄는 것을 참조 카운트로 센다.
 */

export type Category = "game" | "work" | "other";

export interface UsageRow { exe: string; category: Category; seconds: number }
export interface Totals { game: number; work: number; other: number }
export interface Summary { from: string; to: string; rows: UsageRow[]; totals: Totals }
export interface SessionRow {
  exe: string; category: Category; started_at: string; ended_at: string; seconds: number;
}

export const CATEGORY_LABEL: Record<Category, string> = {
  game: "게임", work: "작업", other: "기타",
};
/** 도넛·막대에 쓰는 색. theme.css 토큰만 쓴다. */
export const CATEGORY_COLOR: Record<Category, string> = {
  game: "var(--accent)", work: "var(--ok)", other: "var(--text-dim)",
};

/** 로컬 날짜를 YYYY-MM-DD 로. `Date.toISOString()` 은 UTC 라 하루가 밀린다. */
export function localDay(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** `n` 일 전의 로컬 날짜. */
export function daysAgo(n: number, from = new Date()): string {
  const d = new Date(from);
  d.setDate(d.getDate() - n);
  return localDay(d);
}

/** 이번 주(월요일 시작)의 첫날. */
export function weekStart(from = new Date()): string {
  const d = new Date(from);
  const dow = (d.getDay() + 6) % 7; // 월=0
  d.setDate(d.getDate() - dow);
  return localDay(d);
}

/** 초 → "2시간 14분". 1분 미만은 "1분 미만". */
export function duration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return "1분 미만";
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  if (h === 0) return `${m}분`;
  return m === 0 ? `${h}시간` : `${h}시간 ${m}분`;
}

/** 초 → "2:14" (짧은 자리용). */
export function durationShort(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}` : `${m}분`;
}

/** ISO 시각 → "14:05". */
export function hhmm(iso: string): string {
  const t = iso.split("T")[1] ?? "";
  return t.slice(0, 5);
}

/** 실행 파일 이름을 보기 좋게 (알려진 것만 사람 이름으로). */
const PRETTY: Record<string, string> = {
  code: "VS Code", "code - insiders": "VS Code Insiders", devenv: "Visual Studio",
  windowsterminal: "Terminal", pwsh: "PowerShell", powershell: "PowerShell",
  chrome: "Chrome", msedge: "Edge", firefox: "Firefox",
  eldenring: "Elden Ring", nightreign: "Nightreign", slaythespire: "Slay the Spire",
  robloxplayerbeta: "Roblox", steamwebhelper: "Steam", explorer: "탐색기",
  claude: "Claude", discord: "Discord", spotify: "Spotify", obsidian: "Obsidian",
  "league of legends": "League of Legends", leagueclientux: "LoL 클라이언트",
  leagueclient: "LoL 클라이언트", riotclientservices: "Riot 클라이언트",
  valorant: "VALORANT", "valorant-win64-shipping": "VALORANT",
  epicgameslauncher: "Epic Games", galaxyclient: "GOG Galaxy", notepad: "메모장",
  destiny2: "Destiny 2", limbuscompany: "Limbus Company", lobotomycorp: "Lobotomy Corporation",
  slaythespire2: "Slay the Spire 2", astralparty_int: "Astral Party",
};
export function prettyExe(exe: string): string {
  return PRETTY[exe.toLowerCase()] ?? exe;
}

/** 사람이 흔히 쓰는 줄임말 → 실행 파일 이름. `PRETTY` 의 역방향 매핑으로 못 잡는 것만. */
const ALIASES: Record<string, string> = {
  "visual studio code": "code", vscode: "code", lol: "league of legends",
};
/** `PRETTY` 를 뒤집은 것 — 사람 이름(소문자) → 실행 파일 이름. */
const PRETTY_REVERSE: Record<string, string> = Object.fromEntries(
  Object.entries(PRETTY).map(([exe, pretty]) => [pretty.toLowerCase(), exe]),
);

/** 사람이 입력한 이름 하나를 실행 파일 이름으로. 아는 이름이 아니면 그대로 통과시킨다. */
function toExeKey(raw: string): string {
  const key = raw.trim().toLowerCase().replace(/\.exe$/, "");
  return ALIASES[key] ?? PRETTY_REVERSE[key] ?? key;
}

/** 합계가 0 이어도 나누지 않는 비율. */
export const share = (part: number, total: number) => (total > 0 ? part / total : 0);

/**
 * 도넛 조각 색. 프로그램 수가 팔레트보다 많으면 같은 색을 점점 옅게 돌려 쓴다 —
 * 새 색을 지어내면 테마(강조색·팔레트)와 따로 놀게 된다.
 */
const SLICE_PALETTE = [
  "var(--accent)", "var(--ok)", "var(--warn)", "var(--danger)", "var(--brand-spotify)",
];
export function sliceColor(i: number): string {
  const base = SLICE_PALETTE[i % SLICE_PALETTE.length];
  const round = Math.floor(i / SLICE_PALETTE.length);
  if (round === 0) return base;
  const pct = Math.max(30, 100 - round * 30);
  return `color-mix(in srgb, ${base} ${pct}%, transparent)`;
}

/**
 * 설정에 적은 "추가로 볼 프로그램" 목록을 정규화한다 (쉼표 구분, 대소문자·.exe 무시).
 * 사람이 읽는 이름("VS Code")도 받는다 — `toExeKey` 가 실행 파일 이름으로 바꾼다.
 */
export function parseExtras(text: string): string[] {
  return String(text ?? "")
    .split(",")
    .map(toExeKey)
    .filter(Boolean);
}

/** 쉼표 목록에 프로그램을 더한다 (이미 있으면 그대로). */
export function addExtra(text: string, exe: string): string {
  const list = parseExtras(text);
  const key = toExeKey(exe);
  if (!key || list.includes(key)) return text;
  return [...list, key].join(", ");
}

/** 쉼표 목록에서 프로그램을 뺀다. */
export function removeExtra(text: string, exe: string): string {
  const key = toExeKey(exe);
  return parseExtras(text).filter((e) => e !== key).join(", ");
}

/**
 * 위젯이 보여줄 행을 고른다: 게임은 자동으로, 그 밖에는 사용자가 등록한 것만.
 *
 * 분류가 "게임" 인 것은 규칙이 알아서 잡아 주고, 나머지는 눈에 띄는 대로 다 보여주면
 * 목록이 금방 쓰레기가 된다 — 그래서 명시적으로 등록한 것만 더한다.
 */
export function pickTracked(rows: UsageRow[], extras: string[]): UsageRow[] {
  const set = new Set(extras);
  const picked = rows.filter((r) => r.category === "game" || set.has(r.exe.toLowerCase()));
  return [...picked].sort((a, b) => b.seconds - a.seconds);
}

/**
 * 검색어가 이 프로그램에 걸리는가.
 *
 * 실행 파일 이름(`code`)과 사람 이름(`VS Code`) **둘 다** 본다. 사용자는 목록에서 본 이름으로
 * 찾는데 저장된 것은 실행 파일 이름이라, 한쪽만 보면 눈앞의 항목이 검색에 안 걸린다.
 */
export function matchProgram(exe: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return exe.toLowerCase().includes(q) || prettyExe(exe).toLowerCase().includes(q);
}

/** 아직 등록하지 않은 프로그램들 (등록 후보로 보여준다). 게임은 자동으로 세므로 뺀다. */
export function pickCandidates(rows: UsageRow[], extras: string[], limit = 8, query = ""): UsageRow[] {
  const set = new Set(extras);
  return rows
    .filter((r) => r.category !== "game" && !set.has(r.exe.toLowerCase()) && matchProgram(r.exe, query))
    .slice(0, limit);
}

let refs = 0;
/**
 * 이 위젯이 떠 있는 동안 백엔드 폴링을 켜 둔다 (여러 위젯이 공유).
 *
 * 켜기/끄기가 뒤집혀 도착하면 추적이 꺼진 채로 남으므로 `invokeInOrder` 로 보낸다.
 * 보낼 값은 **보내는 순간의** `refs` 로 정한다 — 큐에 쌓인 사이 다른 위젯이 붙었을 수 있다.
 */
function useActivityPolling() {
  useEffect(() => {
    refs += 1;
    invokeInOrder("activity_set_active", () => ({ active: refs > 0 }));
    return () => {
      refs -= 1;
      invokeInOrder("activity_set_active", () => ({ active: refs > 0 }));
    };
  }, []);
}

/** `from`~`to` 의 사용 시간 집계. 기록이 바뀌면 다시 읽는다. */
export function useUsage(from: string, to: string) {
  const [data, setData] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);
  useActivityPolling();

  const reload = useCallback(() => {
    invoke<Summary>("activity_usage", { from, to })
      .then((s) => { setData(s); setError(null); })
      .catch((e) => setError(String(e)));
  }, [from, to]);

  useEffect(reload, [reload]);
  useEvent("activity://changed", reload);
  return { data, error };
}

/** 하루의 사용 구간. */
export function useSessions(day: string, limit = 20) {
  const [data, setData] = useState<SessionRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  useActivityPolling();

  const reload = useCallback(() => {
    invoke<SessionRow[]>("activity_sessions", { day, limit })
      .then((s) => { setData(s); setError(null); })
      .catch((e) => setError(String(e)));
  }, [day, limit]);

  useEffect(reload, [reload]);
  useEvent("activity://changed", reload);
  return { data, error };
}
