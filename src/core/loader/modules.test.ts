import { describe, expect, it } from "vitest";
import { evaluateBundle, ModuleError } from "./modules";
import { CompileError } from "./compile";

const host = { react: { __esModule: true, default: { tag: "react" }, useState: "useState" } };

describe("evaluateBundle", () => {
  it("상대 경로·확장자 생략·index 를 푼다", async () => {
    const out = await evaluateBundle("w", {
      entry: "index.tsx",
      files: {
        "index.tsx": 'import { a } from "./lib"; import b from "./sub"; export default () => a + b;',
        "lib.ts": "export const a: number = 1;",
        "sub/index.js": "export default 2;",
      },
    }, host);
    expect((out.default as () => number)()).toBe(3);
  });

  it("호스트 모듈을 그대로 넘긴다 (default import 포함)", async () => {
    const out = await evaluateBundle("w", {
      entry: "index.js",
      files: { "index.js": 'import React, { useState } from "react"; export default [React.tag, useState];' },
    }, host);
    expect(out.default).toEqual(["react", "useState"]);
  });

  it("JSON 을 import 한다", async () => {
    const out = await evaluateBundle("w", {
      entry: "index.js",
      files: { "index.js": 'import data from "./d.json"; export default data.n;', "d.json": '{"n": 7}' },
    }, host);
    expect(out.default).toBe(7);
  });

  it("쓸 수 없는 모듈은 이유와 함께 거절한다", async () => {
    await expect(evaluateBundle("w", {
      entry: "index.js",
      files: { "index.js": 'import fs from "fs"; export default fs;' },
    }, host)).rejects.toThrow(ModuleError);
  });

  it("없는 상대 경로", async () => {
    await expect(evaluateBundle("w", {
      entry: "index.js",
      files: { "index.js": 'import x from "./nope"; export default x;' },
    }, host)).rejects.toThrow(/nope/);
  });

  it("문법 오류는 파일과 줄을 알려 준다", async () => {
    const err = await evaluateBundle("w", {
      entry: "index.jsx",
      files: { "index.jsx": "export default function A() {\n  return <div>;\n}" },
    }, host).catch((e) => e);
    expect(err).toBeInstanceOf(CompileError);
    expect(err.file).toBe("index.jsx");
    expect(err.line).toBe(2);
  });

  it("순환 import 도 CommonJS 처럼 끝난다", async () => {
    const out = await evaluateBundle("w", {
      entry: "a.js",
      files: {
        "a.js": 'import { b } from "./b"; export const a = 1; export default () => b();',
        "b.js": 'import { a } from "./a"; export const b = () => a + 1;',
      },
    }, host);
    expect((out.default as () => number)()).toBe(2);
  });
});
