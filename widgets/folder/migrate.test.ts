import { describe, expect, it } from "vitest";
import { migrate } from "./migrate";

describe("folder migrate", () => {
  it("전용 폴더 경로는 managed 로 바꾸고 dir 을 지운다", () => {
    const out = migrate({ dir: "C:\\Users\\me\\AppData\\Roaming\\com.user.deskboard\\folders\\abc-123" }, "abc-123");
    expect(out.source).toBe("managed");
    expect(out.dir).toBe("");
  });
  it("다른 경로는 link 로 두고 dir 을 남긴다", () => {
    const out = migrate({ dir: "D:\\shortcuts" }, "abc-123");
    expect(out.source).toBe("link");
    expect(out.dir).toBe("D:\\shortcuts");
  });
  it("빈 경로는 managed", () => {
    expect(migrate({ dir: "" }, "abc-123").source).toBe("managed");
  });
  it("이미 source 가 있으면 그대로", () => {
    expect(migrate({ source: "link", dir: "D:/x" }, "abc-123")).toEqual({ source: "link", dir: "D:/x" });
  });
});
