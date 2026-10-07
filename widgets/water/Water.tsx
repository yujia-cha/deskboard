import { useEffect, useRef } from "react";
import type { WidgetProps } from "deskboard";
import { WaterEngine } from "./engine";
import type { WaterSettings } from "./params";
import "./Water.css";

export type { WaterSettings } from "./params";

/**
 * 통에 담긴 픽셀 물. 통 안쪽을 끌면 흔들고, 가장자리(점선 고리)를 끌면 돌린다.
 * 휠 = 15°씩 회전, 더블클릭 = 원위치. 물리 법칙은 설정의 "물성" 에서 고르거나 직접 조정한다.
 *
 * 실제 일은 `WaterEngine` 이 한다 — React 는 캔버스를 깔고 설정을 넘길 뿐이다.
 * 매 프레임 DOM 을 바꾸지 않으므로 `WidgetFrame` 의 넘침 측정(MutationObserver)도 깨우지 않는다.
 */
export function Water({ settings }: WidgetProps<WaterSettings>) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const probeRef = useRef<HTMLSpanElement>(null);
  const engineRef = useRef<WaterEngine | null>(null);
  const initial = useRef(settings);

  useEffect(() => {
    const engine = new WaterEngine(canvasRef.current!, probeRef.current!, initial.current);
    engineRef.current = engine;
    return () => { engine.dispose(); engineRef.current = null; };
  }, []);
  useEffect(() => { engineRef.current?.setSettings(settings); }, [settings]);

  return (
    <div className="water">
      <canvas ref={canvasRef} className="water-canvas" />
      {/* CSS 토큰을 실제 색으로 풀어 읽는 데만 쓴다 (`readColors`) */}
      <span ref={probeRef} className="water-probe" aria-hidden />
    </div>
  );
}
