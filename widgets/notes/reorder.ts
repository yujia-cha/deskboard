/**
 * 할 일 순서 바꾸기의 계산 부분. DOM 없이 테스트한다 — `Notes` 는 포인터 이벤트만 다룬다.
 *
 * **왜 "앞에 끼워 넣기"인가.** 목록은 완료 항목을 숨길 수 있어서, 화면에 보이는 순서와
 * 저장된 전체 순서가 다르다. "n번째로 옮긴다"로 계산하면 숨은 항목 때문에 엉뚱한 자리에
 * 떨어진다. **어느 항목 앞에 두는지**로 말하면 보이는 것만 알아도 전체 순서가 정해진다.
 */

/**
 * `dragId` 를 빼서 `beforeId` **바로 앞**에 끼워 넣는다. `beforeId` 가 null 이면 맨 뒤로.
 *
 * 없는 id 나 자기 자신 앞으로 옮기는 경우는 원본을 그대로 돌려준다 — 부르는 쪽이
 * 매 포인터 이동마다 호출하므로, 의미 없는 변경으로 목록이 흔들리면 안 된다.
 */
export function moveBefore(ids: string[], dragId: string, beforeId: string | null): string[] {
  const from = ids.indexOf(dragId);
  if (from < 0) return ids;
  if (beforeId === dragId) return ids;
  if (beforeId !== null && !ids.includes(beforeId)) return ids;

  const rest = ids.filter((id) => id !== dragId);
  const at = beforeId === null ? rest.length : rest.indexOf(beforeId);
  const next = [...rest.slice(0, at), dragId, ...rest.slice(at)];
  return next.every((id, i) => id === ids[i]) ? ids : next;
}

/**
 * 포인터가 어느 항목 **앞**을 가리키는지. 각 항목의 세로 중점을 넘었는지로 본다.
 *
 * 중점을 쓰는 이유: 위쪽 절반에 있으면 그 항목 앞, 아래쪽 절반이면 다음 항목 앞이 된다 —
 * 경계가 항목 사이에 오므로 한 칸씩 밀릴 때 손의 위치와 결과가 맞는다.
 * 마지막 항목의 아래쪽 절반이면 null(맨 뒤)이다.
 */
export function dropTarget(rows: { id: string; top: number; height: number }[], y: number): string | null {
  for (const r of rows) {
    if (y < r.top + r.height / 2) return r.id;
  }
  return null;
}
