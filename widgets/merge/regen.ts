import { useEffect, useRef, useState } from "react";

/** 다음 +1⚡ 까지 남은 초. 시계가 되감겨 anchor 보다 앞이면 한 주기를 그대로 돌려준다. */
export function nextRegenIn(anchorMs: number, intervalSec: number, nowMs: number): number {
  const interval = Math.max(1, intervalSec);
  const elapsed = (nowMs - anchorMs) / 1000;
  if (elapsed < 0) return interval;
  return interval - Math.floor(elapsed % interval);
}

/** anchor 이후 지난 주기 수 — 늘어났으면 백엔드가 정산할 때다. */
export function regenCycles(anchorMs: number, intervalSec: number, nowMs: number): number {
  const elapsed = (nowMs - anchorMs) / 1000;
  return elapsed < 0 ? 0 : Math.floor(elapsed / Math.max(1, intervalSec));
}

export function formatMMSS(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

interface RegenView {
  energy: number;
  regenAnchor: number;
  eco: { regenIntervalSec: number; regenCap: number };
}

/**
 * "다음 +1 까지" 초를 돌려준다 (가득 찼으면 null).
 * 1초 interval 은 ⚡ < cap 이고 창이 보일 때만 돈다. 주기가 넘어가면 `reload()` 를 한 번 부른다. rAF 없음.
 */
export function useRegenCountdown(view: RegenView | null, reload: () => unknown): number | null {
  const [now, setNow] = useState(() => Date.now());
  const [visible, setVisible] = useState(() => document.visibilityState === "visible");
  const seen = useRef<{ anchor: number; cycles: number } | null>(null);
  const reloadRef = useRef(reload);
  reloadRef.current = reload;

  useEffect(() => {
    const on = () => setVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", on);
    return () => document.removeEventListener("visibilitychange", on);
  }, []);

  const active = !!view && view.energy < view.eco.regenCap && visible;
  const anchor = view?.regenAnchor ?? 0;
  const interval = view?.eco.regenIntervalSec ?? 120;

  useEffect(() => {
    if (!active) return;
    const tick = () => {
      const t = Date.now();
      setNow(t);
      const cycles = regenCycles(anchor, interval, t);
      const prev = seen.current;
      seen.current = { anchor, cycles };
      if (prev && prev.anchor === anchor && cycles > prev.cycles) reloadRef.current();
    };
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [active, anchor, interval]);

  return active ? nextRegenIn(anchor, interval, now) : null;
}
