import { useTranslation } from "react-i18next";
import type { AgentProfile, ClaimBid, Lease, TaskCard } from "../../types";
import { statusBadgeClass } from "./ui";

interface TaskBoardProps {
  currentTasks: TaskCard[];
  currentTask?: TaskCard | null;
  leases: Lease[];
  agents: AgentProfile[];
  claimBids: ClaimBid[];
  onSelectTask: (id: string) => void;
}

export function TaskBoard({
  currentTasks,
  currentTask,
  leases,
  agents,
  claimBids,
  onSelectTask,
}: TaskBoardProps) {
  const { t } = useTranslation();
  const activeItemClass = "menu-active";
  const itemClass = "";

  return (
    <section className="card card-border bg-base-100">
      <div className="card-body gap-3">
        <div className="flex items-center justify-between">
          <h2 className="card-title">{t("taskBoard")}</h2>
          <span className="badge badge-primary">
            {currentTasks.length}
          </span>
        </div>

        <ul className="menu menu-sm rounded-box bg-base-100 gap-2 p-2">
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
                    <p className="line-clamp-2 text-sm opacity-70">{task.normalizedGoal}</p>
                    <div className="mt-3 flex flex-wrap gap-2">
                      <span className="badge badge-primary">
                        {t("bidsCount", { count: bidCount })}
                      </span>
                      <span className="badge badge-neutral">
                        {owner ? t("ownerLabel", { name: owner.name }) : t("unassigned")}
                      </span>
                    </div>
                  </div>
                </a>
              </li>
            );
          })}

          {currentTasks.length === 0 ? (
            <li className="menu-disabled px-2 text-sm">{t("noTasksYet")}</li>
          ) : null}
        </ul>
      </div>
    </section>
  );
}
