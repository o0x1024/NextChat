import { useTranslation } from "react-i18next";
import type {
  AgentProfile,
  MemoryItem,
  SkillPack,
  TaskCard,
  ToolManifest,
  ToolRun,
  WorkGroup,
} from "../../types";
import {
  joinPolicyList,
  selectedSkillsForAgent,
  toolExposureReason,
  type ToolExposureReason,
} from "./agentPermissions";
import { roleAccent } from "./ui";

interface AgentWorkspaceProps {
  agents: AgentProfile[];
  skills: SkillPack[];
  tools: ToolManifest[];
  workGroups: WorkGroup[];
  taskCards: TaskCard[];
  toolRuns: ToolRun[];
  memoryItems: MemoryItem[];
  currentAgent?: AgentProfile | null;
  onSelectAgent: (id: string) => void;
}

function riskBadgeClass(riskLevel: ToolManifest["riskLevel"]) {
  switch (riskLevel) {
    case "high":
      return "badge-error";
    case "medium":
      return "badge-warning";
    default:
      return "badge-success";
  }
}

function exposureBadgeClass(reason: ToolExposureReason) {
  switch (reason) {
    case "blocked_by_permission":
      return "badge-error";
    case "blocked_by_skill":
      return "badge-warning";
    case "not_bound":
      return "badge-ghost";
    default:
      return "badge-success";
  }
}

export function AgentWorkspace({
  agents,
  skills,
  tools,
  workGroups,
  taskCards,
  toolRuns,
  memoryItems,
  currentAgent,
  onSelectAgent,
}: AgentWorkspaceProps) {
  const { t } = useTranslation();
  const membershipGroups = currentAgent
    ? workGroups.filter((group) => group.memberAgentIds.includes(currentAgent.id))
    : [];
  const assignedTasks = currentAgent
    ? taskCards.filter((task) => task.assignedAgentId === currentAgent.id)
    : [];
  const ownedToolRuns = currentAgent
    ? toolRuns.filter((run) => run.agentId === currentAgent.id)
    : [];
  const activeToolRuns = ownedToolRuns.filter(
    (run) => run.state === "queued" || run.state === "running",
  );
  const agentMemory = currentAgent
    ? memoryItems.filter((item) => item.scope === "agent" && item.scopeId === currentAgent.id)
    : [];
  const selectedSkills = currentAgent
    ? selectedSkillsForAgent(currentAgent, skills)
    : [];
  const mappedSkills = currentAgent
    ? currentAgent.skillIds.map((skillId) => skills.find((skill) => skill.id === skillId))
    : [];
  const allowedSkillTags = Array.from(
    new Set(selectedSkills.flatMap((skill) => skill.allowedToolTags)),
  );
  const toolAvailability = currentAgent
    ? tools.map((tool) => ({
        tool,
        reason: toolExposureReason(currentAgent, tool, selectedSkills),
      }))
    : [];
  const availableToolCount = toolAvailability.filter(
    ({ reason }) => reason === "available",
  ).length;
  const permissionRuleCount = currentAgent
    ? currentAgent.permissionPolicy.allowToolIds.length +
      currentAgent.permissionPolicy.denyToolIds.length +
      currentAgent.permissionPolicy.requireApprovalToolIds.length +
      currentAgent.permissionPolicy.allowFsRoots.length +
      currentAgent.permissionPolicy.allowNetworkDomains.length
    : 0;

  return (
    <section className="card card-border flex min-h-0 flex-1 bg-base-100">
      <div className="card-body min-h-0 gap-3">
        <div className="flex items-center justify-between">
          <h2 className="card-title">{t("agentDirectory")}</h2>
          <span className="badge badge-primary">{agents.length}</span>
        </div>

        <div className="grid min-h-0 flex-1 gap-4 xl:grid-cols-[300px_minmax(0,1fr)]">
          <div className="min-h-0 overflow-auto rounded-box bg-base-100 p-2">
            <ul className="menu menu-sm rounded-box bg-base-100 gap-2">
              {agents.map((agent) => {
                const accent = roleAccent(agent.role);
                const selected = currentAgent?.id === agent.id;
                return (
                  <li key={agent.id}>
                    <a className={selected ? "menu-active" : undefined} onClick={() => onSelectAgent(agent.id)}>
                      <div
                        className="grid h-9 w-9 shrink-0 place-items-center rounded-btn border bg-base-100 text-xs font-semibold"
                        style={{ borderColor: accent, color: accent }}
                      >
                        {agent.avatar}
                      </div>
                      <div className="min-w-0">
                        <span className="block truncate font-medium">{agent.name}</span>
                        <span className="block truncate text-xs opacity-60">{agent.role}</span>
                      </div>
                    </a>
                  </li>
                );
              })}
            </ul>
          </div>

          <div className="min-h-0 space-y-4 overflow-auto">
            <div className="card card-border bg-base-100">
              <div className="card-body gap-3">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="flex items-start gap-3">
                    <div
                      className="grid h-14 w-14 shrink-0 place-items-center rounded-box bg-base-200 text-lg font-semibold"
                      style={{
                        color: currentAgent ? roleAccent(currentAgent.role) : undefined,
                      }}
                    >
                      {currentAgent?.avatar ?? "AI"}
                    </div>
                    <div>
                      <h3 className="card-title">
                        {currentAgent?.name ?? t("createAgentAction")}
                      </h3>
                      <p className="text-sm text-base-content/60">
                        {currentAgent?.objective ?? t("agentConfigTitle")}
                      </p>
                    </div>
                  </div>

                  <div className="flex flex-wrap justify-end gap-2">
                    <span className="badge badge-neutral">
                      {currentAgent?.role ?? t("notAvailable")}
                    </span>
                    <span
                      className={`badge ${
                        currentAgent?.canSpawnSubtasks ? "badge-success" : "badge-ghost"
                      }`}
                    >
                      {currentAgent?.canSpawnSubtasks
                        ? t("subtasksEnabled")
                        : t("subtasksDisabled")}
                    </span>
                  </div>
                </div>

                <div className="stats stats-vertical bg-base-200 2xl:stats-horizontal">
                  <div className="stat">
                    <div className="stat-title">{t("provider")}</div>
                    <div className="stat-value text-base">
                      {currentAgent?.modelPolicy.provider ?? t("notAvailable")}
                    </div>
                  </div>
                  <div className="stat">
                    <div className="stat-title">{t("model")}</div>
                    <div className="stat-value text-base">
                      {currentAgent?.modelPolicy.model ?? t("notAvailable")}
                    </div>
                  </div>
                  <div className="stat">
                    <div className="stat-title">{t("temperature")}</div>
                    <div className="stat-value text-base">
                      {currentAgent?.modelPolicy.temperature ?? t("notAvailable")}
                    </div>
                  </div>
                  <div className="stat">
                    <div className="stat-title">{t("parallelRuns")}</div>
                    <div className="stat-value text-base">
                      {currentAgent?.maxParallelRuns ?? 0}
                    </div>
                  </div>
                  <div className="stat">
                    <div className="stat-title">{t("workGroupMembership")}</div>
                    <div className="stat-value text-base">{membershipGroups.length}</div>
                  </div>
                  <div className="stat">
                    <div className="stat-title">{t("assignedTasks")}</div>
                    <div className="stat-value text-base">{assignedTasks.length}</div>
                  </div>
                </div>
              </div>
            </div>

            <div className="grid gap-4 xl:grid-cols-2">
              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <h3 className="card-title text-base">{t("executionPolicy")}</h3>
                    <span className="badge badge-secondary">
                      {currentAgent?.maxParallelRuns ?? 0}
                    </span>
                  </div>

                  <div className="space-y-2 text-sm">
                    <div className="rounded-box bg-base-200 px-3 py-2">
                      <span>{t("canSpawnSubtasks")}</span>
                      <span className="font-medium">
                        {currentAgent?.canSpawnSubtasks ? t("enabled") : t("disabled")}
                      </span>
                    </div>
                    <div className="rounded-box bg-base-200 px-3 py-2">
                      <span>{t("skills")}</span>
                      <span className="font-medium">{currentAgent?.skillIds.length ?? 0}</span>
                    </div>
                    <div className="rounded-box bg-base-200 px-3 py-2">
                      <span>{t("tools")}</span>
                      <span className="font-medium">{currentAgent?.toolIds.length ?? 0}</span>
                    </div>
                    <div className="rounded-box bg-base-200 px-3 py-2">
                      <span>{t("permissions")}</span>
                      <span className="font-medium">{permissionRuleCount}</span>
                    </div>
                  </div>
                </div>
              </div>

              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <h3 className="card-title text-base">{t("taskLoad")}</h3>
                    <span className="badge badge-secondary">{ownedToolRuns.length}</span>
                  </div>

                  <div className="stats bg-base-200">
                    <div className="stat px-4 py-3">
                      <div className="stat-title">{t("assignedTasks")}</div>
                      <div className="stat-value text-base">{assignedTasks.length}</div>
                    </div>
                    <div className="stat px-4 py-3">
                      <div className="stat-title">{t("activeToolRuns")}</div>
                      <div className="stat-value text-base">{activeToolRuns.length}</div>
                    </div>
                  </div>

                  <ul className="menu menu-sm rounded-box bg-base-100 p-2">
                    {assignedTasks.slice(0, 4).map((task) => (
                      <li key={task.id}>
                        <div className="flex items-center justify-between gap-3">
                          <span className="truncate">{task.title}</span>
                          <span className="badge badge-neutral">
                            {t(`taskStatus.${task.status}`)}
                          </span>
                        </div>
                      </li>
                    ))}
                    {assignedTasks.length === 0 ? (
                      <li className="menu-disabled">
                        <span>{t("noAssignedTasks")}</span>
                      </li>
                    ) : null}
                  </ul>
                </div>
              </div>
            </div>

            <div className="grid gap-4 xl:grid-cols-2">
              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <h3 className="card-title text-base">{t("skills")}</h3>
                    <span className="badge badge-secondary">
                      {currentAgent?.skillIds.length ?? 0}
                    </span>
                  </div>

                  <div className="space-y-2">
                    <div className="rounded-box bg-base-200 px-3 py-2 text-sm text-base-content/60">
                      <div className="font-medium text-base-content">
                        {t("skillAllowedToolTags")}
                      </div>
                      <div className="mt-1">
                        {allowedSkillTags.join(", ") || t("notAvailable")}
                      </div>
                    </div>
                    {mappedSkills.map((skill, index) =>
                      skill ? (
                        <div
                          key={skill.id}
                          className="rounded-box bg-base-200 p-3"
                        >
                          <div className="font-medium">{skill.name}</div>
                          <div className="mt-1 text-sm text-base-content/60">
                            {skill.allowedToolTags.join(", ") || t("notAvailable")}
                          </div>
                        </div>
                      ) : (
                        <div
                          key={currentAgent?.skillIds[index] ?? index}
                          className="badge badge-primary"
                        >
                          {currentAgent?.skillIds[index]}
                        </div>
                      ),
                    )}
                    {(currentAgent?.skillIds.length ?? 0) === 0 ? (
                      <span className="text-sm text-base-content/60">{t("notAvailable")}</span>
                    ) : null}
                  </div>
                </div>
              </div>

              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <div>
                      <h3 className="card-title text-base">{t("tools")}</h3>
                      <p className="text-sm text-base-content/60">
                        {t("toolAvailability")}
                      </p>
                    </div>
                    <span className="badge badge-secondary">{availableToolCount}</span>
                  </div>

                  <div className="space-y-2">
                    {toolAvailability.map(({ tool, reason }) => (
                        <div
                          key={tool.id}
                          className="flex items-start justify-between gap-3 rounded-box bg-base-200 p-3"
                        >
                          <div>
                            <div className="flex flex-wrap items-center gap-2">
                              <div className="font-medium">{tool.name}</div>
                              <span className={`badge ${exposureBadgeClass(reason)}`}>
                                {t(`toolStatus.${reason}`)}
                              </span>
                            </div>
                            <div className="mt-1 text-sm text-base-content/60">
                              {tool.description}
                            </div>
                            <div className="mt-1 text-xs text-base-content/50">
                              {reason === "blocked_by_skill"
                                ? t("toolStatusBlockedBySkillHint", {
                                    category: tool.category,
                                  })
                                : t(`toolStatusHint.${reason}`)}
                            </div>
                          </div>
                          <div className="flex flex-col items-end gap-2">
                            <span className={`badge ${riskBadgeClass(tool.riskLevel)}`}>
                              {tool.riskLevel}
                            </span>
                            <span className="text-xs text-base-content/50">
                              {tool.category}
                            </span>
                          </div>
                        </div>
                    ))}
                    {toolAvailability.length === 0 ? (
                      <span className="text-sm text-base-content/60">{t("notAvailable")}</span>
                    ) : null}
                  </div>
                </div>
              </div>
            </div>

            <div className="grid gap-4 xl:grid-cols-[minmax(0,1.1fr)_minmax(0,0.9fr)]">
              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <h3 className="card-title text-base">{t("permissions")}</h3>
                    <span className="badge badge-secondary">{permissionRuleCount}</span>
                  </div>

                  <div className="grid gap-4 md:grid-cols-2">
                    <div>
                      <div className="mb-2 text-sm font-medium">{t("permissionAllowTools")}</div>
                      <div className="text-sm text-base-content/60">
                        {joinPolicyList(currentAgent?.permissionPolicy.allowToolIds ?? []) ||
                          t("permissionInheritTools")}
                      </div>
                    </div>
                    <div>
                      <div className="mb-2 text-sm font-medium">{t("permissionDenyTools")}</div>
                      <div className="text-sm text-base-content/60">
                        {joinPolicyList(currentAgent?.permissionPolicy.denyToolIds ?? []) ||
                          t("notAvailable")}
                      </div>
                    </div>
                    <div>
                      <div className="mb-2 text-sm font-medium">{t("permissionRequireApprovalTools")}</div>
                      <div className="text-sm text-base-content/60">
                        {joinPolicyList(currentAgent?.permissionPolicy.requireApprovalToolIds ?? []) ||
                          t("notAvailable")}
                      </div>
                    </div>
                    <div>
                      <div className="mb-2 text-sm font-medium">{t("permissionAllowFsRoots")}</div>
                      <div className="text-sm text-base-content/60">
                        {joinPolicyList(currentAgent?.permissionPolicy.allowFsRoots ?? []) ||
                          t("permissionRuntimeDefaultRoots")}
                      </div>
                    </div>
                    <div className="md:col-span-2">
                      <div className="mb-2 text-sm font-medium">{t("permissionAllowNetworkDomains")}</div>
                      <div className="text-sm text-base-content/60">
                        {joinPolicyList(currentAgent?.permissionPolicy.allowNetworkDomains ?? []) ||
                          t("permissionAllDomains")}
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <h3 className="card-title text-base">{t("memoryPolicyTitle")}</h3>
                    <span className="badge badge-secondary">{agentMemory.length}</span>
                  </div>

                  <div className="grid gap-4 md:grid-cols-3">
                    <div>
                      <div className="mb-2 text-sm font-medium">{t("readScope")}</div>
                      <div className="flex flex-wrap gap-2">
                        {(currentAgent?.memoryPolicy.readScope ?? []).map((scope) => (
                          <span key={scope} className="badge badge-neutral">
                            {scope}
                          </span>
                        ))}
                        {(currentAgent?.memoryPolicy.readScope.length ?? 0) === 0 ? (
                          <span className="text-sm text-base-content/60">
                            {t("notAvailable")}
                          </span>
                        ) : null}
                      </div>
                    </div>

                    <div>
                      <div className="mb-2 text-sm font-medium">{t("writeScope")}</div>
                      <div className="flex flex-wrap gap-2">
                        {(currentAgent?.memoryPolicy.writeScope ?? []).map((scope) => (
                          <span key={scope} className="badge badge-neutral">
                            {scope}
                          </span>
                        ))}
                        {(currentAgent?.memoryPolicy.writeScope.length ?? 0) === 0 ? (
                          <span className="text-sm text-base-content/60">
                            {t("notAvailable")}
                          </span>
                        ) : null}
                      </div>
                    </div>

                    <div>
                      <div className="mb-2 text-sm font-medium">{t("pinnedMemory")}</div>
                      <div className="flex flex-wrap gap-2">
                        {(currentAgent?.memoryPolicy.pinnedMemoryIds ?? []).map((memoryId) => (
                          <span key={memoryId} className="badge badge-neutral">
                            {memoryId}
                          </span>
                        ))}
                        {(currentAgent?.memoryPolicy.pinnedMemoryIds.length ?? 0) === 0 ? (
                          <span className="text-sm text-base-content/60">
                            {t("notAvailable")}
                          </span>
                        ) : null}
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <div className="card card-border bg-base-100">
                <div className="card-body gap-3">
                  <div className="flex items-center justify-between">
                    <h3 className="card-title text-base">{t("workGroupMembership")}</h3>
                    <span className="badge badge-secondary">{membershipGroups.length}</span>
                  </div>

                  <ul className="menu menu-sm rounded-box bg-base-100 p-2">
                    {membershipGroups.map((group) => (
                      <li key={group.id}>
                        <div className="flex items-center justify-between gap-3">
                          <span className="truncate">{group.name}</span>
                          <span className="badge badge-neutral">
                            {group.kind === "persistent" ? t("persistent") : t("ephemeral")}
                          </span>
                        </div>
                      </li>
                    ))}
                    {membershipGroups.length === 0 ? (
                      <li className="menu-disabled">
                        <span>{t("noMemberships")}</span>
                      </li>
                    ) : null}
                  </ul>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
