import { useMemo } from "react";
import { GRIDS, litCells, readout } from "./words";
import "./WordClock.css";

/**
 * QLOCKTWO 스타일 워드 클락. 글자 격자에서 지금 시각에 해당하는 단어만 밝힌다.
 * 꺼진 글자는 지우지 않고 아주 흐리게 남긴다 — 그래야 격자로 읽힌다.
 */
export function WordClock({ date, lang, size, color }: {
  date: Date;
  lang: "en" | "ko";
  size: { w: number; h: number };
  color?: string;
}) {
  const grid = GRIDS[lang] ?? GRIDS.en;
  const h = date.getHours();
  const m = date.getMinutes();
  // 5분 단위로만 바뀌므로 그 슬롯이 같으면 다시 계산하지 않는다.
  const slot = Math.floor(m / 5);
  const { lit, text } = useMemo(
    () => ({ lit: litCells(grid, h, slot * 5), text: readout(grid, h, slot * 5) }),
    [grid, h, slot],
  );

  const cols = grid.rows[0].length;
  const rows = grid.rows.length;
  // 격자가 위젯 안에 들어가는 최대 글자 크기
  const cell = Math.max(6, Math.min(size.w / cols, size.h / rows));

  return (
    <div className="wordclock" title={text}
      style={{ "--cell": `${cell}px`, ...(color ? { "--lit": color } : {}) } as React.CSSProperties}>
      {grid.rows.map((row, r) => (
        <div className="wordclock-row" key={r}>
          {[...row].map((ch, c) => (
            <span key={c} className={lit.has(`${r},${c}`) ? "on" : ""}>{ch}</span>
          ))}
        </div>
      ))}
    </div>
  );
}
