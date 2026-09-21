import type { WidgetDefinition } from "../types";
import { GitStatus, type GitStatusSettings } from "./GitStatus";

export const gitStatusWidget: WidgetDefinition<GitStatusSettings> = {
  id: "gitstatus",
  title: "git 저장소",
  icon: "🌿",
  component: GitStatus,
  defaultSize: { w: 300, h: 200 },
  minSize: { w: 180, h: 90 },
  settingsSchema: [
    // 폴더 하나를 고르면 그 아래 2단계까지 훑는다 — 프로젝트를 모아 두는 폴더를 지정하면 된다.
    { key: "root", label: "저장소 폴더", type: "path", pick: "directory", default: "" },
    { key: "root2", label: "저장소 폴더 2 (선택)", type: "path", pick: "directory", default: "" },
    { key: "onlyDirty", label: "손댈 것이 있는 저장소만", type: "boolean", default: false },
    { key: "maxRepos", label: "표시할 저장소 수", type: "number", default: 6, min: 1, max: 30, step: 1 },
  ],
};
