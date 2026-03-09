import { useTranslation } from "react-i18next";
import type {
  AgentProfile,
  ClaimBid,
  Lease,
  MemoryItem,
  TaskCard,
  ToolManifest,
  ToolRun,
} from "../../types";
import { formatTime, statusBadgeClass } from "./ui";

interface TaskWorkspaceProps {
  currentTasks: TaskCard[];
  currentTask?: TaskCard | null;
  leases: Lease[];
  agents: AgentProfile[];
  claimBids: ClaimBid[];
  toolRuns: ToolRun[];
  tools: ToolManifest[];
  memoryItems: MemoryItem[];
  language: "zh" | "en";
  onSelectTask: (id: string) => void;
}

export function TaskWorkspace({
  currentTasks,
  currentTask,
  leases,
  agents,
  claimBids,
  toolRuns,
  tools,
  memoryItems,
  language,
  onSelectTask,
}: TaskWorkspaceProps) {
  const { t } = useTranslation();
  const activeItemClass = "menu-active";
  const itemClass = "";

  const currentLease = currentTask
    ? leases.find((lease) => lease.taskCardId === currentTask.id)
    : undefined;
  const parentTask = currentTask?.parentId
    ? currentTasks.find((task) => task.id === currentTask.parentId) ?? null
    : null;
  const childTasks = currentTask
    ? currentTasks.filter((task) => task.parentId === currentTask.id)
    : [];
  const taskBids = currentTask
    ? claimBids
        .filter((bid) => bid.taskCardId === currentTask.id)
        .sort((a, b) => b.capabilityScore - a.capabilityScore)
    : [];
  const taskToolRuns = currentTask
    ? toolRuns.filter((run) => run.taskCardId === currentTask.id)
    : [];
  const relatedMemory = currentTask
    ? memoryItems.filter((item) => item.scopeId === currentTask.id)
    : [];

  return (
    <section className="card card-border flex min-h-0 flex-1 bg-base-100">
      <div className="card-body min-h-0 gap-3">
        <div className="flex items-center justify-between">
          <h2 className="card-title">{t("selectedTaskContext")}</h2>
          <span className="badge badge-primary">{currentTasks.length}</span>
        </div>

        <div className="grid min-h-0 flex-1 gap-4 xl:grid-cols-[340px_minmax(0,1fr)]">
          <div className="min-h-0 overflow-auto rounded-box bg-base-100 p-2">
            <ul className="menu menu-sm rounded-box bg-base-100 gap-2">
              {currentTasks.map((task) => {
                const lease = leases.find((item) => item.taskCardId === task.id);
                const owner = agents.find((agent) => agent.id === lease?.ownerAgentId);
                const bidCount = claimBids.filter((bid) => bid.taskCardId === task.id).length;

                return (
                  <li key={task.id}>
                    <a
                      className={currentTask?.id === task.id ? activeItemClass : itemClass}
                      onClick={() => onSelectTask(task.id)}
                    >
                      <div className="w-full">
                        <div className="mb-2 flex items-start justify-between gap-3">
                          <strong className="line-clamp-1">{task.title}</strong>
                          <span className={`badge ${statusBadgeClass(task.status)}`}>
                            {t(`taskStatus.${task.status}`)}
                          </span>
                        </div>
                        <div className="line-clamp-2 text-sm opacity-70">{task.normalizedGoal}</div>
                        <div className="mt-2 flex flex-wrap gap-2">
                          <span className="badge badge-primary">{t("bidsCount", { count: bidCount })}</span>
                          <span className="badge badge-neutral">
                            {owner ? t("ownerLabel", { name: owner.name }) : t("unassigned")}
                          </span>
                        </div>
                      </div>
                    </a>
                  </li>
                );
              })}
            </ul>
          </div>

          <div className="min-h-0 overflow-auto space-y-4">
            {currentTask ? (
              <>
                <div className="card card-border bg-base-100">
                  <div className="card-body gap-3">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <h3 className="card-title">{currentTask.title}</h3>
                        <p className="text-sm text-base-content/60">{currentTask.normalizedGoal}</p>
                      </div>
                      <span className={`badge ${statusBadgeClass(currentTask.status)}`}>
                        {t(`taskStatus.${currentTask.status}`)}
                      </span>
                    </div>
                    <p className="whitespace-pre-wrap text-sm">{currentTask.inputPayload}</p>
                    <div className="stats stats-vertical bg-base-200 lg:stats-horizontal">
                      <div className="stat">
                        <div className="stat-title">{t("created")}</div>
                        <div className="stat-value text-base">
                          {formatTime(currentTask.createdAt, language)}
                        </div>
                      </div>
                      <div className="stat">
                        <div className="stat-title">{t("leaseState")}</div>
                        <div className="stat-value text-base">
                          {currentLease?.state ?? t("notAvailable")}
                        </div>
                      </div>
                      <div className="stat">
                        <div className="stat-title">{t("parentTask")}</div>
                        <div className="stat-value text-base">
                          {parentTask?.title ?? t("notAvailable")}
                        </div>
                      </div>
                    </div>
                  </div>
                </div>

                <div className="grid gap-4 xl:grid-cols-2">
                  <div className="card card-border bg-base-100">
                    <div className="card-body gap-3">
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("childTasks")}</h3>
                        <span className="badge badge-secondary">{childTasks.length}</span>
                      </div>
                      <div className="space-y-2">
                        {childTasks.map((task) => (
                          <button
                            key={task.id}
                            className="btn btn-soft btn-sm justify-start"
                            onClick={() => onSelectTask(task.id)}
                          >
                            <span className={`badge ${statusBadgeClass(task.status)}`}>
                              {t(`taskStatus.${task.status}`)}
                            </span>
                            <span className="truncate">{task.title}</span>
                          </button>
                        ))}
                        {childTasks.length === 0 ? (
                          <p className="text-sm text-base-content/60">{t("noChildTasks")}</p>
                        ) : null}
                      </div>
                    </div>
                  </div>

                  <div className="card card-border bg-base-100">
                    <div className="card-body gap-3">
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("claimBids")}</h3>
                        <span className="badge badge-secondary">{taskBids.length}</span>
                      </div>
                      <div className="space-y-2">
                        {taskBids.map((bid) => {
                          const agent = agents.find((item) => item.id === bid.agentId);
                          return (
                            <div
                              key={bid.id}
                              className="rounded-box bg-base-200 px-4 py-3"
                            >
                              <div className="flex items-center justify-between gap-3">
                                <strong>{agent?.name ?? bid.agentId}</strong>
                                <span className="badge badge-primary">
                                  {t("capabilityScore")}: {bid.capabilityScore}
                                </span>
                              </div>
                              <p className="mt-2 text-sm text-base-content/70">{bid.rationale}</p>
                              <div className="mt-2 flex flex-wrap gap-2">
                                {bid.expectedTools.map((toolId) => (
                                  <span key={toolId} className="badge badge-neutral">
                                    {toolId}
                                  </span>
                                ))}
                              </div>
                            </div>
                          );
                        })}
                        {taskBids.length === 0 ? (
                          <p className="text-sm text-base-content/60">{t("noClaimBids")}</p>
                        ) : null}
                      </div>
                    </div>
                  </div>
                </div>

                <div className="grid gap-4 xl:grid-cols-2">
                  <div className="card card-border bg-base-100">
                    <div className="card-body gap-3">
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("toolRunsTitle")}</h3>
                        <span className="badge badge-secondary">{taskToolRuns.length}</span>
                      </div>
                      <div className="space-y-2">
                        {taskToolRuns.map((run) => {
                          const tool = tools.find((item) => item.id === run.toolId);
                          return (
                            <div
                              key={run.id}
                              className="rounded-box bg-base-200 px-4 py-3"
                            >
                              <div className="flex items-center justify-between gap-3">
                                <strong>{tool?.name ?? run.toolId}</strong>
                                <span className="badge badge-neutral">{run.state}</span>
                              </div>
                              <div className="mt-2 text-sm text-base-content/65">
                                {run.approvalRequired ? t("approval") : t("summaryFeed")}
                              </div>
                            </div>
                          );
                        })}
                        {taskToolRuns.length === 0 ? (
                          <p className="text-sm text-base-content/60">{t("noToolRuns")}</p>
                        ) : null}
                      </div>
                    </div>
                  </div>

                  <div className="card card-border bg-base-100">
                    <div className="card-body gap-3">
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("memoryTitle")}</h3>
                        <span className="badge badge-secondary">{relatedMemory.length}</span>
                      </div>
                      <div className="space-y-2">
                        {relatedMemory.map((memory) => (
                          <div
                            key={memory.id}
                            className="rounded-box bg-base-200 px-4 py-3 text-sm"
                          >
                            <div>{memory.content}</div>
                            <div className="mt-2 flex flex-wrap gap-2">
                              {memory.tags.map((tag) => (
                                <span key={tag} className="badge badge-neutral">
                                  {tag}
                                </span>
                              ))}
                            </div>
                          </div>
                        ))}
                        {relatedMemory.length === 0 ? (
                          <p className="text-sm text-base-content/60">{t("noMemory")}</p>
                        ) : null}
                      </div>
                    </div>
                  </div>
                </div>
              </>
            ) : (
              <div className="hero min-h-72 rounded-box bg-base-200">
                <div className="hero-content text-center">
                  <div>
                    <h3 className="text-lg font-semibold">{t("taskBoard")}</h3>
                    <p className="mt-2 text-sm text-base-content/60">{t("noTasksYet")}</p>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
