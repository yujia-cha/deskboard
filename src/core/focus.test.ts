import { describe, expect, it } from "vitest";
import { keyboardWanted } from "./focus";

const idle = { textFocus: false, locked: true, settingsOpen: false, openPopups: 0 };

describe("keyboardWanted", () => {
  it("잠긴 채 칠 것이 없는 위젯을 누르면 키보드를 돌려준다", () => {
    expect(keyboardWanted(idle)).toBe(false);
  });

  it("입력 요소에 포커스가 있으면 쥔다", () => {
    expect(keyboardWanted({ ...idle, textFocus: true })).toBe(true);
  });

  it("편집 모드는 Esc 를 받아야 하므로 쥔다", () => {
    expect(keyboardWanted({ ...idle, locked: false })).toBe(true);
  });

  it("설정 패널이 열려 있으면 쥔다", () => {
    expect(keyboardWanted({ ...idle, settingsOpen: true })).toBe(true);
  });

  it("팝업이 하나라도 열려 있으면 Esc 로 닫을 수 있게 쥔다", () => {
    expect(keyboardWanted({ ...idle, openPopups: 1 })).toBe(true);
  });
});
