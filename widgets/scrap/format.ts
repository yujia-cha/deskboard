/** 스크랩 위젯의 순수 함수들 (테스트 대상). */

export interface ScrapItem {
  title: string;
  url: string;
  source: string;
  /** 모델이 알아낸 게시일 (YYYY-MM-DD 등, 모르면 빈 문자열) */
  published: string;
  summary: string;
}

/** 링크로 열어도 되는 주소인가 — 웹 페이지가 만든 내용이므로 http/https 만 믿는다. */
export function safeUrl(url: string): string | null {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:" ? u.href : null;
  } catch {
    return null;
  }
}

/** 출처가 비어 있으면 주소의 호스트 이름으로 (`www.` 는 뗀다). */
export function sourceOf(item: ScrapItem): string {
  if (item.source.trim()) return item.source.trim();
  try { return new URL(item.url).hostname.replace(/^www\./, ""); } catch { return ""; }
}

/** "방금 · 12분 전 · 3시간 전 · 2일 전" */
export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "방금";
  if (s < 3600) return `${Math.floor(s / 60)}분 전`;
  if (s < 86400) return `${Math.floor(s / 3600)}시간 전`;
  return `${Math.floor(s / 86400)}일 전`;
}

/** 쉼표로 적은 사이트 목록 → 호스트 이름 배열 (주소를 통째로 붙여 넣어도 호스트만 남긴다). */
export function parseDomains(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/[,\s]+/)) {
    const t = raw.trim().toLowerCase();
    if (!t) continue;
    let host = t;
    try { if (t.includes("://")) host = new URL(t).hostname; } catch { continue; }
    host = host.replace(/^www\./, "").replace(/\/.*$/, "");
    if (/^[a-z0-9.-]+\.[a-z]{2,}$/.test(host) && !out.includes(host)) out.push(host);
  }
  return out;
}
