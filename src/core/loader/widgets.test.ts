import { describe, expect, it } from "vitest";
import { evaluateBundle } from "./modules";
import { hostModules } from "./host";
import { validateSchema } from "../widgetTypes";

/**
 * 저장소의 내장 위젯 전부를 **실제 로더로** 불러 본다 — 앱이 런타임에 하는 일과 같다.
 * 위젯이 쓸 수 없는 모듈을 import 했거나(`../../core/...`, npm 패키지), default export 를
 * 빠뜨렸거나, widget.json 이 틀렸으면 여기서 걸린다.
 */
const raw = import.meta.glob("/widgets/*/**/*.{ts,tsx,js,jsx,mjs,css,json}", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

const widgets = new Map<string, Record<string, string>>();
for (const [path, text] of Object.entries(raw)) {
  const [, , id, ...rest] = path.split("/");
  const rel = rest.join("/");
  if (/\.test\.[jt]sx?$/.test(rel)) continue; // 테스트는 배포·번들에서 빠진다
  if (!widgets.has(id)) widgets.set(id, {});
  widgets.get(id)![rel] = text;
}

describe("내장 위젯 폴더", () => {
  it("하나 이상 있다", () => {
    expect(widgets.size).toBeGreaterThan(0);
  });

  for (const [id, files] of widgets) {
    describe(id, () => {
      const manifest = JSON.parse(files["widget.json"] ?? "null");

      it("widget.json 이 올바르다", () => {
        expect(id).toMatch(/^[a-z0-9][a-z0-9_-]{0,47}$/);
        expect(manifest, "widget.json 이 없습니다").toBeTruthy();
        expect(typeof manifest.title).toBe("string");
        expect(manifest.minSize.w).toBeLessThanOrEqual(manifest.defaultSize.w);
        expect(manifest.minSize.h).toBeLessThanOrEqual(manifest.defaultSize.h);
        expect(manifest.singleton).toBeUndefined();
        const { warnings } = validateSchema(manifest.settingsSchema);
        expect(warnings).toEqual([]);
      });

      it("로더로 불러와 컴포넌트를 내보낸다", async () => {
        const entry = manifest?.entry ?? ["index.tsx", "index.jsx", "index.ts", "index.js"].find((f) => f in files);
        if (String(entry).endsWith(".html")) return;
        const exports = await evaluateBundle(id, { entry, files }, hostModules());
        expect(typeof exports.default === "function" || typeof exports.default === "object").toBe(true);
        if ("migrate" in exports) expect(typeof exports.migrate).toBe("function");
      });
    });
  }
});
