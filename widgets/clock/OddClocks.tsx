import type { CSSProperties } from "react";
import { bcdColumns, fibRoles, FIB_VALUES } from "./oddtime";
import "./OddClocks.css";

/**
 * 바이너리 시계 (BCD) — 자리마다 세로 한 줄, 아래가 1의 자리.
 * 읽는 법이 직관적이지 않으므로 실제 시각을 title 에 넣는다.
 */
export function BinaryClock({ date, size, seconds, color }: {
  date: Date; size: { w: number; h: number }; seconds: boolean; color?: string;
}) {
  const cols = bcdColumns(date.getHours(), date.getMinutes(), date.getSeconds(), seconds);
  const tallest = Math.max(...cols.map((c) => c.bits.length));
  // 점 크기: 가로는 열 수, 세로는 가장 긴 열에 맞춘다. 간격은 점의 절반.
  const dot = Math.max(4, Math.min(size.w / (cols.length * 1.5), size.h / (tallest * 1.5)));
  const text = cols.map((c) => c.digit).join("").replace(/(\d\d)(?=\d)/g, "$1:");

  return (
    <div className="binclock" title={text}
      style={{ "--dot": `${dot}px`, "--pitch": `${dot * 0.5}px`, ...(color ? { "--lit": color } : {}) } as CSSProperties}>
      <div className="binclock-group">
        {cols.map((c, i) => (
          <div className="binclock-col" key={i} data-unit={c.unit} title={`${c.digit}`}>
            {/* column-reverse 라 배열 순서대로 넣으면 LSB 가 아래로 간다 */}
            {[...c.bits].reverse().map((on, b) => (
              <span key={b} className={`binclock-cell ${on ? "on" : ""}`} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

const FIB_CLASS = ["fib-1a", "fib-1b", "fib-2", "fib-3", "fib-5"];

/**
 * 피보나치 시계 — 1,1,2,3,5 칸의 합으로 시와 분(5분 단위)을 나타낸다.
 * 시 = 강조색, 분 = ok, 둘 다 쓰이면 warn.
 */
export function FibonacciClock({ date, size, color }: {
  date: Date; size: { w: number; h: number }; color?: string;
}) {
  const m = date.getMinutes();
  // 같은 합이라도 조합이 여러 가지다. 5분 슬롯마다 배치를 바꿔 실물처럼 살아 있게 한다.
  const roles = fibRoles(date.getHours(), m, Math.floor(m / 5));
  // 격자는 8칸 × 5칸
  const u = Math.max(4, Math.min(size.w / 8.4, size.h / 5.4));
  const hh = String(((date.getHours() % 12) + 12) % 12 || 12);
  const text = `${hh}:${String(Math.floor(m / 5) * 5).padStart(2, "0")}`;

  return (
    <div className="fibclock" title={text}
      style={{ "--u": `${u}px`, ...(color ? { "--lit": color } : {}) } as CSSProperties}>
      <div className="fibclock-grid">
        {FIB_VALUES.map((v, i) => (
          <div key={i} className={`fibclock-cell ${FIB_CLASS[i]}`} data-role={roles[i]} title={`${v}`} />
        ))}
      </div>
    </div>
  );
}
