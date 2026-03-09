import { useTranslation } from "react-i18next";
import type { AgentProfile, Lease, TaskCard } from "../../types";

interface LeasePanelProps {
  leases: Lease[];
  currentTasks: TaskCard[];
  agents: AgentProfile[];
}

export function LeasePanel({ leases, currentTasks, agents }: LeasePanelProps) {
  const { t } = useTranslation();
  const taskIds = new Set(currentTasks.map((task) => task.id));
  const activeLeases = leases.filter(
    (lease) => taskIds.has(lease.taskCardId) && lease.state !== "released",
  );

  return (
    <section className="card card-border bg-base-100">
      <div className="card-body gap-4">
        <div className="flex items-center justify-between">
          <h2 className="card-title">{t("leasesTitle")}</h2>
          <span className="badge badge-secondary">
            {activeLeases.length}
          </span>
        </div>

        <ul className="list gap-3">
          {activeLeases.map((lease) => {
            const task = currentTasks.find((item) => item.id === lease.taskCardId);
            const owner = agents.find((agent) => agent.id === lease.ownerAgentId);

            return (
              <li
                key={lease.id}
                className="list-row rounded-box bg-base-200"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-3">
                    <strong className="line-clamp-1">{task?.title ?? lease.taskCardId}</strong>
                    <span className="badge badge-neutral shrink-0">{lease.state}</span>
                  </div>
                  <div className="mt-2 text-sm text-base-content/65">
                    {owner ? t("ownerAgent", { name: owner.name }) : t("unassigned")}
                  </div>
                </div>
              </li>
            );
          })}

          {activeLeases.length === 0 ? (
            <li className="list-row rounded-box">
              <div className="alert alert-soft">
                <span className="text-sm">{t("noActiveLeases")}</span>
              </div>
            </li>
          ) : null}
        </ul>
      </div>
    </section>
  );
}
