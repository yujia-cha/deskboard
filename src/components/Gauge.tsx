/** 원형 게이지 (0~100). 값에 따라 ok/warn/danger 색으로 바뀐다. */
export function Gauge({ value, label, sub, size = 84, stroke = 8 }: {
  value: number | null; label: string; sub?: string; size?: number; stroke?: number;
}) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const v = value == null ? 0 : Math.max(0, Math.min(100, value));
  const color = v >= 90 ? "var(--danger)" : v >= 70 ? "var(--warn)" : "var(--accent)";
  return (
    <div className="gauge" style={{ width: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--track)" strokeWidth={stroke} />
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={color} strokeWidth={stroke}
          strokeLinecap="round" strokeDasharray={c} strokeDashoffset={c * (1 - v / 100)}
          transform={`rotate(-90 ${size / 2} ${size / 2})`} style={{ transition: "stroke-dashoffset .6s, stroke .6s" }} />
        <text x="50%" y="50%" textAnchor="middle" dominantBaseline="central" fill="var(--text)"
          fontSize={size * 0.24} fontWeight={500}>{value == null ? "—" : `${Math.round(v)}%`}</text>
      </svg>
      <div className="gauge-label">{label}</div>
      {sub && <div className="gauge-sub">{sub}</div>}
    </div>
  );
}
