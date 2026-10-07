import { describe, expect, it } from "vitest";
import { migrate } from "./migrate";

describe("github migrate", () => {
  it("v1 의 showChecks 를 showRepos 로 옮긴다", () => {
    expect(migrate({ showChecks: false }).showRepos).toBe(false);
    expect(migrate({ showChecks: true }).showRepos).toBe(true);
  });
  it("이미 showRepos 가 있으면 유지한다", () => {
    expect(migrate({ showChecks: false, showRepos: true }).showRepos).toBe(true);
  });
  it("빈 설정은 그대로", () => {
    expect(migrate({})).toEqual({});
  });
});
