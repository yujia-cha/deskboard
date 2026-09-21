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

    // --- 이 위젯이 어떤 폴더를 보는가 ---
    // 두 가지는 전혀 다른 일이다. 하나의 경로 칸("비우면 자동 생성")으로 섞어 두면
    // 사용자는 지금 자기 파일이 어디로 가는지, 지우면 무엇이 지워지는지 알 수 없다.
    { key: "source", label: "폴더 종류", type: "select", default: "managed",
      options: [
        { value: "managed", label: "새 폴더 만들기 (deskboard 전용)" },
        { value: "link", label: "기존 폴더 연결" },
      ] },
    { key: "sourceNoteManaged", type: "note", showIf: { key: "source", equals: "managed" },
      label:
        "이 위젯만의 빈 폴더를 %APPDATA%\\com.user.deskboard\\folders\\ 아래에 만들어 씁니다.\n" +
        "끌어다 놓은 파일은 그 폴더로 옮겨지거나(이동) 복제됩니다(복사).\n" +
        "위젯을 지워도 폴더와 그 안의 파일은 남습니다." },
    { key: "sourceNoteLink", type: "note", showIf: { key: "source", equals: "link" },
      label:
        "이미 쓰고 있는 폴더(다운로드, 프로젝트 폴더 등)를 그대로 보여 줍니다.\n" +
        "보이는 것이 곧 그 폴더의 실제 내용이라, 여기서 지우면 원본이 지워집니다.\n" +
        "연결만 하는 것이므로 위젯을 지워도 폴더는 그대로입니다." },
    { key: "dir", label: "연결할 폴더", type: "path", pick: "directory", default: "",
      showIf: { key: "source", equals: "link" } },

    { key: "layout", label: "펼침 모양", type: "select", default: "grid",
      options: [
        { value: "list", label: "세로 목록" },
        { value: "grid", label: "격자" },
      ] },
    { key: "columns", label: "격자 열 수", type: "number", default: 4, min: 2, max: 8, step: 1 },
    { key: "showName", label: "이름 라벨 표시", type: "boolean", default: true },
    { key: "showItemNames", label: "항목 이름 표시", type: "boolean", default: true },
    { key: "dropMode", label: "끌어다 놓으면", type: "select", default: "move",
      options: [
        { value: "move", label: "이동 (원본이 폴더로 옮겨집니다)" },
        { value: "copy", label: "복사 (원본은 그대로 둡니다)" },
      ] },
    { key: "closeOnLaunch", label: "실행 후 닫기", type: "boolean", default: true },
  ],
};
