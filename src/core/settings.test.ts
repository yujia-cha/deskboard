import { describe, expect, it } from "vitest";
import { defaultInstances, findFreeSlot, migrate, overlaps, resolvePalette } from "./settings";

describe("layout", () => {
  it("default widgets do not overlap", () => {
    const list = defaultInstances();
    for (let i = 0; i < list.length; i++)
      for (let j = i + 1; j < list.length; j++)
        expect(overlaps(list[i], list[j]), `${list[i].widgetId} vs ${list[j].widgetId}`).toBe(false);
  });

  it("findFreeSlot avoids existing widgets", () => {
    const existing = defaultInstances();
    const { x, y } = findFreeSlot(existing, 300, 200, { w: 1920, h: 1040 });
    expect(existing.some((e) => overlaps({ x, y, w: 300, h: 200 }, e))).toBe(false);
  });

  it("findFreeSlot falls back when nothing fits", () => {
    expect(findFreeSlot([{ x: 0, y: 0, w: 5000, h: 5000 }], 300, 200, { w: 1920, h: 1040 })).toEqual({ x: 24, y: 24 });
  });
});

describe("palette", () => {
  it("passes explicit choices through", () => {
    expect(resolvePalette("dark", false)).toBe("dark");
    expect(resolvePalette("light", true)).toBe("light");
  });

  it('resolves "auto" from the OS preference', () => {
    expect(resolvePalette("auto", true)).toBe("dark");
    expect(resolvePalette("auto", false)).toBe("light");
  });

  it("defaults to dark when the OS preference is unreadable", () => {
    // matchMedia 가 없는 환경에서 호출하면 다크로 떨어진다
    expect(resolvePalette("auto")).toBe("dark");
  });
});

describe("migrate", () => {
  it("carries the old GitHub CI toggle over to the repository list", () => {
    expect(migrate("github", { showChecks: false }).showRepos).toBe(false);
    expect(migrate("github", { showChecks: true }).showRepos).toBe(true);
  });

  it("does not overwrite a choice the user already made", () => {
    expect(migrate("github", { showChecks: false, showRepos: true }).showRepos).toBe(true);
  });

  it("leaves a fresh widget alone", () => {
    expect(migrate("github", {})).toEqual({});
  });
});

describe("migrate: 폴더의 두 모드", () => {
  it("전용 폴더였던 경로는 managed 로 접고 경로는 지운다", () => {
    const id = "abc-123";
    const out = migrate("folder", { dir: "C:\\Users\\me\\AppData\\Roaming\\com.user.deskboard\\folders\\" + id }, id);
    expect(out.source).toBe("managed");
    expect(out.dir).toBe("");
  });

  it("사용자가 직접 고른 폴더는 link 로 남는다", () => {
    const out = migrate("folder", { dir: "D:\\shortcuts" }, "abc-123");
    expect(out.source).toBe("link");
    expect(out.dir).toBe("D:\\shortcuts");
  });

  it("빈 경로는 managed", () => {
    expect(migrate("folder", { dir: "" }, "abc-123").source).toBe("managed");
  });

  it("이미 source 가 있으면 건드리지 않는다", () => {
    expect(migrate("folder", { source: "link", dir: "D:/x" }, "abc-123")).toEqual({ source: "link", dir: "D:/x" });
  });
});
