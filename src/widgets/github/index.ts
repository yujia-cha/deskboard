import type { WidgetDefinition } from "../types";
import { Github, type GithubSettings } from "./Github";

export const githubWidget: WidgetDefinition<GithubSettings> = {
  id: "github",
  title: "GitHub",
  icon: "🐙",
  component: Github,
  defaultSize: { w: 320, h: 260 },
  minSize: { w: 190, h: 80 },
  settingsSchema: [
    // owner/repo 형식만 인정한다 — 그 외는 무시된다. 8개까지만 본다(질의가 커진다).
    { key: "repos", label: "지켜볼 저장소 (쉼표로 여러 개, 8개까지)", type: "text", default: "",
      placeholder: "owner/repo, owner/other" },
    { key: "showContributions", label: "기여도 잔디", type: "boolean", default: true },
    { key: "showRepos", label: "저장소별 현황", type: "boolean", default: true },
    { key: "showNotifications", label: "알림 개수", type: "boolean", default: true },
  ],
};
