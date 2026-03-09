import { type ChangeEvent, type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  AgentProfile,
  ClaimBid,
  ConversationMessage,
  CreateWorkGroupInput,
  Lease,
  SystemSettings,
  TaskCard,
  ToolManifest,
  ToolRun,
  WorkGroup,
} from "../../types";
import { formatTime, roleAccent, statusBadgeClass } from "./ui";

interface ChatManagementProps {
  workGroups: WorkGroup[];
  agents: AgentProfile[];
  messages: ConversationMessage[];
  taskCards: TaskCard[];
  leases: Lease[];
  claimBids: ClaimBid[];
  toolRuns: ToolRun[];
  tools: ToolManifest[];
  settings: SystemSettings;
  selectedWorkGroupId?: string;
  language: Language;
  backstageOpen: boolean;
  onSelectWorkGroup: (id: string) => void;
  onCreateGroup: (input: CreateWorkGroupInput) => Promise<void>;
  onSendMessage: (workGroupId: string, content: string) => Promise<void>;
  onAddAgent: (workGroupId: string, agentId: string) => Promise<void>;
  onRemoveAgent: (workGroupId: string, agentId: string) => Promise<void>;
  onApproveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  onToggleBackstage: () => void;
}

function senderBubbleClass(message: ConversationMessage) {
  if (message.senderKind === "human") return "chat-bubble-primary";
  if (message.senderKind === "agent") return "chat-bubble-secondary";
  return "chat-bubble-neutral";
}

function executionBadgeClass(message: ConversationMessage) {
  if (message.executionMode === "real_model") return "badge-success";
  if (message.executionMode === "fallback") return "badge-warning";
  return "badge-ghost";
}

const emptyGroupForm: CreateWorkGroupInput = {
  name: "",
  goal: "",
  kind: "persistent",
  defaultVisibility: "summary",
  autoArchive: false,
};

export function ChatManagement({
  workGroups,
  agents,
  messages,
  taskCards,
  leases,
  claimBids,
  toolRuns,
  tools,
  settings,
  selectedWorkGroupId,
  language,
  backstageOpen,
  onSelectWorkGroup,
  onCreateGroup,
  onSendMessage,
  onAddAgent,
  onRemoveAgent,
  onApproveRun,
  onToggleBackstage,
}: ChatManagementProps) {
  const { t } = useTranslation();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [runningPanelOpen, setRunningPanelOpen] = useState(false);
  const [composerValue, setComposerValue] = useState("");
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [memberModalOpen, setMemberModalOpen] = useState(false);
  const [groupForm, setGroupForm] = useState<CreateWorkGroupInput>(emptyGroupForm);

  const currentGroup = useMemo(
    () => workGroups.find((group) => group.id === selectedWorkGroupId) ?? workGroups[0],
    [selectedWorkGroupId, workGroups],
  );

  const currentMessages = useMemo(() => {
    if (!currentGroup) return [];
    const allMessages = messages.filter((message) => message.workGroupId === currentGroup.id);
    return backstageOpen
      ? allMessages
      : allMessages.filter((message) => message.visibility === "main");
  }, [backstageOpen, currentGroup, messages]);

  const currentMembers = useMemo(() => {
    if (!currentGroup) return [];
    const memberIds = new Set(currentGroup.memberAgentIds);
    return agents.filter((agent) => memberIds.has(agent.id));
  }, [agents, currentGroup]);

  const currentGroupTasks = useMemo(() => {
    if (!currentGroup) return [];
    return taskCards
      .filter((task) => task.workGroupId === currentGroup.id)
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
  }, [currentGroup, taskCards]);

  const activeTasks = useMemo(
    () =>
      currentGroupTasks.filter(
        (task) => !["completed", "cancelled"].includes(task.status),
      ),
    [currentGroupTasks],
  );

  const currentTaskIds = useMemo(
    () => new Set(currentGroupTasks.map((task) => task.id)),
    [currentGroupTasks],
  );

  const currentLeases = useMemo(
    () =>
      leases.filter(
        (lease) => currentTaskIds.has(lease.taskCardId) && lease.state !== "released",
      ),
    [currentTaskIds, leases],
  );

  const currentApprovals = useMemo(
    () =>
      toolRuns.filter(
        (run) =>
          currentTaskIds.has(run.taskCardId) &&
          run.approvalRequired &&
          run.state === "pending_approval",
      ),
    [currentTaskIds, toolRuns],
  );

  async function handleSend(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!currentGroup || !composerValue.trim()) return;
    await onSendMessage(currentGroup.id, composerValue.trim());
    setComposerValue("");
  }

  async function handleCreateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onCreateGroup(groupForm);
    setGroupForm(emptyGroupForm);
    setCreateModalOpen(false);
  }

  async function handleToggleMember(agent: AgentProfile) {
    if (!currentGroup) return;
    if (currentGroup.memberAgentIds.includes(agent.id)) {
      await onRemoveAgent(currentGroup.id, agent.id);
      return;
    }
    await onAddAgent(currentGroup.id, agent.id);
  }

  return (
    <div className="flex h-full">
      {sidebarOpen && (
        <div className="flex w-64 shrink-0 flex-col border-r border-base-content/10 transition-all duration-300">
          <div className="flex items-center justify-between border-b border-base-content/10 px-4 py-3">
            <h2 className="text-sm font-bold">{t("chatManagement")}</h2>
            <div className="flex items-center gap-1">
              <button className="btn btn-primary btn-xs" onClick={() => setCreateModalOpen(true)}>
                + {t("create")}
              </button>
              <button className="btn btn-ghost btn-xs" onClick={() => setSidebarOpen(false)}>
                <i className="fas fa-indent" />
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto">
            <ul className="menu menu-sm gap-0.5 p-2">
              {workGroups.map((group) => (
                <li key={group.id}>
                  <a
                    className={`flex items-center gap-2 rounded-lg py-2.5 ${currentGroup?.id === group.id ? "menu-active" : ""
                      }`}
                    onClick={() => onSelectWorkGroup(group.id)}
                  >
                    <div className="grid h-8 w-8 place-items-center rounded-lg border border-primary/20 bg-primary/10 text-[10px] font-bold text-primary">
                      {group.name.slice(0, 2).toUpperCase()}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">{group.name}</div>
                      <div className="truncate text-xs text-base-content/50">{group.goal}</div>
                    </div>
                    <span className="badge badge-ghost badge-xs">
                      {group.kind === "persistent" ? "P" : "E"}
                    </span>
                  </a>
                </li>
              ))}
              {workGroups.length === 0 && (
                <li className="py-4 text-center text-sm text-base-content/50">
                  {t("noWorkGroupsYet")}
                </li>
              )}
            </ul>
          </div>
        </div>
      )}

      <div className="flex min-w-0 flex-1 flex-col">
        {currentGroup ? (
          <>
            <div className="flex items-center justify-between border-b border-base-content/10 px-5 py-3">
              <div className="flex min-w-0 items-center gap-3">
                {!sidebarOpen && (
                  <button className="btn btn-ghost btn-sm -ml-2" onClick={() => setSidebarOpen(true)}>
                    <i className="fas fa-outdent" />
                  </button>
                )}
                <h3 className="truncate text-base font-bold">{currentGroup.name}</h3>
                <span className="badge badge-ghost badge-sm shrink-0">
                  {currentGroup.kind === "persistent" ? t("persistent") : t("ephemeral")}
                </span>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  className={`btn btn-sm ${backstageOpen ? "btn-primary" : "btn-ghost"}`}
                  onClick={onToggleBackstage}
                >
                  {backstageOpen ? t("mainFeed") : t("backstage")}
                </button>
                <button
                  className={`btn btn-sm ${runningPanelOpen ? "btn-primary" : "btn-ghost"}`}
                  onClick={() => setRunningPanelOpen(!runningPanelOpen)}
                >
                  {t("runningPanel")}
                </button>
                <button className="btn btn-ghost btn-sm" onClick={() => setMemberModalOpen(true)}>
                  {t("members")} ({currentMembers.length})
                </button>
              </div>
            </div>

            {currentMembers.length > 0 && (
              <div className="flex items-center gap-2 overflow-x-auto border-b border-base-content/5 px-5 py-2">
                {currentMembers.map((agent) => {
                  const accent = roleAccent(agent.role);
                  return (
                    <div
                      key={agent.id}
                      className="badge badge-ghost shrink-0 gap-1.5 py-3"
                      style={{ borderColor: `${accent}40`, color: accent }}
                    >
                      <span className="text-[10px] font-bold">{agent.avatar}</span>
                      {agent.name}
                    </div>
                  );
                })}
              </div>
            )}

            <div className={`grid min-h-0 flex-1 gap-4 px-5 py-4 ${runningPanelOpen ? "xl:grid-cols-[minmax(0,1fr)_340px]" : "grid-cols-1"}`}>
              <div className="flex min-h-0 flex-col">
                <div className="flex-1 space-y-1 overflow-y-auto">
                  {currentMessages.map((message) => (
                    <div
                      key={message.id}
                      className={`chat ${message.senderKind === "human" ? "chat-end" : "chat-start"}`}
                    >
                      <div className="chat-header mb-1 text-xs text-base-content/50">
                        <span className="font-medium">{message.senderName}</span>
                        {message.visibility === "backstage" && (
                          <span className="ml-1 badge badge-warning badge-xs">{t("backstage")}</span>
                        )}
                        {message.executionMode && (
                          <span className={`ml-1 badge badge-xs ${executionBadgeClass(message)}`}>
                            {message.executionMode === "real_model"
                              ? t("realModelReady")
                              : t("fallbackExecution")}
                          </span>
                        )}
                        <time className="ml-2">{formatTime(message.createdAt, language)}</time>
                      </div>
                      <div className={`chat-bubble whitespace-pre-wrap text-sm ${senderBubbleClass(message)}`}>
                        {message.content}
                      </div>
                    </div>
                  ))}

                  {currentMessages.length === 0 && (
                    <div className="hero min-h-40 rounded-box bg-base-200">
                      <div className="hero-content text-center">
                        <div className="max-w-xs">
                          <h3 className="text-base font-semibold">{t("noMessagesYet")}</h3>
                          <p className="mt-1 text-sm text-base-content/60">{t("noMessagesHint")}</p>
                        </div>
                      </div>
                    </div>
                  )}
                </div>

                <div className="mt-auto px-5 pb-6">
                  <form
                    className="flex flex-col bg-base-100 rounded-2xl border border-primary/20 shadow-sm focus-within:ring-2 focus-within:ring-primary/10 transition-all p-2 pt-3"
                    onSubmit={(event) => {
                      if (composerValue.trim()) {
                        void handleSend(event);
                      } else {
                        event.preventDefault();
                      }
                    }}
                  >
                    <textarea
                      className="textarea textarea-ghost w-full min-h-[60px] resize-none focus:outline-none bg-transparent placeholder:opacity-30 text-sm leading-relaxed"
                      placeholder={t("taskPlaceholder")}
                      rows={2}
                      value={composerValue}
                      onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                        setComposerValue(event.target.value)
                      }
                      onKeyDown={(event) => {
                        if (event.key === "Enter" && !event.shiftKey) {
                          event.preventDefault();
                          if (composerValue.trim() && currentGroup) {
                            void onSendMessage(currentGroup.id, composerValue.trim());
                            setComposerValue("");
                          }
                        }
                      }}
                    />

                    <div className="flex items-center justify-between px-2 pb-1 mt-2">
                      {/* Left Icons Group */}
                      <div className="flex items-center gap-3.5 text-base-content/40">
                        <button type="button" className="hover:text-primary transition-colors" title="Attach"><i className="fas fa-paperclip text-xs" /></button>
                        <div className="bg-primary/10 text-primary p-1 rounded flex items-center justify-center"><i className="fas fa-tools text-[10px]" /></div>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-cog text-xs" /></button>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-users text-xs" /></button>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-brain text-xs" /></button>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-at text-xs" /></button>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-bolt text-xs" /></button>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-table text-xs" /></button>
                        <button type="button" className="hover:text-primary transition-colors"><i className="fas fa-database text-xs" /></button>
                      </div>

                      {/* Right Group: Model & Send */}
                      <div className="flex items-center gap-4">
                        <div className="flex items-center gap-1.5 text-[10px] font-bold text-base-content/40 cursor-pointer hover:text-primary transition-all">
                          {settings.globalConfig.defaultLLMModel} <i className="fas fa-chevron-down text-[8px]" />
                        </div>
                        <div className="text-base-content/30"><i className="fas fa-language text-xs" /></div>
                        <button
                          type="submit"
                          className={`btn btn-circle btn-xs w-7 h-7 min-h-0 border-none transition-all ${composerValue.trim()
                            ? "bg-primary text-primary-content hover:scale-110 shadow-lg shadow-primary/20"
                            : "bg-base-200 text-base-content/20"
                            }`}
                          disabled={!composerValue.trim()}
                        >
                          <i className="fas fa-arrow-up text-[10px]" />
                        </button>
                      </div>
                    </div>
                  </form>
                </div>
              </div>

              {runningPanelOpen && (
                <aside className="flex min-h-0 flex-col gap-3 overflow-y-auto w-[340px]">
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
                    <div className="card-body gap-3">
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
                            <div key={task.id} className="rounded-box bg-base-200 px-4 py-3">
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
                    <div className="card-body gap-3">
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
                                <strong className="line-clamp-1 text-sm">
                                  {tool?.name ?? run.toolId}
                                </strong>
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
                </aside>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center">
            <div className="text-center text-base-content/50">
              <div className="mb-3 text-4xl">💬</div>
              <div>{t("noWorkGroupSelected")}</div>
            </div>
          </div>
        )}
      </div>

      {createModalOpen && (
        <dialog className="modal modal-open" onClick={() => setCreateModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => setCreateModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">{t("createWorkGroup")}</h3>
            <form className="space-y-3" onSubmit={(event) => void handleCreateGroup(event)}>
              <input
                className="input input-bordered input-sm w-full"
                placeholder={t("name")}
                required
                value={groupForm.name}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  setGroupForm((form) => ({ ...form, name: event.target.value }))
                }
              />
              <textarea
                className="textarea textarea-bordered textarea-sm w-full"
                placeholder={t("sharedGoal")}
                rows={2}
                value={groupForm.goal}
                onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                  setGroupForm((form) => ({ ...form, goal: event.target.value }))
                }
              />
              <select
                className="select select-bordered select-sm w-full"
                value={groupForm.kind}
                onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                  setGroupForm((form) => ({
                    ...form,
                    kind: event.target.value as CreateWorkGroupInput["kind"],
                  }))
                }
              >
                <option value="persistent">{t("persistent")}</option>
                <option value="ephemeral">{t("ephemeral")}</option>
              </select>
              <div className="modal-action">
                <button className="btn btn-ghost btn-sm" type="button" onClick={() => setCreateModalOpen(false)}>
                  {t("cancel")}
                </button>
                <button className="btn btn-primary btn-sm" type="submit">
                  {t("createGroup")}
                </button>
              </div>
            </form>
          </div>
        </dialog>
      )}

      {memberModalOpen && currentGroup && (
        <dialog className="modal modal-open" onClick={() => setMemberModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => setMemberModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">
              {t("members")} - {currentGroup.name}
            </h3>
            <div className="space-y-2">
              {agents.map((agent) => {
                const isMember = currentGroup.memberAgentIds.includes(agent.id);
                return (
                  <div
                    key={agent.id}
                    className="flex items-center justify-between rounded-box bg-base-200 px-4 py-2.5"
                  >
                    <div className="flex items-center gap-2">
                      <div className="grid h-8 w-8 place-items-center rounded-btn bg-primary/10 text-xs font-bold text-primary">
                        {agent.avatar}
                      </div>
                      <div>
                        <div className="text-sm font-medium">{agent.name}</div>
                        <div className="text-xs text-base-content/50">{agent.role}</div>
                      </div>
                    </div>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={isMember}
                      onChange={() => void handleToggleMember(agent)}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        </dialog>
      )}
    </div>
  );
}
