import type { CSSProperties } from "react";
import "./DotDigits.css";

/** 5×7 도트 매트릭스 글리프 (문자 도트용). 1 = 켜짐. */
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

/** 3×5 사각 블록 글리프 (디지털 시계 스타일). */
export const BLOCK_GLYPHS: Record<string, string[]> = {
  "0": ["111", "101", "101", "101", "111"],
  "1": ["110", "010", "010", "010", "111"],
  "2": ["111", "001", "111", "100", "111"],
  "3": ["111", "001", "111", "001", "111"],
  "4": ["101", "101", "111", "001", "001"],
  "5": ["111", "100", "111", "001", "111"],
  "6": ["111", "100", "111", "101", "111"],
  "7": ["111", "001", "001", "001", "001"],
  "8": ["111", "101", "111", "101", "111"],
  "9": ["111", "101", "111", "001", "111"],
  ":": ["0", "1", "0", "1", "0"],
  " ": ["00", "00", "00", "00", "00"],
};
export const BLOCK_ROWS = 5;

type Font = { glyphs: Record<string, string[]>; rows: number };
const FONTS: Record<"dots" | "blocks", Font> = {
  dots: { glyphs: GLYPHS, rows: GLYPH_ROWS },
  blocks: { glyphs: BLOCK_GLYPHS, rows: BLOCK_ROWS },
};

/**
 * 텍스트를 글리프 격자로 렌더.
 * - style="dots": 켜진 셀에 문자(dot)를 찍는다 (■, 이모지 등)
 * - style="blocks": 켜진 셀을 색 있는 둥근 사각형으로 그린다 (gap 만큼 띄움)
 * cell = 셀 한 변(px), 글자 사이는 1셀 간격.
 */
export function DotDigits({ text, cell, style = "dots", dot = "■", color, gap }: {
  text: string; cell: number; style?: "dots" | "blocks"; dot?: string; color?: string; gap?: number;
}) {
  const font = FONTS[style];
  const g = cell * (gap ?? (style === "blocks" ? 0.22 : 0.12));
  const isEmoji = /\p{Extended_Pictographic}/u.test(dot);
  const vars = { "--cell": `${cell}px`, "--gap": `${g}px`, "--block": color || "var(--accent)" } as CSSProperties;
  return (
    <div className={`dots dots-${style}`} style={{ gap: cell, ...vars }}>
      {[...text].map((ch, i) => {
        const rows = font.glyphs[ch] ?? font.glyphs[" "];
        const cols = rows[0].length;
        return (
          <div key={i} className="dots-glyph" style={{ gridTemplateColumns: `repeat(${cols}, var(--cell))` }}>
            {rows.flatMap((row, r) => [...row].map((bit, c) => (
              <span key={`${r}-${c}`} className={`dots-cell ${bit === "1" ? "on" : ""} ${isEmoji ? "emoji" : ""}`}>
                {style === "dots" && bit === "1" ? dot : ""}
              </span>
            )))}
          </div>
        );
      })}
    </div>
  );
}

/** 주어진 영역에 글자 수만큼 넣을 수 있는 최대 셀 크기. */
export function fitCell(text: string, w: number, h: number, style: "dots" | "blocks" = "dots"): number {
  const font = FONTS[style];
  const cols = [...text].reduce((n, ch) => n + (font.glyphs[ch] ?? font.glyphs[" "])[0].length, 0) + Math.max(0, text.length - 1);
  return Math.max(3, Math.floor(Math.min(w / cols, h / font.rows)));
}
