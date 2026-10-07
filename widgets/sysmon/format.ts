/** 바이트/초 → 사람이 읽는 속도. 자릿수를 고정해 매 틱 폭이 흔들리지 않게 한다. */
export function formatRate(bps: number): string {
  if (!Number.isFinite(bps) || bps < 0) return "0 KB/s";
  if (bps < 1024) return `${Math.round(bps)} B/s`;
  const kb = bps / 1024;
  if (kb < 1024) return `${kb < 10 ? kb.toFixed(1) : Math.round(kb)} KB/s`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB/s`;
  return `${(mb / 1024).toFixed(1)} GB/s`;
}

/** 바이트 → 사람이 읽는 크기 (프로세스 메모리용). */
export function formatBytes(b: number): string {
  if (!Number.isFinite(b) || b <= 0) return "0 MB";
  const mb = b / 1024 ** 2;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

/**
 * 그래프 눈금 상한. 실제 최대치보다 살짝 위의 "깔끔한" 값으로 올려서
 * 트래픽이 조금 늘 때마다 그래프가 통째로 출렁이지 않게 한다.
 */
export function rateCeiling(values: number[]): number {
  const peak = Math.max(0, ...values);
  const min = 64 * 1024; // 64 KB/s 아래로는 내려가지 않는다 — 평소엔 바닥에 붙어 보여야 한다
  let step = min;
  while (step < peak) step *= 2;
  return step;
}

/** 프로세스 이름에서 확장자를 떼고 길면 줄인다. */
export function shortProcName(name: string, max = 18): string {
  const base = name.replace(/\.exe$/i, "");
  return base.length <= max ? base : `${base.slice(0, max - 1)}…`;
}
