import type { KeyboardEvent, ReactNode } from "react";

interface ContinuousRangeControlProps {
  iconClass: string;
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  badge: string;
  minIndicator: ReactNode;
  maxIndicator: ReactNode;
  onChange: (value: number) => void;
  onCommit: () => void;
}

export function ContinuousRangeControl({
  iconClass,
  label,
  value,
  min,
  max,
  step,
  badge,
  minIndicator,
  maxIndicator,
  onChange,
  onCommit,
}: ContinuousRangeControlProps) {
  const handleInputChange = (nextValue: string) => {
    const parsedValue = Number(nextValue);
    if (!Number.isFinite(parsedValue)) {
      return;
    }
    onChange(parsedValue);
  };

  const handleRangeKeyUp = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Tab") {
      return;
    }
    onCommit();
  };

  const handleNumberKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      onCommit();
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <label className="text-xs font-bold flex items-center gap-2">
          <i className={iconClass} />
          {label}
        </label>
        <span className="badge badge-primary badge-sm font-mono">{badge}</span>
      </div>

      <div className="flex items-center gap-3">
        <span className="flex min-w-6 justify-center text-[10px] opacity-40">{minIndicator}</span>
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          className="range range-primary range-xs flex-1"
          value={value}
          onChange={(event) => handleInputChange(event.target.value)}
          onPointerUp={onCommit}
          onKeyUp={handleRangeKeyUp}
        />
        <span className="flex min-w-6 justify-center text-sm opacity-40">{maxIndicator}</span>
        <input
          type="number"
          min={min}
          max={max}
          step={step}
          value={value}
          className="input input-bordered input-xs w-20 text-right font-mono"
          onChange={(event) => handleInputChange(event.target.value)}
          onBlur={onCommit}
          onKeyDown={handleNumberKeyDown}
        />
      </div>
    </div>
  );
}
