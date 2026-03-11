import type { Language } from "../../store/preferencesStore";

export type DecisionFilter = "all" | "owner" | "agent" | "failed";

interface ExecutionDecisionFilterProps {
  language: Language;
  value: DecisionFilter;
  onChange: (value: DecisionFilter) => void;
}

const FILTERS: DecisionFilter[] = ["all", "owner", "agent", "failed"];

export function ExecutionDecisionFilter({
  language,
  value,
  onChange,
}: ExecutionDecisionFilterProps) {
  const labels =
    language === "zh"
      ? { all: "全部", owner: "群主决策", agent: "成员决策", failed: "失败决策" }
      : { all: "All", owner: "Owner", agent: "Agent", failed: "Failed" };
  return (
    <div className="flex flex-wrap items-center gap-2">
      {FILTERS.map((filter) => (
        <button
          key={filter}
          type="button"
          className={`btn btn-xs ${value === filter ? "btn-primary" : "btn-ghost"}`}
          onClick={() => onChange(filter)}
        >
          {labels[filter]}
        </button>
      ))}
    </div>
  );
}
