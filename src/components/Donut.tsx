/**
 * 도넛 차트. `Gauge` 와 같은 SVG 원호 방식인데 여러 조각을 잇는다.
 *
 * 조각을 하나씩 `stroke-dasharray` 로 그리고 `stroke-dashoffset` 으로 밀어 배치한다
 * (path 를 계산하는 것보다 짧고, 둥근 끝처리도 그대로 쓸 수 있다).
 */
export interface Slice {
  value: number;
  color: string;
  label: string;
}

export function Donut({ slices, size = 96, stroke = 12, center, sub }: {
  slices: Slice[];
  size?: number;
  stroke?: number;
  /** 가운데 큰 글자 */
  center?: string;
  /** 가운데 작은 글자 */
  sub?: string;
}) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const total = slices.reduce((n, s) => n + Math.max(0, s.value), 0);

  // 각 조각이 시작하는 위치(누적 비율)
  let acc = 0;
  const drawn = slices
    .filter((s) => s.value > 0)
    .map((s) => {
      const frac = total > 0 ? s.value / total : 0;
      const offset = acc;
      acc += frac;
      return { ...s, frac, offset };
    });

  return (
    <div className="donut" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--track)" strokeWidth={stroke} />
        {drawn.map((s, i) => (
          <circle
            key={i}
            cx={size / 2} cy={size / 2} r={r}
            fill="none" stroke={s.color} strokeWidth={stroke}
            strokeDasharray={`${c * s.frac} ${c}`}
            strokeDashoffset={-c * s.offset}
            transform={`rotate(-90 ${size / 2} ${size / 2})`}
            style={{ transition: "stroke-dasharray .5s var(--ease), stroke-dashoffset .5s var(--ease)" }}
          >
            <title>{`${s.label} ${Math.round(s.frac * 100)}%`}</title>
          </circle>
        ))}
        {center && (
          <text x="50%" y={sub ? "45%" : "50%"} textAnchor="middle" dominantBaseline="central"
            fill="var(--text)" fontSize={size * 0.2} fontWeight={600}>{center}</text>
        )}
        {sub && (
          <text x="50%" y="62%" textAnchor="middle" dominantBaseline="central"
            fill="var(--text-dim)" fontSize={size * 0.12}>{sub}</text>
        )}
      </svg>
    </div>
  );
}
