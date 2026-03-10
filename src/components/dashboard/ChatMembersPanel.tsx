import { useMemo, useState, type ChangeEvent } from "react";
import { useTranslation } from "react-i18next";
import type { AgentProfile, TaskCard, WorkGroup } from "../../types";
import { roleAccent } from "./ui";

interface ChatMembersPanelProps {
  currentGroup: WorkGroup;
  currentMembers: AgentProfile[];
  availableAgents: AgentProfile[];
  currentGroupTasks: TaskCard[];
  onCancelTask: (taskCardId: string) => Promise<void>;
  onAddAgent: (agentId: string) => Promise<void>;
  onRemoveAgent: (agent: AgentProfile) => Promise<void>;
}

function isBuiltinGroupOwner(agent: AgentProfile) {
  const role = agent.role.trim().toLowerCase();
  return role === "group owner" || agent.name === "群主";
}

export function ChatMembersPanel({
  currentGroup,
  currentMembers,
  availableAgents,
  currentGroupTasks,
  onCancelTask,
  onAddAgent,
  onRemoveAgent,
}: ChatMembersPanelProps) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
  const [stoppingAgentId, setStoppingAgentId] = useState<string | null>(null);
  const filteredAvailableAgents = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) {
      return availableAgents;
    }
    return availableAgents.filter((agent) => {
      const haystack = [agent.name, agent.role, agent.objective, agent.id]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  }, [availableAgents, search]);

  async function handleStopAgent(agentId: string) {
    if (stoppingAgentId) {
      return;
    }
    const stoppableTasks = currentGroupTasks.filter(
      (task) =>
        task.assignedAgentId === agentId &&
        !["completed", "cancelled", "needs_review"].includes(task.status),
    );
    if (stoppableTasks.length === 0) {
      return;
    }
    setStoppingAgentId(agentId);
    try {
      const results = await Promise.allSettled(stoppableTasks.map((task) => onCancelTask(task.id)));
      const failed = results.find(
        (result): result is PromiseRejectedResult => result.status === "rejected",
      );
      if (failed) {
        throw failed.reason;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      window.alert(message);
    } finally {
      setStoppingAgentId(null);
    }
  }

  return (
    <>
      <section className="card card-border bg-base-100">
        <div className="card-body gap-3">
          <div className="flex items-center justify-between gap-2">
            <h3 className="card-title text-base">{t("members")}</h3>
            <span className="badge badge-ghost">{t("itemsCount", { count: currentMembers.length })}</span>
          </div>
          <p className="text-sm text-base-content/60">{t("membersPanelHint")}</p>
          <div className="space-y-2">
            {currentMembers.map((agent) => {
              const accent = roleAccent(agent.role);
              const lockOwner = isBuiltinGroupOwner(agent);
              const stoppableTaskCount = currentGroupTasks.filter(
                (task) =>
                  task.assignedAgentId === agent.id &&
                  !["completed", "cancelled", "needs_review"].includes(task.status),
              ).length;
              const stoppingCurrentAgent = stoppingAgentId === agent.id;
              return (
                <div
                  key={agent.id}
                  className="flex items-center justify-between rounded-box bg-base-200 px-3 py-2"
                >
                  <div className="min-w-0">
                    <div className="truncate text-sm font-semibold" style={{ color: accent }}>
                      {agent.name}
                    </div>
                    <div className="truncate text-xs text-base-content/60">{agent.role}</div>
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs"
                      disabled={lockOwner || stoppableTaskCount === 0 || Boolean(stoppingAgentId)}
                      title={
                        lockOwner
                          ? t("groupOwnerLocked")
                          : stoppableTaskCount > 0
                            ? t("stopExecution")
                            : t("noAgentTasksToStop")
                      }
                      onClick={() => {
                        void handleStopAgent(agent.id);
                      }}
                    >
                      <i
                        className={`${
                          stoppingCurrentAgent
                            ? "fas fa-spinner fa-spin text-[10px]"
                            : "fas fa-stop text-[10px]"
                        }`}
                      />
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs text-error"
                      disabled={lockOwner}
                      title={lockOwner ? t("groupOwnerLocked") : t("delete")}
                      onClick={() => void onRemoveAgent(agent)}
                    >
                      <i className="fas fa-user-minus" />
                    </button>
                  </div>
                </div>
              );
            })}
            {currentMembers.length === 0 ? (
              <div className="alert alert-soft">
                <span className="text-sm">{t("noMembersInGroup")}</span>
              </div>
            ) : null}
          </div>
        </div>
      </section>

      <section className="card card-border bg-base-100">
        <div className="card-body gap-3">
          <div className="flex items-center justify-between">
            <h3 className="card-title text-base">{t("addMembers")}</h3>
            <span className="badge badge-secondary">{currentGroup.name}</span>
          </div>
          <label className="input input-bordered input-sm flex items-center gap-2">
            <i className="fas fa-search text-xs text-base-content/40" />
            <input
              type="text"
              className="grow"
              value={search}
              placeholder={t("searchMembersToAdd")}
              onChange={(event: ChangeEvent<HTMLInputElement>) => setSearch(event.target.value)}
            />
          </label>
          <div className="space-y-2">
            {filteredAvailableAgents.map((agent) => (
              <div
                key={agent.id}
                className="flex items-center justify-between rounded-box bg-base-200 px-3 py-2"
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold">{agent.name}</div>
                  <div className="truncate text-xs text-base-content/60">{agent.role}</div>
                </div>
                <button
                  type="button"
                  className="btn btn-primary btn-xs"
                  onClick={() => void onAddAgent(agent.id)}
                >
                  <i className="fas fa-user-plus mr-1 text-[10px]" />
                  {t("add")}
                </button>
              </div>
            ))}
            {availableAgents.length === 0 ? (
              <div className="alert alert-soft">
                <span className="text-sm">{t("noAvailableAgentsToAdd")}</span>
              </div>
            ) : null}
            {availableAgents.length > 0 && filteredAvailableAgents.length === 0 ? (
              <div className="alert alert-soft">
                <span className="text-sm">{t("noMembersSearchResult")}</span>
              </div>
            ) : null}
          </div>
        </div>
      </section>
    </>
  );
}
