/**
 * 대시보드가 **키보드를 쓰는 중**인가 — 백엔드가 전경을 원래 창에 돌려줄지 정하는 기준.
 *
 * 입력 요소 포커스만 보면 Esc 가 0.25초 뒤에 죽는다: 편집 모드(선택 해제 → 잠금)와
 * 팝업(닫기)은 입력 요소 없이 Esc 를 받는데, 그 사이 전경이 다른 앱으로 넘어가기 때문이다.
 * 이 상태들은 사용자가 대시보드를 **조작하는 중**인 시간이라 전경을 쥐어도 된다 —
 * 끝나면(잠금·닫기) 백엔드가 그때 돌려준다.
 *
 * 잠긴 상태에서 칠 것이 없는 위젯을 누른 경우는 여전히 거짓이다. 원래 고친 증상
 * ("시계를 눌렀을 뿐인데 다른 앱에서 한글이 안 쳐진다")의 경로는 그대로다.
 */
export function keyboardWanted(s: {
  textFocus: boolean;
  locked: boolean;
  settingsOpen: boolean;
  openPopups: number;
}): boolean {
  return s.textFocus || !s.locked || s.settingsOpen || s.openPopups > 0;
}
