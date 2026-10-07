import { compile } from "./compile";

/**
 * 위젯 번들(파일 맵)을 평가해 모듈 exports 를 얻는다.
 *
 * import 는 sucrase 가 `require()` 로 바꿔 두므로, 여기서 `require` 를 직접 만들어 넘긴다:
 * - `host` 에 있는 이름(react, deskboard, date-fns …) → 호스트가 가진 그 모듈. **React 는 하나뿐**
 *   이어야 hooks 가 동작한다.
 * - `./x` · `../x` → 같은 번들 안의 파일 (확장자·index 생략 가능)
 * - `.css` → `<style data-widget=id>` 로 주입
 * - `.json` → JSON.parse
 * - 그 밖의 것은 **오류** — 위젯 폴더 밖의 npm 패키지는 없다.
 */

export interface Bundle { entry: string; files: Record<string, string> }

export class ModuleError extends Error {}

const EXTS = [".tsx", ".ts", ".jsx", ".js", ".mjs", ".json", ".css"];

function resolveRelative(from: string, spec: string, files: Record<string, string>): string | null {
  const parts = from.split("/").slice(0, -1);
  for (const seg of spec.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") { if (!parts.length) return null; parts.pop(); } else parts.push(seg);
  }
  const base = parts.join("/");
  for (const c of [base, ...EXTS.map((e) => base + e), ...EXTS.map((e) => `${base}/index${e}`)])
    if (c in files) return c;
  return null;
}

/** 다른 위젯 스타일·사용자 CSS 보다 앞에 둔다 — `user.css` 가 항상 이겨야 한다. */
function injectCss(widgetId: string, file: string, css: string) {
  if (typeof document === "undefined") return;
  const el = document.createElement("style");
  el.dataset.widget = widgetId;
  el.dataset.file = file;
  el.textContent = css;
  const user = document.getElementById("deskboard-user-css");
  document.head.insertBefore(el, user);
}

export function removeWidgetCss(widgetId: string) {
  if (typeof document === "undefined") return;
  document.head.querySelectorAll(`style[data-widget="${CSS.escape(widgetId)}"]`).forEach((el) => el.remove());
}

/**
 * 번들 전체를 미리 변환한 뒤(비동기) 진입 파일부터 동기로 평가한다.
 * 순환 import 는 CommonJS 처럼 "아직 덜 채워진 exports" 를 돌려준다.
 */
export async function evaluateBundle(
  widgetId: string,
  bundle: Bundle,
  host: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const code: Record<string, string> = {};
  await Promise.all(
    Object.entries(bundle.files)
      .filter(([f]) => /\.(tsx|ts|jsx|js|mjs)$/.test(f))
      .map(async ([f, src]) => { code[f] = await compile(widgetId, f, src); }),
  );
  if (!(bundle.entry in bundle.files)) throw new ModuleError(`진입 파일 ${bundle.entry} 이(가) 없습니다`);

  removeWidgetCss(widgetId);
  const cache = new Map<string, { exports: Record<string, unknown> }>();

  const load = (file: string): Record<string, unknown> => {
    const cached = cache.get(file);
    if (cached) return cached.exports;
    if (file.endsWith(".css")) {
      injectCss(widgetId, file, bundle.files[file]);
      cache.set(file, { exports: {} });
      return {};
    }
    if (file.endsWith(".json")) {
      let value: Record<string, unknown>;
      try { value = JSON.parse(bundle.files[file]); } catch (e) { throw new ModuleError(`${file}: ${(e as Error).message}`); }
      const mod = { exports: { default: value, ...value, __esModule: true } };
      cache.set(file, mod);
      return mod.exports;
    }
    const mod = { exports: {} as Record<string, unknown> };
    cache.set(file, mod);
    const require = (spec: string) => {
      if (Object.prototype.hasOwnProperty.call(host, spec)) return host[spec];
      if (spec.startsWith("./") || spec.startsWith("../")) {
        const target = resolveRelative(file, spec, bundle.files);
        if (!target) throw new ModuleError(`${file}: "${spec}" 를 찾을 수 없습니다`);
        return load(target);
      }
      throw new ModuleError(
        `${file}: "${spec}" 는 쓸 수 없는 모듈입니다 — 쓸 수 있는 것: ${Object.keys(host).join(", ")}, 그리고 위젯 폴더 안의 파일`,
      );
    };
    // sourceURL 이 붙어 있어 DevTools·스택에 dbw://<id>/<file> 로 보인다.
    const fn = new Function("require", "module", "exports", `${code[file]}\n//# sourceURL=dbw://${widgetId}/${file}`);
    fn(require, mod, mod.exports);
    return mod.exports;
  };

  return load(bundle.entry);
}
