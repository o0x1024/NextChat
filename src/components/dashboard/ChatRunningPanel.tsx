import { useTranslation } from "react-i18next";
import type {
  AgentProfile,
  ClaimBid,
  Lease,
  TaskCard,
  ToolManifest,
  ToolRun,
} from "../../types";
import { statusBadgeClass } from "./ui";

interface ChatRunningPanelProps {
  activeTasks: TaskCard[];
  currentLeases: Lease[];
  currentApprovals: ToolRun[];
  currentGroupTasks: TaskCard[];
  claimBids: ClaimBid[];
  agents: AgentProfile[];
  tools: ToolManifest[];
  highlightedTaskId: string | null;
  onTaskBoardRef: (node: HTMLDivElement | null) => void;
  onApprovalsRef: (node: HTMLDivElement | null) => void;
  onSetTaskCardRef: (taskId: string, node: HTMLDivElement | null) => void;
  onJumpToTaskBoard: (taskId?: string) => void;
  onApproveRun: (toolRunId: string, approved: boolean) => Promise<void>;
}

export function ChatRunningPanel({
  activeTasks,
  currentLeases,
  currentApprovals,
  currentGroupTasks,
  claimBids,
  agents,
  tools,
  highlightedTaskId,
  onTaskBoardRef,
  onApprovalsRef,
  onSetTaskCardRef,
  onJumpToTaskBoard,
  onApproveRun,
}: ChatRunningPanelProps) {
  const { t } = useTranslation();

  return (
    <>
      <section className="card card-border bg-base-100">
        <div className="card-body gap-3">
          <div className="flex items-center justify-between">
            <h3 className="card-title text-base">{t("runningPanel")}</h3>
            <span className="badge badge-secondary">{t("groupBound")}</span>
          </div>
          <p className="text-sm text-base-content/60">{t("runningPanelHint")}</p>
          <div className="stats stats-vertical bg-base-200">
            <div className="stat px-4 py-3">
              <div className="stat-title">{t("activeTasks")}</div>
              <div className="stat-value text-base">{activeTasks.length}</div>
            </div>
            <div className="stat px-4 py-3">
              <div className="stat-title">{t("leasesTitle")}</div>
              <div className="stat-value text-base">{currentLeases.length}</div>
            </div>
            <div className="stat px-4 py-3">
              <div className="stat-title">{t("approvals")}</div>
              <div className="stat-value text-base">{currentApprovals.length}</div>
            </div>
          </div>
        </div>
      </section>

      <section className="card card-border bg-base-100">
        <div className="card-body gap-3" ref={onTaskBoardRef}>
          <div className="flex items-center justify-between">
            <h3 className="card-title text-base">{t("taskBoard")}</h3>
            <span className="badge badge-primary">{activeTasks.length}</span>
          </div>
          <div className="space-y-2">
            {activeTasks.slice(0, 8).map((task) => {
              const lease = currentLeases.find((item) => item.taskCardId === task.id);
              const owner = agents.find((agent) => agent.id === lease?.ownerAgentId);
              const bidCount = claimBids.filter((bid) => bid.taskCardId === task.id).length;
              return (
                <div
                  key={task.id}
                  ref={(node) => {
                    onSetTaskCardRef(task.id, node);
                  }}
                  className={`rounded-box px-4 py-3 transition-colors ${
                    highlightedTaskId === task.id
                      ? "bg-primary/10 ring-1 ring-primary/30"
                      : "bg-base-200"
                  }`}
                >
                  <div className="mb-2 flex items-start justify-between gap-3">
                    <strong className="line-clamp-2 text-sm">{task.title}</strong>
                    <span className={`badge shrink-0 ${statusBadgeClass(task.status)}`}>
                      {t(`taskStatus.${task.status}`)}
                    </span>
                  </div>
                  <p className="line-clamp-2 text-xs text-base-content/65">
                    {task.normalizedGoal}
                  </p>
                  <div className="mt-3 flex flex-wrap gap-2 text-xs">
                    <span className="badge badge-neutral">
                      {owner ? t("ownerLabel", { name: owner.name }) : t("unassigned")}
                    </span>
                    <span className="badge badge-ghost">
                      {t("bidsCount", { count: bidCount })}
                    </span>
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs"
                      onClick={() => onJumpToTaskBoard(task.id)}
                    >
                      {t("openTask")}
                    </button>
                  </div>
                </div>
              );
            })}

            {activeTasks.length === 0 && (
              <div className="alert alert-soft">
                <span className="text-sm">{t("noActiveTasksInGroup")}</span>
              </div>
            )}
          </div>
        </div>
      </section>

      <section className="card card-border bg-base-100">
        <div className="card-body gap-3" ref={onApprovalsRef}>
          <div className="flex items-center justify-between">
            <h3 className="card-title text-base">{t("approvals")}</h3>
            <span className="badge badge-warning">{currentApprovals.length}</span>
          </div>
          <div className="space-y-2">
            {currentApprovals.map((run) => {
              const task = currentGroupTasks.find((item) => item.id === run.taskCardId);
              const tool = tools.find((item) => item.id === run.toolId);
              return (
                <div key={run.id} className="rounded-box bg-warning/10 px-4 py-3">
                  <div className="mb-1 flex items-start justify-between gap-2">
                    <strong className="line-clamp-1 text-sm">{tool?.name ?? run.toolId}</strong>
                    <span className="badge badge-warning shrink-0">{t("approval")}</span>
                  </div>
                  <p className="text-xs text-base-content/70">
                    {task?.title ?? t("toolRunNeedsApproval", { id: run.id.slice(0, 8) })}
                  </p>
                  <div className="mt-3 flex gap-2">
                    <button
                      className="btn btn-primary btn-xs"
                      onClick={() => void onApproveRun(run.id, true)}
                    >
                      {t("approve")}
                    </button>
                    <button
                      className="btn btn-soft btn-xs"
                      onClick={() => void onApproveRun(run.id, false)}
                    >
                      {t("reject")}
                    </button>
                    {task ? (
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => onJumpToTaskBoard(task.id)}
                      >
                        {t("openTask")}
                      </button>
                    ) : null}
                  </div>
                </div>
              );
            })}

            {currentApprovals.length === 0 && (
              <div className="alert alert-soft">
                <span className="text-sm">{t("noPendingApprovals")}</span>
              </div>
            )}
          </div>
        </div>
      </section>
    </>
  );
}
