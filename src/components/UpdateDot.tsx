import { selectHasUpdate, useUpdater } from "../core/updater";
import "./UpdateDot.css";

/** 새 업데이트가 있을 때만 뜨는 작은 점 — 설정 진입점 위에 얹는다. */
export function UpdateDot({ className }: { className?: string }) {
  const hasUpdate = useUpdater(selectHasUpdate);
  const state = useUpdater((s) => s.state);
  if (!hasUpdate) return null;
  const version = state.kind === "available" || state.kind === "ready" ? state.version : "";
  return <span className={"update-dot " + (className ?? "")} title={`새 버전 ${version}`} />;
}
