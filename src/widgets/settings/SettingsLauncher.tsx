import { useSettings } from "../../core/settings";
import { UpdateDot } from "../../components/UpdateDot";
import "./SettingsLauncher.css";

/**
 * 설정 위젯 — 톱니바퀴 아이콘만 있는 정사각형. 클릭하면 편집 모드(잠금)를 토글한다.
 * 설정 패널 자체는 편집 모드의 편집 바 "⚙ 설정" 버튼이나 각 위젯 오버레이 ⚙ 로 연다.
 */
export function SettingsLauncher() {
  const locked = useSettings((s) => s.locked);
  const setLocked = useSettings((s) => s.setLocked);

  return (
    <button className="launcher" title={locked ? "편집 모드 — 위젯 이동·크기 조절" : "잠금"} onClick={() => setLocked(!locked)}>
      ⚙
      <UpdateDot className="corner" />
    </button>
  );
}
