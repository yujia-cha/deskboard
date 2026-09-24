import { create } from "zustand";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * GitHub 릴리스 자동 업데이트.
 *
 * 저장소에는 **소스만** 올린다. 태그를 밀면 GitHub Actions 가 Windows 러너에서 NSIS 설치본을
 * 빌드해 서명하고, `latest.json` 과 함께 릴리스에 붙인다 (`.github/workflows/release.yml`).
 * 앱은 `tauri.conf.json` 의 `plugins.updater.endpoints` 가 가리키는 그 `latest.json` 을 본다.
 *
 * 서명 검증은 플러그인이 한다 — `pubkey` 로 풀리지 않는 파일은 받아도 설치되지 않는다.
 * 그래서 릴리스를 누가 올렸는지와 무관하게 **개인 키를 가진 쪽이 만든 것만** 설치된다.
 *
 * 확인은 **시작 후 한 번**, 그리고 사용자가 설정에서 누를 때 한다. 주기적으로 긁지 않는다 —
 * 바탕화면 위젯이 배경에서 네트워크를 쓰는 이유로는 약하다.
 */
export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }                                   // 최신
  | { kind: "available"; version: string; notes: string }
  | { kind: "downloading"; version: string; percent: number | null }
  | { kind: "ready"; version: string }                 // 설치 끝, 재시작만 남음
  | { kind: "error"; message: string };

/** 개발 실행에서는 업데이터 플러그인 자체를 걸지 않는다 (`lib.rs`). */
export const UPDATER_ENABLED = !import.meta.env.DEV;

interface UpdaterStore {
  version: string;
  state: UpdateState;
  checkNow(): Promise<void>;
  install(): Promise<void>;
  restart(): void;
}

// 받아 둔 Update 핸들 — 설치할 때 그대로 써야 한다 (다시 check 하면 두 번 내려받는다).
let pending: Update | null = null;

export const useUpdater = create<UpdaterStore>((set) => ({
  version: "",
  state: { kind: "idle" },

  async checkNow() {
    if (!UPDATER_ENABLED) return;
    set({ state: { kind: "checking" } });
    try {
      const up = await check();
      pending = up ?? null;
      set({ state: up
        ? { kind: "available", version: up.version, notes: (up.body ?? "").trim() }
        : { kind: "none" } });
    } catch (e) {
      set({ state: { kind: "error", message: String(e) } });
    }
  },

  async install() {
    const up = pending;
    if (!up) return;
    // 진행률은 서버가 Content-Length 를 줄 때만 안다 — 없으면 null 로 두고 "받는 중" 만 보여준다.
    let total = 0, got = 0;
    set({ state: { kind: "downloading", version: up.version, percent: null } });
    try {
      await up.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          set({
            state: {
              kind: "downloading",
              version: up.version,
              percent: total > 0 ? Math.min(100, Math.round((got / total) * 100)) : null,
            },
          });
        }
      });
      set({ state: { kind: "ready", version: up.version } });
    } catch (e) {
      set({ state: { kind: "error", message: String(e) } });
    }
  },

  restart() {
    relaunch().catch((e) => set({ state: { kind: "error", message: String(e) } }));
  },
}));

export const selectHasUpdate = (s: UpdaterStore) => s.state.kind === "available" || s.state.kind === "ready";

let started = false;

/**
 * 버전을 읽고, 시작 후 한 번 업데이트를 확인한다. `App.tsx` 가 로드 완료 시 한 번 부른다.
 * StrictMode 의 마운트→언마운트→재마운트나 HMR 로 두 번 불려도 한 번만 예약한다.
 */
export function startUpdater() {
  if (started) return;
  started = true;
  getVersion().then((version) => useUpdater.setState({ version })).catch(() => {});
  // 켜자마자 네트워크를 때리지 않도록 조금 늦춘다 — 시작 직후는 위젯들이 저마다 첫 데이터를
  // 받는 시간이라 가장 붐빈다.
  if (!UPDATER_ENABLED) return;
  window.setTimeout(() => { useUpdater.getState().checkNow(); }, 8000);
}

// 콘솔에서 배지를 시뮬레이션: __updater.setState({ state: { kind: "available", version: "9.9.9", notes: "" } })
if (import.meta.env.DEV && typeof window !== "undefined") (window as any).__updater = useUpdater;
