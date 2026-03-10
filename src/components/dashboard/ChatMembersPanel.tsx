import { useMemo, useState, type ChangeEvent } from "react";
import { useTranslation } from "react-i18next";
import type { AgentProfile, WorkGroup } from "../../types";
import { roleAccent } from "./ui";

interface ChatMembersPanelProps {
  currentGroup: WorkGroup;
  currentMembers: AgentProfile[];
  availableAgents: AgentProfile[];
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
  onAddAgent,
  onRemoveAgent,
}: ChatMembersPanelProps) {
  const { t } = useTranslation();
  const [search, setSearch] = useState("");
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
