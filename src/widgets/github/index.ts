import type { WidgetDefinition } from "../types";
import { Github, type GithubSettings } from "./Github";

export const githubWidget: WidgetDefinition<GithubSettings> = {
  id: "github",
  title: "GitHub",
  icon: "🐙",
  component: Github,
  defaultSize: { w: 280, h: 160 },
  minSize: { w: 170, h: 80 },
  settingsSchema: [
    // owner/repo 형식만 인정한다 — 그 외는 무시된다.
    { key: "repos", label: "CI 를 볼 저장소 (쉼표로 여러 개)", type: "text", default: "",
      placeholder: "owner/repo, owner/other" },
    { key: "showNotifications", label: "알림 개수", type: "boolean", default: true },
    { key: "showChecks", label: "CI 상태", type: "boolean", default: true },
  ],
};
