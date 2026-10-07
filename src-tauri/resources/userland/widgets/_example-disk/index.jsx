import { useCommand, Gauge } from "deskboard";

const GB = 1024 ** 3;

// widget.json 의 command 가 돌려준 JSON(드라이브 목록)을 게이지로 그린다.
// 드라이브가 하나면 PowerShell 이 배열이 아니라 객체 하나를 주므로 [].concat 으로 맞춘다.
export default function Disk({ size }) {
  const out = useCommand();
  if (!out) return <div style={{ color: "var(--text-dim)" }}>불러오는 중…</div>;
  if (out.error) return <div style={{ color: "var(--danger)" }}>{out.error}</div>;

  const drives = [].concat(out.data ?? []);
  const gauge = Math.max(48, Math.min(84, Math.floor(size.h * 0.5)));
  return (
    <div style={{ display: "flex", gap: 12, flexWrap: "wrap", alignItems: "flex-start" }}>
      {drives.map((d) => {
        const total = d.Used + d.Free;
        return (
          <Gauge
            key={d.Name}
            size={gauge}
            value={total > 0 ? (d.Used / total) * 100 : null}
            label={`${d.Name}:`}
            sub={`${Math.round(d.Free / GB)}GB 남음`}
          />
        );
      })}
    </div>
  );
}
