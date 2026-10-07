/**
 * 위젯 소스(.tsx/.ts/.jsx/.js) → CommonJS 코드.
 *
 * sucrase 는 첫 위젯을 불러올 때 **지연 로드**한다 (별도 청크). 변환 결과는 소스 해시로
 * IndexedDB 에 캐시해서, 두 번째 시작부터는 변환을 건너뛴다 — 내장 위젯도 이 길로 뜨므로
 * 시작 시간이 여기에 달려 있다.
 *
 * 줄 번호는 원본과 같다 (sucrase 는 줄을 보존한다). 그래서 오류의 `file:line` 이 그대로 맞는다.
 */

/** 변환기 버전 — 바꾸면 캐시가 통째로 무효가 된다. */
const COMPILER_VERSION = "1";

type Sucrase = typeof import("sucrase");
let sucrase: Promise<Sucrase> | null = null;
const loadSucrase = () => (sucrase ??= import("sucrase"));

export class CompileError extends Error {
  constructor(public file: string, public line: number | null, message: string) {
    super(line != null ? `${file}:${line} ${message}` : `${file}: ${message}`);
  }
}

function transformsFor(file: string): ("jsx" | "typescript" | "imports")[] {
  if (file.endsWith(".tsx")) return ["jsx", "typescript", "imports"];
  // .ts 에 jsx 를 켜면 `<T>x` 캐스트를 태그로 읽는다
  if (file.endsWith(".ts")) return ["typescript", "imports"];
  return ["jsx", "imports"];
}

/**
 * `scope`(위젯 id)와 `file` 로 캐시 칸 하나를 쓴다 — 칸마다 마지막 소스의 결과만 남겨 두므로,
 * 위젯을 고칠 때마다 옛 결과가 쌓이지 않는다.
 */
export async function compile(scope: string, file: string, source: string): Promise<string> {
  const slot = `${scope}/${file}`;
  const digest = await hash(`${COMPILER_VERSION}\0${source}`);
  const hit = await cache.get(slot, digest);
  if (hit != null) return hit;
  const { transform } = await loadSucrase();
  let code: string;
  try {
    code = transform(source, {
      transforms: transformsFor(file),
      jsxRuntime: "automatic",
      production: true,
      filePath: file,
    }).code;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    // sucrase: "Unexpected token (3:5)"
    const m = /\((\d+):(\d+)\)/.exec(msg);
    throw new CompileError(file, m ? Number(m[1]) : null, msg);
  }
  cache.set(slot, digest, code);
  return code;
}

async function hash(text: string): Promise<string> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return fallbackHash(text);
  const buf = await subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** crypto.subtle 이 없는 환경용 (FNV-1a 두 벌). 캐시 키일 뿐이라 충돌 저항은 필요 없다. */
function fallbackHash(text: string): string {
  let a = 0x811c9dc5, b = 0x01000193;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    a = Math.imul(a ^ c, 0x01000193) >>> 0;
    b = Math.imul(b ^ c, 0x5bd1e995) >>> 0;
  }
  return `${a.toString(16)}${b.toString(16)}${text.length.toString(16)}`;
}

/**
 * 변환 캐시. IndexedDB 가 있으면 거기에, 없으면(테스트·막힌 환경) 메모리에만.
 * 캐시는 최적화일 뿐이다 — 실패하면 조용히 다시 변환한다.
 */
const cache = (() => {
  type Entry = { digest: string; code: string };
  const mem = new Map<string, Entry>();
  // 저장 형식을 바꾸며 이름을 올렸다 — 옛 DB(키마다 결과가 쌓이던 것)는 지운다.
  const DB = "deskboard-widget-cache-v2", STORE = "compiled";
  let db: Promise<IDBDatabase | null> | null = null;
  const open = () =>
    (db ??= new Promise((resolve) => {
      if (typeof indexedDB === "undefined") return resolve(null);
      try { indexedDB.deleteDatabase("deskboard-widget-cache"); } catch { /* 없으면 그만 */ }
      const req = indexedDB.open(DB, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => resolve(null);
    }));
  const valid = (e: unknown): e is Entry =>
    !!e && typeof (e as Entry).digest === "string" && typeof (e as Entry).code === "string";
  return {
    async get(slot: string, digest: string): Promise<string | null> {
      const m = mem.get(slot);
      if (m) return m.digest === digest ? m.code : null;
      const d = await open();
      if (!d) return null;
      return new Promise((resolve) => {
        try {
          const req = d.transaction(STORE).objectStore(STORE).get(slot);
          req.onsuccess = () => resolve(valid(req.result) && req.result.digest === digest ? req.result.code : null);
          req.onerror = () => resolve(null);
        } catch { resolve(null); }
      });
    },
    async set(slot: string, digest: string, code: string) {
      mem.set(slot, { digest, code });
      const d = await open();
      if (!d) return;
      try { d.transaction(STORE, "readwrite").objectStore(STORE).put({ digest, code }, slot); } catch { /* 캐시일 뿐 */ }
    },
  };
})();
