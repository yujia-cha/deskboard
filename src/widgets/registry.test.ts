import { describe, expect, it } from "vitest";
import { WIDGETS, widgetById } from "./registry";
import { defaultsOf } from "./types";

describe("widget registry", () => {
  it("has unique ids", () => {
    const ids = WIDGETS.map((w) => w.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("every widget has sane sizes and a component", () => {
    for (const w of WIDGETS) {
      expect(w.minSize.w).toBeLessThanOrEqual(w.defaultSize.w);
      expect(w.minSize.h).toBeLessThanOrEqual(w.defaultSize.h);
      expect(typeof w.component).toBe("function");
      expect(w.title.length).toBeGreaterThan(0);
    }
  });

  it("settings schema keys are unique and defaults match types", () => {
    for (const w of WIDGETS) {
      const keys = (w.settingsSchema ?? []).map((f) => f.key);
      expect(new Set(keys).size).toBe(keys.length);
      const d = defaultsOf(w.settingsSchema);
      for (const f of w.settingsSchema ?? []) {
        if (f.type === "boolean") expect(typeof d[f.key]).toBe("boolean");
        if (f.type === "number") expect(typeof d[f.key]).toBe("number");
        if (f.type === "select") expect(f.options.some((o) => o.value === f.default)).toBe(true);
      }
    }
  });

  it("looks up by id", () => {
    expect(widgetById("clock")?.title).toBe("시계");
    expect(widgetById("nope")).toBeUndefined();
  });
});
