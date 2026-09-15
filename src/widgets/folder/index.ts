import type { WidgetDefinition } from "../types";
import { Folder, type FolderSettings } from "./Folder";

export const folderWidget: WidgetDefinition<FolderSettings> = {
  id: "folder",
  title: "폴더",
  icon: "📁",
  component: Folder,
  defaultSize: { w: 88, h: 104 },
  minSize: { w: 64, h: 64 },
  settingsSchema: [
    { key: "name", label: "이름", type: "text", default: "새 폴더" },
    { key: "image", label: "커스텀 이미지 (비우면 내부 아이콘 미리보기)", type: "path", pick: "image", default: "" },
    { key: "dir", label: "연결 디렉터리 (비우면 자동 생성)", type: "path", pick: "directory", default: "" },
    { key: "layout", label: "펼침 모양", type: "select", default: "grid",
      options: [
        { value: "list", label: "세로 목록" },
        { value: "grid", label: "격자" },
      ] },
    { key: "columns", label: "격자 열 수", type: "number", default: 4, min: 2, max: 8, step: 1 },
    { key: "showName", label: "이름 라벨 표시", type: "boolean", default: true },
    { key: "showItemNames", label: "항목 이름 표시", type: "boolean", default: true },
    { key: "dropMode", label: "드래그&드롭 시", type: "select", default: "move",
      options: [
        { value: "move", label: "이동" },
        { value: "copy", label: "복사" },
      ] },
    { key: "closeOnLaunch", label: "실행 후 닫기", type: "boolean", default: true },
  ],
};
