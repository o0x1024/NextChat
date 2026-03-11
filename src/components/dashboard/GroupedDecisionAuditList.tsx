import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import { decisionAuditBadgeClass } from "./executionDecisionAudit";
import { chainBadgeClass, narrativeChainToken } from "./chainVisual";
import { formatTime } from "./ui";

export interface DecisionAuditListItem {
  id: string;
  taskCardId?: string;
  blockerId?: string;
  senderName: string;
  eventType: string;
  status: "generated" | "failed" | "applied";
  provider?: string;
  model?: string;
  action?: string;
  prompt?: string;
  raw?: string;
  error?: string;
  timestamp: string;
}

interface GroupedDecisionAuditListProps {
  events: DecisionAuditListItem[];
  taskTitleById: Map<string, string>;
  language: Language;
  onJumpToTask: (taskId: string) => void;
  onJumpToBlocker: (blockerId: string) => void;
  onJumpToNarrative: (target: { taskId?: string; blockerId?: string }) => void;
}

function prettifyPayload(raw: string) {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

export function GroupedDecisionAuditList({
  events,
  taskTitleById,
  language,
  onJumpToTask,
  onJumpToBlocker,
  onJumpToNarrative,
}: GroupedDecisionAuditListProps) {
  const { t } = useTranslation();
  const grouped = useMemo(() => {
    const groups = new Map<string, DecisionAuditListItem[]>();
    for (const event of events) {
      const key = event.taskCardId ?? "__ungrouped__";
      const current = groups.get(key) ?? [];
      current.push(event);
      groups.set(key, current);
    }
    return [...groups.entries()].sort((left, right) => {
      const leftLatest = left[1][left[1].length - 1]?.timestamp ?? "";
      const rightLatest = right[1][right[1].length - 1]?.timestamp ?? "";
      return rightLatest.localeCompare(leftLatest);
    });
  }, [events]);

  return (
    <div className="space-y-2">
      {grouped.map(([taskId, items]) => {
        const title =
          (taskId !== "__ungrouped__" && taskTitleById.get(taskId)) ||
          (language === "zh" ? "未绑定任务" : "Unassigned");
        return (
          <details key={taskId} className="overflow-hidden rounded-box bg-base-200" open>
            <summary className="flex cursor-pointer list-none items-center justify-between gap-3 px-3 py-2 [&::-webkit-details-marker]:hidden">
              <div className="flex min-w-0 items-center gap-2">
                <span className="truncate text-sm font-semibold">{title}</span>
                <span className="badge badge-xs badge-ghost">{t("itemsCount", { count: items.length })}</span>
              </div>
              <time className="text-xs text-base-content/60">
                {formatTime(items[items.length - 1]?.timestamp ?? "", language)}
              </time>
            </summary>
            <div className="space-y-2 border-t border-base-content/10 px-3 py-2">
              {items.map((event) => {
                const chain = narrativeChainToken({ taskId: event.taskCardId, blockerId: event.blockerId });
                return <article key={event.id} className="overflow-hidden rounded-box bg-base-100 px-3 py-2 text-xs">
                  <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="font-semibold">{event.senderName}</span>
                      <span className="badge badge-xs badge-ghost">{event.eventType}</span>
                      {chain ? <span className={`badge badge-xs ${chainBadgeClass(chain.key)}`}>{chain.label}</span> : null}
                      <span className={`badge badge-xs ${decisionAuditBadgeClass(event.status)}`}>
                        {event.status}
                      </span>
                    </div>
                    <time className="text-base-content/60">
                      {formatTime(event.timestamp, language)}
                    </time>
                  </div>
                  {event.blockerId || event.taskCardId ? (
                    <div className="mb-2 flex flex-wrap gap-2">
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() =>
                          onJumpToNarrative({ blockerId: event.blockerId, taskId: event.taskCardId })
                        }
                      >
                        {language === "zh" ? "打开叙事" : "Open narrative"}
                      </button>
                      {event.blockerId ? (
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => onJumpToBlocker(event.blockerId as string)}
                      >
                        {language === "zh" ? "定位阻塞" : "Open blocker"}
                      </button>
                      ) : null}
                      {event.taskCardId ? (
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => onJumpToTask(event.taskCardId as string)}
                      >
                        {t("openTask")}
                      </button>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="mb-2 flex flex-wrap gap-2">
                    {event.provider ? <span className="badge badge-xs badge-ghost">{t("provider")}: {event.provider}</span> : null}
                    {event.model ? <span className="badge badge-xs badge-ghost">{t("model")}: {event.model}</span> : null}
                    {event.action ? <span className="badge badge-xs badge-ghost">{t("actions")}: {event.action}</span> : null}
                  </div>
                  {event.prompt ? (
                    <pre className="mb-2 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-200 px-2 py-1 font-mono text-xs">
                      {event.prompt}
                    </pre>
                  ) : null}
                  {event.raw ? (
                    <pre className="mb-2 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-200 px-2 py-1 font-mono text-xs">
                      {prettifyPayload(event.raw)}
                    </pre>
                  ) : null}
                  {event.error ? (
                    <pre className="max-h-28 overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-200 px-2 py-1 font-mono text-xs text-error">
                      {event.error}
                    </pre>
                  ) : null}
                </article>;
              })}
            </div>
          </details>
        );
      })}
    </div>
  );
}
