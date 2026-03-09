import type { Language } from "../../store/preferencesStore";
import type { TaskStatus } from "../../types";

export function formatTime(value: string | null | undefined, language: Language) {
  if (!value) {
    return language === "zh" ? "待处理" : "Pending";
  }

  const locale = language === "zh" ? "zh-CN" : "en-US";
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(value));
}

export function roleAccent(role: string) {
  const normalized = role.toLowerCase();
  if (normalized.includes("research")) return "#22c55e";
  if (normalized.includes("builder") || normalized.includes("engineer")) return "#0ea5e9";
  if (normalized.includes("review")) return "#f59e0b";
  return "#ec4899";
}

export function statusBadgeClass(status: TaskStatus) {
  switch (status) {
    case "completed":
      return "badge-success";
    case "cancelled":
      return "badge-error";
    case "waiting_approval":
    case "needs_review":
      return "badge-warning";
    default:
      return "badge-info";
  }
}
