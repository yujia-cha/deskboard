import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react()],

  // 위젯 폴더의 테스트(vitest)가 `import … from "deskboard"` 를 풀 수 있게. 앱 번들은 widgets/ 를 import 하지 않는다
  // — 위젯은 런타임에 백엔드가 읽어 주고 `core/loader` 가 평가한다.
  test: {
    // tauri 빌드가 리소스(widgets/)를 target/ 아래로 복사한다 — 그 사본까지 테스트하지 않는다
    exclude: ["**/node_modules/**", "src-tauri/**"],
  },
  resolve: {
    alias: { deskboard: decodeURIComponent(new URL("./src/sdk/index.ts", import.meta.url).pathname).replace(/^\/([A-Za-z]:)/, "$1") },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**", "**/widgets/**"],
    },
  },
}));
