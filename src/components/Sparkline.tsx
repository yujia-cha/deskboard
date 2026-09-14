/** 작은 꺾은선 그래프. values 는 오래된 것 → 최신 순. */
export function Sparkline({ values, max, height = 36, color = "var(--accent)", fill = true }: {
  values: number[]; max?: number; height?: number; color?: string; fill?: boolean;
}) {
  const w = 100, h = height;
  const m = max ?? Math.max(1, ...values);
  const n = values.length;
  if (n < 2) return <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ width: "100%", height }} />;
  const pts = values.map((v, i) => [(i / (n - 1)) * w, h - (Math.min(v, m) / m) * (h - 2) - 1] as const);
  const d = pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  return (
    <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ width: "100%", height, display: "block" }}>
      {fill && <path d={`${d} L${w},${h} L0,${h} Z`} fill={color} opacity={0.15} />}
      <path d={d} fill="none" stroke={color} strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
    </svg>
  );
}
