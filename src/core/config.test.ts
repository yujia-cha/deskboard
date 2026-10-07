import { describe, expect, it } from "vitest";
import { DEFAULT_CONFIG, DEFAULT_STRINGS, mergeConfig, t, useConfig } from "./config";
import { applyOverrides } from "./widgetRegistry";
import type { WidgetDefinition } from "./widgetTypes";

describe("mergeConfig", () => {
  it("파일이 없으면 기본값 그대로", () => {
    expect(mergeConfig(DEFAULT_CONFIG, null)).toEqual({ value: DEFAULT_CONFIG, warnings: [] });
  });

  it("일부만 적으면 나머지는 기본값", () => {
    const { value, warnings } = mergeConfig(DEFAULT_CONFIG, { defaults: { cornerRadius: 8 } });
    expect(value.defaults.cornerRadius).toBe(8);
    expect(value.defaults.surfaceOpacity).toBe(DEFAULT_CONFIG.defaults.surfaceOpacity);
    expect(value.layout).toEqual(DEFAULT_CONFIG.layout);
    expect(warnings).toEqual([]);
  });

  it("타입이 틀리면 기본값을 쓰고 경고한다", () => {
    const { value, warnings } = mergeConfig(DEFAULT_CONFIG, { defaults: { cornerRadius: "8" } });
    expect(value.defaults.cornerRadius).toBe(16);
    expect(warnings[0]).toMatch(/defaults\.cornerRadius/);
  });

  it("모르는 키(오타)는 경고한다 — 열린 맵 안은 제외", () => {
    const { warnings } = mergeConfig(DEFAULT_CONFIG, { defualts: {}, widgets: { clock: { title: "x" } }, intervals: { sysmon: 3 } });
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toMatch(/defualts/);
  });

  it("범위를 넘으면 잘라 넣는다", () => {
    const { value, warnings } = mergeConfig(DEFAULT_CONFIG, { defaults: { borderWidth: 99 } });
    expect(value.defaults.borderWidth).toBe(6);
    expect(warnings).toHaveLength(1);
  });

  it("선택지 밖의 문자열은 기본값", () => {
    const { value } = mergeConfig(DEFAULT_CONFIG, { defaults: { palette: "blue" } });
    expect(value.defaults.palette).toBe("dark");
  });

  it("배열은 통째로 바꾸고, 잘못된 항목만 버린다", () => {
    const { value, warnings } = mergeConfig(DEFAULT_CONFIG, {
      layout: { sizePresets: [{ label: "반", k: 0.5 }, { label: "틀림" }], default: [{ widget: "clock", x: 0, y: 0, w: 10, h: 10 }] },
    });
    expect(value.layout.sizePresets).toEqual([{ label: "반", k: 0.5 }]);
    expect(value.layout.default).toHaveLength(1);
    expect(warnings).toHaveLength(1);
  });

  it("최상위가 객체가 아니면 통째로 무시", () => {
    const { value, warnings } = mergeConfig(DEFAULT_CONFIG, [1, 2]);
    expect(value).toBe(DEFAULT_CONFIG);
    expect(warnings).toHaveLength(1);
  });
});

describe("t", () => {
  it("자리표시자를 채운다", () => {
    useConfig.setState({ strings: DEFAULT_STRINGS });
    expect(t("multi", { n: 3 })).toMatch(/^3개 선택/);
  });

  it("사용자 문구가 이긴다", () => {
    useConfig.setState({ strings: { ...DEFAULT_STRINGS, editBar: { ...DEFAULT_STRINGS.editBar, lock: "LOCK" } } });
    expect(t("lock")).toBe("LOCK");
    useConfig.setState({ strings: DEFAULT_STRINGS });
  });
});

describe("applyOverrides", () => {
  const base: WidgetDefinition = {
    id: "w", title: "W", component: () => null, root: "builtin",
    defaultSize: { w: 200, h: 100 }, minSize: { w: 100, h: 50 },
    settingsSchema: [
      { key: "mode", label: "모드", type: "select", default: "a", options: [{ value: "a", label: "A" }, { value: "b", label: "B" }] },
      { key: "n", label: "N", type: "number", default: 1 },
    ],
  };

  it("제목·크기·숨김·설정 기본값을 덮는다", () => {
    const { def, warnings } = applyOverrides(base, { title: "새 이름", defaultSize: { w: 300, h: 150 }, hidden: true, settings: { mode: "b", n: 5 } });
    expect(def.title).toBe("새 이름");
    expect(def.defaultSize).toEqual({ w: 300, h: 150 });
    expect(def.hidden).toBe(true);
    expect(def.settingsSchema!.map((f) => (f as { default: unknown }).default)).toEqual(["b", 5]);
    expect(warnings).toEqual([]);
    // 원본은 그대로
    expect((base.settingsSchema![0] as { default: unknown }).default).toBe("a");
  });

  it("맞지 않는 값은 버리고 경고한다", () => {
    const { def, warnings } = applyOverrides(base, { settings: { mode: "z", nope: 1 }, minSize: { w: -1, h: 2 } });
    expect((def.settingsSchema![0] as { default: unknown }).default).toBe("a");
    expect(def.minSize).toEqual(base.minSize);
    expect(warnings).toHaveLength(3);
  });

  it("minSize 가 defaultSize 보다 크면 맞춘다", () => {
    const { def } = applyOverrides(base, { minSize: { w: 500, h: 20 } });
    expect(def.minSize).toEqual({ w: 200, h: 20 });
  });
});
