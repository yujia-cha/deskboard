import type { WidgetDefinition } from "../types";
import { Water, type WaterSettings } from "./Water";
import { PRESETS } from "./params";
import { SCENES } from "./scene";

const custom = { showIf: { key: "preset", equals: "custom" } } as const;
const w = PRESETS.water;

export const waterWidget: WidgetDefinition<WaterSettings> = {
  id: "water",
  title: "물통",
  icon: "💧",
  component: Water,
  defaultSize: { w: 200, h: 200 },
  minSize: { w: 120, h: 120 },
  settingsSchema: [
    { key: "note", type: "note", label: "통 안쪽을 끌면 흔들고, 가장자리(점선 고리)를 세로로 끌면 기울입니다(2D 보기에서는 돌립니다). 물체는 집어서 옮길 수 있습니다. 휠: 기울이기 · 더블클릭: 원위치." },
    { key: "view", label: "보기", type: "select", default: "iso",
      options: [
        { value: "iso", label: "등각 3D (복셀)" },
        { value: "isoLite", label: "등각 (2D 유체를 늘임 — 가벼움)" },
        { value: "flat", label: "2D 측면" },
      ] },
    { key: "scene", label: "장면 (src/widgets/water/scene.ts)", type: "select", default: "default",
      options: Object.entries(SCENES).map(([value, s]) => ({ value, label: s.label })) },
    { key: "preset", label: "물성", type: "select", default: "water",
      options: [
        { value: "water", label: "물" },
        { value: "honey", label: "꿀 (끈적함)" },
        { value: "jelly", label: "젤리 (뭉치고 튕김)" },
        { value: "zerog", label: "무중력" },
        { value: "moon", label: "달 (중력 1/6)" },
        { value: "custom", label: "직접 조정" },
      ] },
    // "직접 조정" 일 때만 — 시작값은 물과 같다.
    { key: "gravity", label: "중력", type: "range", default: w.gravity, min: 0, max: 30, step: 0.1, unit: " m/s²", ...custom },
    { key: "gravityDir", label: "중력 방향", type: "range", default: w.gravityDir, min: -180, max: 180, step: 5, unit: "°", ...custom },
    { key: "viscosity", label: "점성", type: "range", default: w.viscosity, min: 0, max: 100, step: 1, ...custom },
    { key: "splash", label: "튐", type: "range", default: w.splash, min: 0, max: 100, step: 1, ...custom },
    { key: "cohesion", label: "표면장력·응집", type: "range", default: w.cohesion, min: 0, max: 100, step: 1, ...custom },
    { key: "wallFriction", label: "벽 마찰", type: "range", default: w.wallFriction, min: 0, max: 100, step: 1, ...custom },
    { key: "bounce", label: "벽 반발", type: "range", default: w.bounce, min: 0, max: 100, step: 1, ...custom },
    { key: "inertia", label: "흔들림 민감도", type: "range", default: w.inertia, min: 0, max: 200, step: 5, unit: "%", ...custom },
    { key: "timeScale", label: "시간 배율", type: "range", default: w.timeScale, min: 25, max: 200, step: 5, unit: "%", ...custom },
    // 늘 보이는 것
    { key: "fill", label: "물 양", type: "range", default: 45, min: 10, max: 80, step: 5, unit: "%" },
    { key: "pixelSize", label: "입자 크기", type: "range", default: 2, min: 1, max: 4, step: 1, unit: " px" },
    { key: "shape", label: "통 모양 (2D 측면)", type: "select", default: "square", showIf: { key: "view", equals: "flat" },
      options: [{ value: "square", label: "사각" }, { value: "round", label: "원형" }] },
    { key: "quality", label: "품질", type: "select", default: "medium",
      options: [
        { value: "low", label: "낮음 (가벼움)" },
        { value: "medium", label: "보통" },
        { value: "high", label: "높음 (압력을 더 정확히)" },
      ] },
    { key: "returnHome", label: "놓으면 제자리로", type: "boolean", default: true },
    { key: "color", label: "물 색 (비우면 강조색)", type: "text", default: "", placeholder: "#4fd1c5" },
  ],
};
