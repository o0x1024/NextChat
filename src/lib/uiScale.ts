const UI_SCALE_VARIABLES = [
  ["--radius-selector", "--ui-radius-selector-base"],
  ["--radius-field", "--ui-radius-field-base"],
  ["--radius-box", "--ui-radius-box-base"],
  ["--size-selector", "--ui-size-selector-base"],
  ["--size-field", "--ui-size-field-base"],
] as const;

function divideCssValue(value: string, scale: number) {
  const match = value.trim().match(/^(-?\d*\.?\d+)([a-z%]*)$/i);
  if (!match) {
    return value.trim();
  }

  const numeric = Number(match[1]);
  const unit = match[2] || "px";

  if (!Number.isFinite(numeric) || scale === 0) {
    return value.trim();
  }

  return `${numeric / scale}${unit}`;
}

export function syncUiScaleBaseVariables(root: HTMLElement, scale: number) {
  const styles = getComputedStyle(root);

  UI_SCALE_VARIABLES.forEach(([sourceVar, baseVar]) => {
    const currentValue = styles.getPropertyValue(sourceVar).trim();
    if (!currentValue) {
      return;
    }
    root.style.setProperty(baseVar, divideCssValue(currentValue, scale));
  });
}
