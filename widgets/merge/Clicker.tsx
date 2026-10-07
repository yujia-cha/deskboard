import { useCallback, useEffect, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { invokeInOrder } from "deskboard";
import type { MergeContent, MergeView } from "./types";
import { Icon } from "./Icon";

const R = 24;
const C = 2 * Math.PI * R;

interface Props {
  view: MergeView;
  content: MergeContent | null;
  reload: () => Promise<void>;
  /** G-4: 에너지가 모자랄 때 강조 */
  hint: boolean;
}

/** `<button>` 이 아니라 div — Space/Enter 가 클릭으로 이중 계산되지 않게 한다. 센 것은 pointerdown 만. */
export function Clicker({ view, content, reload, hint }: Props) {
  const need = Math.max(1, view.eco.clicksPerEnergy);
  const pending = useRef(0);
  const timer = useRef<number | undefined>(undefined);
  const [, bump] = useState(0);
  // 보냈지만 아직 view 에 반영되지 않은 클릭 — 링이 되감기지 않게 더해서 보여 준다.
  // 보내는 동안은 보낼 때의 `clickerRem`(base)을 기준으로 그린다 — reload 가 먼저 닿고 `finally` 가 나중에 돌면
  // 새 rem + inflight 로 한 프레임 겹쳐 세어 링이 튀기 때문이다.
  const [inflight, setInflight] = useState(0);
  const base = useRef(0);
  const remRef = useRef(view.clickerRem);
  remRef.current = view.clickerRem;
  const inflightRef = useRef(0);

  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    const n = pending.current;
    if (n <= 0) return;
    pending.current = 0;
    if (inflightRef.current === 0) base.current = remRef.current;
    inflightRef.current += n;
    setInflight((v) => v + n);
    // invokeInOrder 는 실패해도 undefined 로 끝난다 — 어느 쪽이든 다시 읽어 바로잡는다.
    invokeInOrder("merge_clicker_add", { n })
      .then(() => reload())
      .finally(() => {
        inflightRef.current = Math.max(0, inflightRef.current - n);
        setInflight((v) => Math.max(0, v - n));
      });
  }, [reload]);

  // 떠날 때 모아 둔 클릭을 보낸다.
  useEffect(() => () => flush(), [flush]);

  const onDown = (e: RPointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    pending.current += 1;
    bump((v) => v + 1);
    window.clearTimeout(timer.current);
    if (view.clickerRem + inflight + pending.current >= need) flush();
    else timer.current = window.setTimeout(flush, 1000);
  };

  const shown = ((inflight > 0 ? base.current : view.clickerRem) + inflight + pending.current) % need;
  const skin = content?.skins.clicker ?? { emoji: "🐹" };

  return (
    <div
      role="button"
      tabIndex={-1}
      aria-label="클리커"
      className={`merge-clicker ${hint ? "hint" : ""}`}
      title={`${Math.floor(shown)}/${need} — 눌러서 ⚡ 모으기`}
      onPointerDown={onDown}
    >
      <svg className="merge-ring" viewBox="0 0 56 56" aria-hidden>
        <circle className="merge-ring-bg" cx="28" cy="28" r={R} />
        <circle
          className="merge-ring-fg" cx="28" cy="28" r={R}
          transform="rotate(-90 28 28)"
          strokeDasharray={C}
          strokeDashoffset={C * (1 - shown / need)}
        />
      </svg>
      <span className="merge-clicker-icon"><Icon icon={skin} /></span>
    </div>
  );
}
