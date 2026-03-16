export const DEFAULT_FONT_SIZE = 14;
export const FONT_SIZE_MIN = 12;
export const FONT_SIZE_MAX = 20;
export const FONT_SIZE_STEP = 1;

export const DEFAULT_COMPONENT_SPACING = 4;
export const COMPONENT_SPACING_MIN = 0;
export const COMPONENT_SPACING_MAX = 12;
export const COMPONENT_SPACING_STEP = 1;

export const DEFAULT_COMPONENT_SCALE = 1;
export const COMPONENT_SCALE_MIN = 0.8;
export const COMPONENT_SCALE_MAX = 1.6;
export const COMPONENT_SCALE_STEP = 0.01;

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function normalizeFontSize(value: number) {
  return clamp(Math.round(value), FONT_SIZE_MIN, FONT_SIZE_MAX);
}

export function normalizeComponentSpacing(value: number) {
  return clamp(Math.round(value), COMPONENT_SPACING_MIN, COMPONENT_SPACING_MAX);
}

export function normalizeComponentScale(value: number) {
  return clamp(Math.round(value * 100) / 100, COMPONENT_SCALE_MIN, COMPONENT_SCALE_MAX);
}
