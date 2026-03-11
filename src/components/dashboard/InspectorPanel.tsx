import type { ChangeEvent, FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  AgentProfile,
  AuditEvent,
  CreateAgentInput,
  SkillPack,
  TaskCard,
  ToolManifest,
  WorkGroup,
} from "../../types";
import { formatTime } from "./ui";

interface InspectorPanelProps {
  currentTask?: TaskCard | null;
  currentAgent?: AgentProfile | null;
  currentWorkGroup?: WorkGroup;
  auditEvents: AuditEvent[];
  language: Language;
  agentForm: CreateAgentInput;
  agents: AgentProfile[];
  skills: SkillPack[];
  tools: ToolManifest[];
  onAgentSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onAgentFormChange: (updater: (state: CreateAgentInput) => CreateAgentInput) => void;
  onCancelTask: (taskCardId: string) => void;
  onResumeTask: (taskCardId: string) => void;
  onToggleAgentMembership: (group: WorkGroup, agent: AgentProfile) => void;
}

export function InspectorPanel({
  currentTask,
  currentAgent,
  currentWorkGroup,
  auditEvents,
  language,
  agentForm,
  agents,
  skills,
  tools,
  onAgentSubmit,
  onAgentFormChange,
  onCancelTask,
  onResumeTask,
  onToggleAgentMembership,
}: InspectorPanelProps) {
  const { t } = useTranslation();

  function updateAgentForm<K extends keyof CreateAgentInput>(
    key: K,
    value: CreateAgentInput[K],
  ) {
    onAgentFormChange((state) => ({ ...state, [key]: value }));
  }

  return (
    <section className="card card-border bg-base-100">
      <div className="card-body gap-4">
        <div className="flex items-center justify-between">
          <h2 className="card-title">{t("inspector")}</h2>
          <span className="badge badge-secondary">
            {currentTask ? t("taskFocus") : currentAgent ? t("agentFocus") : t("create")}
          </span>
        </div>

        {currentTask ? (
          <div className="space-y-4">
            <div className="card card-border bg-base-100">
              <div className="card-body">
                <h3 className="card-title text-lg">{currentTask.title}</h3>
                <p className="whitespace-pre-wrap text-sm opacity-70">{currentTask.inputPayload}</p>
              </div>
            </div>

            <div className="stats stats-vertical bg-base-200 sm:stats-horizontal">
              <div className="stat">
                <div className="stat-title">{t("status")}</div>
                <div className="stat-value text-base">{t(`taskStatus.${currentTask.status}`)}</div>
              </div>
              <div className="stat">
                <div className="stat-title">{t("created")}</div>
                <div className="stat-value text-base">
                  {formatTime(currentTask.createdAt, language)}
                </div>
              </div>
            </div>

            <div className="join">
              <button className="btn btn-error btn-sm join-item" onClick={() => onCancelTask(currentTask.id)}>
                {t("cancel")}
              </button>
              <button className="btn btn-secondary btn-sm join-item" onClick={() => onResumeTask(currentTask.id)}>
                {t("resume")}
              </button>
            </div>

            <div className="collapse collapse-arrow bg-base-100">
              <input type="checkbox" defaultChecked />
              <div className="collapse-title min-h-0 px-4 py-3 text-sm font-medium">{t("auditTrail")}</div>
              <div className="collapse-content space-y-2">
                {auditEvents
                  .filter((event) => event.entityId === currentTask.id)
                  .slice(0, 6)
                  .map((event) => (
                    <div
                      key={event.id}
                      className="rounded-box bg-base-200 px-4 py-3 text-sm"
                    >
                      <strong>{event.eventType}</strong>
                      <span className="opacity-55">{formatTime(event.createdAt, language)}</span>
                    </div>
                  ))}
                {auditEvents.filter((event) => event.entityId === currentTask.id).length === 0 ? (
                  <p className="text-sm opacity-60">{t("noAuditEvents")}</p>
                ) : null}
              </div>
            </div>
          </div>
        ) : (
          <form className="flex flex-col gap-3" onSubmit={onAgentSubmit}>
            <fieldset className="fieldset rounded-box bg-base-100 p-4">
              <legend className="fieldset-legend">{t("agentName")}</legend>
              <input
                className="input input-bordered input-sm w-full"
                placeholder={t("agentName")}
                value={agentForm.name}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  updateAgentForm("name", event.target.value)
                }
              />
              <div className="mt-3 grid gap-3 md:grid-cols-2">
                <input
                  className="input input-bordered input-sm"
                  placeholder={t("avatar")}
                  value={agentForm.avatar}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    updateAgentForm("avatar", event.target.value)
                  }
                />
                <input
                  className="input input-bordered input-sm"
                  placeholder={t("role")}
                  value={agentForm.role}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    updateAgentForm("role", event.target.value)
                  }
                />
              </div>
              <textarea
                className="textarea textarea-bordered textarea-sm mt-3 min-h-24 w-full"
                placeholder={t("objective")}
                value={agentForm.objective}
                onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                  updateAgentForm("objective", event.target.value)
                }
              />
              <div className="mt-3 grid gap-3 md:grid-cols-2">
                <select
                  className="select select-bordered select-sm"
                  value={agentForm.provider}
                  onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                    updateAgentForm("provider", event.target.value)
                  }
                >
                  <option value="mock">mock</option>
                  <option value="openai">openai</option>
                </select>
                <input
                  className="input input-bordered input-sm"
                  placeholder={t("model")}
                  value={agentForm.model}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    updateAgentForm("model", event.target.value)
                  }
                />
              </div>
            </fieldset>

            <fieldset className="fieldset rounded-box bg-base-100 p-4">
              <legend className="fieldset-legend">{t("tools")}</legend>
              <div className="flex flex-wrap gap-2">
                {tools.map((tool) => {
                  const active = agentForm.toolIds.includes(tool.id);
                  return (
                    <button
                      type="button"
                      key={tool.id}
                      className={`btn btn-sm rounded-full ${active ? "btn-secondary" : "btn-soft"}`}
                      onClick={() =>
                        onAgentFormChange((state) => ({
                          ...state,
                          toolIds: active
                            ? state.toolIds.filter((id) => id !== tool.id)
                            : [...state.toolIds, tool.id],
                        }))
                      }
                    >
                      {tool.name}
                    </button>
                  );
                })}
              </div>
            </fieldset>

            {currentWorkGroup ? (
              <div className="collapse collapse-arrow bg-base-100">
                <input type="checkbox" defaultChecked />
                <div className="collapse-title min-h-0 px-4 py-3 text-sm font-medium">
                  {t("addToCurrentWorkGroup")}
                </div>
                <div className="collapse-content flex flex-col gap-2">
                  {agents.map((agent) => (
                    <label
                      className="label cursor-pointer justify-start gap-3 rounded-box bg-base-200 px-4 py-3"
                      key={agent.id}
                    >
                      <input
                        className="checkbox checkbox-primary checkbox-sm"
                        type="checkbox"
                        checked={currentWorkGroup.memberAgentIds.includes(agent.id)}
                        onChange={() => onToggleAgentMembership(currentWorkGroup, agent)}
                      />
                      <span className="label-text">{agent.name}</span>
                    </label>
                  ))}
                </div>
              </div>
            ) : null}

            <div className="join join-vertical w-full md:join-horizontal">
              <label className="label join-item cursor-pointer justify-start gap-3 rounded-box bg-base-200 px-4 py-3">
                <input
                  className="checkbox checkbox-primary checkbox-sm"
                  type="checkbox"
                  checked={agentForm.canSpawnSubtasks}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    updateAgentForm("canSpawnSubtasks", event.target.checked)
                  }
                />
                <span className="label-text">{t("canSpawnSubtasks")}</span>
              </label>
              <input
                className="input input-bordered input-sm join-item md:w-32"
                type="number"
                min={1}
                max={8}
                value={agentForm.maxParallelRuns}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  updateAgentForm("maxParallelRuns", Number(event.target.value))
                }
              />
            </div>

            <button className="btn btn-primary btn-sm btn-block" type="submit">
              {currentAgent ? t("updateAgent") : t("createAgent")}
            </button>
          </form>
        )}
      </div>
    </section>
  );
}
