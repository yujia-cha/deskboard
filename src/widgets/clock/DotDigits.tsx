import "./DotDigits.css";

/** 5×7 도트 매트릭스 글리프. 1 = 켜짐. */
export const GLYPHS: Record<string, string[]> = {
  "0": ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
  "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
  "2": ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
  "3": ["11111", "00010", "00100", "00010", "00001", "10001", "01110"],
  "4": ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
  "5": ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
  "6": ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
  "7": ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
  "8": ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
  "9": ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
  ":": ["0", "0", "1", "0", "1", "0", "0"],
  " ": ["000", "000", "000", "000", "000", "000", "000"],
};
export const GLYPH_ROWS = 7;

/** 텍스트를 도트 글리프로 렌더. cell = 셀 한 변(px). 모르는 문자는 공백. */
export function DotDigits({ text, cell, dot, gap = 0.12 }: { text: string; cell: number; dot: string; gap?: number }) {
  const g = cell * gap;
  const isEmoji = /\p{Extended_Pictographic}/u.test(dot);
  return (
    <div className="dots" style={{ gap: cell * 0.5, "--cell": `${cell}px`, "--gap": `${g}px` } as React.CSSProperties}>
      {[...text].map((ch, i) => {
        const rows = GLYPHS[ch] ?? GLYPHS[" "];
        const cols = rows[0].length;
        return (
          <div key={i} className="dots-glyph" style={{ gridTemplateColumns: `repeat(${cols}, var(--cell))` }}>
            {rows.flatMap((row, r) => [...row].map((bit, c) => (
              <span key={`${r}-${c}`} className={`dots-cell ${bit === "1" ? "on" : ""} ${isEmoji ? "emoji" : ""}`}>{bit === "1" ? dot : ""}</span>
            )))}
          </div>
        );
      })}
    </div>
  );
}

/** 주어진 영역에 글자 수만큼 넣을 수 있는 최대 셀 크기. */
export function fitCell(text: string, w: number, h: number): number {
  const cols = [...text].reduce((n, ch) => n + (GLYPHS[ch] ?? GLYPHS[" "])[0].length, 0) + Math.max(0, text.length - 1) * 0.5;
  return Math.max(3, Math.floor(Math.min(w / cols, h / GLYPH_ROWS)));
}
