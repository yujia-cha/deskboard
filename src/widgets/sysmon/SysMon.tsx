import { useEffect, useRef, useState } from "react";
import { useProviderEvent } from "../../core/ipc";
import { Gauge } from "../../components/Gauge";
import { Sparkline } from "../../components/Sparkline";
import type { WidgetProps } from "../types";
import "./SysMon.css";

export interface SysMonSettings extends Record<string, unknown> {
  showHistory: boolean; showCores: boolean; showGpu: boolean;
}

interface GpuSample { name: string; temp_c: number | null; usage: number | null; mem_used_mb: number | null; mem_total_mb: number | null; power_w: number | null }
interface SensorSample {
  cpu_name: string; cpu_usage: number; cpu_cores: number[]; cpu_temp_c: number | null; cpu_temp_source: string | null;
  mem_used: number; mem_total: number; gpu: GpuSample | null;
}

const HISTORY = 30; // 2초 간격 × 30 = 60초
const gb = (b: number) => (b / 1024 ** 3).toFixed(1);

export function SysMon({ settings, size }: WidgetProps<SysMonSettings>) {
  const s = useProviderEvent<SensorSample>("sysmon://update");
  const hist = useRef<{ cpu: number[]; gpu: number[]; mem: number[] }>({ cpu: [], gpu: [], mem: [] });
  const [, tick] = useState(0);
  useEffect(() => {
    if (!s) return;
    const push = (a: number[], v: number) => { a.push(v); if (a.length > HISTORY) a.shift(); };
    push(hist.current.cpu, s.cpu_usage);
    push(hist.current.gpu, s.gpu?.usage ?? 0);
    push(hist.current.mem, (s.mem_used / s.mem_total) * 100);
    tick((t) => t + 1);
  }, [s]);

  if (!s) return <div className="dim">센서 대기 중…</div>;

  const memPct = (s.mem_used / s.mem_total) * 100;
  const showGpu = settings.showGpu && !!s.gpu;
  const gaugeSize = Math.max(56, Math.min(110, (size.w - 48) / (showGpu ? 3 : 2) - 16, size.h * 0.5));
  const temp = (t: number | null) => t == null ? null : <span className={`temp ${t >= 85 ? "hot" : t >= 70 ? "warm" : ""}`}>{Math.round(t)}°C</span>;

  return (
    <div className="sysmon">
      <div className="sysmon-gauges">
        <div className="sysmon-item">
          <Gauge value={s.cpu_usage} label="CPU" size={gaugeSize} />
          <div className="sysmon-meta" title={s.cpu_temp_source ?? "CPU 온도: LibreHardwareMonitor 실행 시 표시"}>
            {temp(s.cpu_temp_c) ?? <span className="dim">온도 없음</span>}
          </div>
        </div>
        {showGpu && s.gpu && (
          <div className="sysmon-item">
            <Gauge value={s.gpu.usage} label="GPU" size={gaugeSize} />
            <div className="sysmon-meta" title={s.gpu.name}>
              {temp(s.gpu.temp_c)}
              {s.gpu.mem_used_mb != null && s.gpu.mem_total_mb != null && (
                <span className="dim"> {(s.gpu.mem_used_mb / 1024).toFixed(1)}/{(s.gpu.mem_total_mb / 1024).toFixed(0)}G</span>
              )}
            </div>
          </div>
        )}
        <div className="sysmon-item">
          <Gauge value={memPct} label="RAM" size={gaugeSize} />
          <div className="sysmon-meta"><span className="dim">{gb(s.mem_used)} / {gb(s.mem_total)} GB</span></div>
        </div>
      </div>

      {settings.showCores && (
        <div className="sysmon-cores">
          {s.cpu_cores.map((c, i) => <div key={i} className="core" style={{ height: `${Math.max(4, c)}%` }} title={`Core ${i}: ${Math.round(c)}%`} />)}
        </div>
      )}

      {settings.showHistory && (
        <div className="sysmon-history">
          <Sparkline values={hist.current.cpu} max={100} height={28} />
          {showGpu && <Sparkline values={hist.current.gpu} max={100} height={28} color="var(--ok)" />}
        </div>
      )}
    </div>
  );
}
