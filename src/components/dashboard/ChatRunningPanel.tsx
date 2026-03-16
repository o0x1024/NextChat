import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  AddWorkflowStageInput,
  AgentProfile,
  ClaimBid,
  Lease,
  OwnerBlockerResolution,
  TaskCard,
  TaskBlockerRecord,
  ToolManifest,
  ToolRun,
  UpdateWorkflowStageInput,
  WorkflowCheckpointRecord,
  WorkflowRecord,
  WorkflowStageRecord,
} from "../../types";
import { statusBadgeClass } from "./ui";
import { WorkflowCheckpointPanel } from "./WorkflowCheckpointPanel";
import { WorkflowControlPanel } from "./WorkflowControlPanel";

type BlockerAction =
  | "provide_context"
  | "reassign_task"
  | "create_dependency_task"
  | "request_approval"
  | "ask_user"
  | "pause_task";

interface BlockerDraft {
  action: BlockerAction;
  message: string;
  targetAgentId: string;
  title: string;
  goal: string;
  question: string;
  optionsText: string;
  context: string;
  allowFreeForm: boolean;
}

interface ChatRunningPanelProps {
  language: Language;
  currentWorkGroupId: string | null;
  activeTasks: TaskCard[];
  currentLeases: Lease[];
  currentApprovals: ToolRun[];
  currentGroupTasks: TaskCard[];
  taskBlockers: TaskBlockerRecord[];
  claimBids: ClaimBid[];
  agents: AgentProfile[];
  tools: ToolManifest[];
  workflowCheckpoints: WorkflowCheckpointRecord[];
  workflows: WorkflowRecord[];
  workflowStages: WorkflowStageRecord[];
  highlightedTaskId: string | null;
  highlightedBlockerId: string | null;
  targetBlockerId: string | null;
  onTaskBoardRef: (node: HTMLDivElement | null) => void;
  onApprovalsRef: (node: HTMLDivElement | null) => void;
  onSetTaskCardRef: (taskId: string, node: HTMLDivElement | null) => void;
  onSetBlockerCardRef: (blockerId: string, node: HTMLDivElement | null) => void;
  onJumpToTaskBoard: (taskId?: string) => void;
  onApproveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  onResolveBlocker: (blockerId: string, resolution: OwnerBlockerResolution) => Promise<void>;
  onCancelWorkflow: (workflowId: string) => Promise<void>;
  onPauseWorkflow: (workflowId: string) => Promise<void>;
  onResumeWorkflow: (workflowId: string) => Promise<void>;
  onSkipStage: (workflowId: string, stageId: string) => Promise<void>;
  onAddStage: (input: AddWorkflowStageInput) => Promise<void>;
  onUpdateStage: (input: UpdateWorkflowStageInput) => Promise<void>;
  onRemoveStage: (stageId: string) => Promise<void>;
}

function initialDraft(taskBlocker: TaskBlockerRecord, agents: AgentProfile[]): BlockerDraft {
  return {
    action: "provide_context",
    message: "",
    targetAgentId:
      agents.find((agent) => agent.id !== taskBlocker.raisedByAgentId)?.id ?? agents[0]?.id ?? "",
    title: "",
    goal: "",
    question: "",
    optionsText: "",
    context: "",
    allowFreeForm: true,
  };
}

function splitOptions(optionsText: string) {
  return optionsText
    .split(/\n|,/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function buildResolution(draft: BlockerDraft): OwnerBlockerResolution | null {
  switch (draft.action) {
    case "provide_context":
      return draft.message.trim()
        ? { action: "provide_context", message: draft.message.trim() }
        : null;
    case "reassign_task":
      return draft.message.trim() && draft.targetAgentId
        ? {
            action: "reassign_task",
            targetAgentId: draft.targetAgentId,
            message: draft.message.trim(),
          }
        : null;
    case "create_dependency_task":
      return draft.targetAgentId && draft.title.trim() && draft.goal.trim() && draft.message.trim()
        ? {
            action: "create_dependency_task",
            targetAgentId: draft.targetAgentId,
            title: draft.title.trim(),
            goal: draft.goal.trim(),
            message: draft.message.trim(),
          }
        : null;
    case "request_approval":
    case "ask_user": {
      const question = draft.question.trim();
      if (!question) {
        return null;
      }
      const options = splitOptions(draft.optionsText);
      return {
        action: draft.action,
        question,
        options,
        context: draft.context.trim() || null,
        allowFreeForm: draft.allowFreeForm,
      };
    }
    case "pause_task":
      return draft.message.trim()
        ? { action: "pause_task", message: draft.message.trim() }
        : null;
    default:
      return null;
  }
}

export function ChatRunningPanel({
  language,
  currentWorkGroupId,
  activeTasks,
  currentLeases,
  currentApprovals,
  currentGroupTasks,
  taskBlockers,
  claimBids,
  agents,
  tools,
  workflowCheckpoints,
  workflows,
  workflowStages,
  highlightedTaskId,
  highlightedBlockerId,
  targetBlockerId,
  onTaskBoardRef,
  onApprovalsRef,
  onSetTaskCardRef,
  onSetBlockerCardRef,
  onJumpToTaskBoard,
  onApproveRun,
  onResolveBlocker,
  onCancelWorkflow,
  onPauseWorkflow,
  onResumeWorkflow,
  onSkipStage,
  onAddStage,
  onUpdateStage,
  onRemoveStage,
}: ChatRunningPanelProps) {
  const { t } = useTranslation();
  const [expandedBlockerId, setExpandedBlockerId] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, BlockerDraft>>({});
  const [resolvingBlockerId, setResolvingBlockerId] = useState<string | null>(null);

  const ownerBlockers = useMemo(
    () =>
      taskBlockers.filter(
        (blocker) => blocker.status === "open" && blocker.resolutionTarget === "owner",
      ),
    [taskBlockers],
  );

  const blockerTasks = useMemo(
    () => new Map(currentGroupTasks.map((task) => [task.id, task])),
    [currentGroupTasks],
  );

  const currentWorkflowIds = useMemo(
    () =>
      new Set(
        workflows
          .filter((workflow) => currentWorkGroupId !== null && workflow.workGroupId === currentWorkGroupId)
          .map((workflow) => workflow.id),
      ),
    [currentWorkGroupId, workflows],
  );

  const currentGroupWorkflows = useMemo(
    () =>
      workflows.filter(
        (workflow) => currentWorkGroupId !== null && workflow.workGroupId === currentWorkGroupId,
      ),
    [currentWorkGroupId, workflows],
  );

  const currentGroupWorkflowStages = useMemo(
    () => workflowStages.filter((stage) => currentWorkflowIds.has(stage.workflowId)),
    [currentWorkflowIds, workflowStages],
  );

  const currentGroupWorkflowCheckpoints = useMemo(
    () =>
      workflowCheckpoints.filter((checkpoint) => {
        if (checkpoint.workflowId) {
          return currentWorkflowIds.has(checkpoint.workflowId);
        }
        return checkpoint.taskId ? blockerTasks.has(checkpoint.taskId) : false;
      }),
    [blockerTasks, currentWorkflowIds, workflowCheckpoints],
  );

  const blockerAgents = useMemo(
    () => new Map(agents.map((agent) => [agent.id, agent])),
    [agents],
  );

  useEffect(() => {
    if (targetBlockerId) {
      setExpandedBlockerId(targetBlockerId);
    }
  }, [targetBlockerId]);

  function ensureDraft(taskBlocker: TaskBlockerRecord) {
    return drafts[taskBlocker.id] ?? initialDraft(taskBlocker, agents);
  }

  function updateDraft(taskBlocker: TaskBlockerRecord, nextDraft: Partial<BlockerDraft>) {
    const currentDraft = ensureDraft(taskBlocker);
    setDrafts((current) => ({
      ...current,
      [taskBlocker.id]: {
        ...currentDraft,
        ...nextDraft,
      },
    }));
  }

  async function handleResolveBlocker(taskBlocker: TaskBlockerRecord) {
    const resolution = buildResolution(ensureDraft(taskBlocker));
    if (!resolution) {
      return;
    }
    setResolvingBlockerId(taskBlocker.id);
    try {
      await onResolveBlocker(taskBlocker.id, resolution);
      setExpandedBlockerId((current) => (current === taskBlocker.id ? null : current));
      setDrafts((current) => {
        const next = { ...current };
        delete next[taskBlocker.id];
        return next;
      });
    } finally {
      setResolvingBlockerId(null);
    }
  }

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
            <div className="stat px-4 py-3">
              <div className="stat-title">{t("blockers")}</div>
              <div className="stat-value text-base">{ownerBlockers.length}</div>
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

      <WorkflowControlPanel
        language={language}
        workflows={currentGroupWorkflows}
        workflowStages={currentGroupWorkflowStages}
        tasks={currentGroupTasks}
        agents={agents}
        onCancelWorkflow={onCancelWorkflow}
        onPauseWorkflow={onPauseWorkflow}
        onResumeWorkflow={onResumeWorkflow}
        onSkipStage={onSkipStage}
        onAddStage={onAddStage}
        onUpdateStage={onUpdateStage}
        onRemoveStage={onRemoveStage}
      />

      <WorkflowCheckpointPanel
        language={language}
        tasks={currentGroupTasks}
        agents={agents}
        checkpoints={currentGroupWorkflowCheckpoints}
        onOpenTask={onJumpToTaskBoard}
      />

      <section className="card card-border bg-base-100">
        <div className="card-body gap-3">
          <div className="flex items-center justify-between">
            <h3 className="card-title text-base">{t("blockers")}</h3>
            <span className="badge badge-warning">{ownerBlockers.length}</span>
          </div>
          <p className="text-sm text-base-content/60">{t("blockersHint")}</p>
          <div className="space-y-3">
            {ownerBlockers.map((taskBlocker) => {
              const task = blockerTasks.get(taskBlocker.taskId);
              const raisedBy = blockerAgents.get(taskBlocker.raisedByAgentId);
              const isExpanded = expandedBlockerId === taskBlocker.id;
              const draft = ensureDraft(taskBlocker);
              const resolution = buildResolution(draft);
              const isResolving = resolvingBlockerId === taskBlocker.id;

              return (
                <div
                  key={taskBlocker.id}
                  ref={(node) => {
                    onSetBlockerCardRef(taskBlocker.id, node);
                  }}
                  className={`rounded-box px-4 py-3 transition-colors ${
                    highlightedBlockerId === taskBlocker.id
                      ? "bg-warning/20 ring-1 ring-warning/40"
                      : "bg-warning/10"
                  }`}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 space-y-2">
                      <div className="flex flex-wrap items-center gap-2 text-xs">
                        <span className="badge badge-warning">{t("blockerOpen")}</span>
                        <span className="badge badge-ghost">{t(`taskStatus.${task?.status ?? "pending"}`)}</span>
                      </div>
                      <strong className="block text-sm">{taskBlocker.summary}</strong>
                      <p className="whitespace-pre-wrap text-xs text-base-content/70">
                        {taskBlocker.details}
                      </p>
                      <div className="flex flex-wrap gap-2 text-xs text-base-content/65">
                        <span>{t("blockerTaskLabel", { name: task?.title ?? taskBlocker.taskId.slice(0, 8) })}</span>
                        <span>{t("blockerRaisedByLabel", { name: raisedBy?.name ?? taskBlocker.raisedByAgentId })}</span>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs shrink-0"
                      onClick={() =>
                        setExpandedBlockerId((current) =>
                          current === taskBlocker.id ? null : taskBlocker.id,
                        )
                      }
                    >
                      {isExpanded ? t("close") : t("blockerHandle")}
                    </button>
                  </div>

                  {isExpanded ? (
                    <div className="mt-4 space-y-3 border-t border-warning/20 pt-3">
                      <div className="flex flex-wrap gap-2">
                        {(
                          [
                            "provide_context",
                            "reassign_task",
                            "create_dependency_task",
                            "request_approval",
                            "ask_user",
                            "pause_task",
                          ] as BlockerAction[]
                        ).map((action) => (
                          <button
                            key={action}
                            type="button"
                            className={`btn btn-xs ${
                              draft.action === action ? "btn-primary" : "btn-ghost"
                            }`}
                            onClick={() => updateDraft(taskBlocker, { action })}
                          >
                            {t(`blockerAction.${action}`)}
                          </button>
                        ))}
                      </div>

                      {(draft.action === "provide_context" || draft.action === "pause_task") && (
                        <label className="form-control gap-2">
                          <span className="label-text text-xs">
                            {draft.action === "pause_task"
                              ? t("blockerPauseMessage")
                              : t("blockerMessage")}
                          </span>
                          <textarea
                            className="textarea textarea-bordered min-h-24"
                            value={draft.message}
                            onChange={(event) =>
                              updateDraft(taskBlocker, { message: event.target.value })
                            }
                            placeholder={t("blockerMessagePlaceholder")}
                          />
                        </label>
                      )}

                      {(draft.action === "reassign_task" ||
                        draft.action === "create_dependency_task") && (
                        <label className="form-control gap-2">
                          <span className="label-text text-xs">{t("blockerTargetAgent")}</span>
                          <select
                            className="select select-bordered"
                            value={draft.targetAgentId}
                            onChange={(event) =>
                              updateDraft(taskBlocker, { targetAgentId: event.target.value })
                            }
                          >
                            {agents.map((agent) => (
                              <option key={agent.id} value={agent.id}>
                                {agent.name}
                              </option>
                            ))}
                          </select>
                        </label>
                      )}

                      {draft.action === "reassign_task" && (
                        <label className="form-control gap-2">
                          <span className="label-text text-xs">{t("blockerMessage")}</span>
                          <textarea
                            className="textarea textarea-bordered min-h-24"
                            value={draft.message}
                            onChange={(event) =>
                              updateDraft(taskBlocker, { message: event.target.value })
                            }
                            placeholder={t("blockerReassignPlaceholder")}
                          />
                        </label>
                      )}

                      {draft.action === "create_dependency_task" && (
                        <>
                          <label className="form-control gap-2">
                            <span className="label-text text-xs">
                              {t("blockerDependencyTitle")}
                            </span>
                            <input
                              className="input input-bordered"
                              value={draft.title}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { title: event.target.value })
                              }
                              placeholder={t("blockerDependencyTitlePlaceholder")}
                            />
                          </label>
                          <label className="form-control gap-2">
                            <span className="label-text text-xs">
                              {t("blockerDependencyGoal")}
                            </span>
                            <textarea
                              className="textarea textarea-bordered min-h-24"
                              value={draft.goal}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { goal: event.target.value })
                              }
                              placeholder={t("blockerDependencyGoalPlaceholder")}
                            />
                          </label>
                          <label className="form-control gap-2">
                            <span className="label-text text-xs">{t("blockerMessage")}</span>
                            <textarea
                              className="textarea textarea-bordered min-h-24"
                              value={draft.message}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { message: event.target.value })
                              }
                              placeholder={t("blockerDependencyMessagePlaceholder")}
                            />
                          </label>
                        </>
                      )}

                      {(draft.action === "request_approval" || draft.action === "ask_user") && (
                        <>
                          <label className="form-control gap-2">
                            <span className="label-text text-xs">{t("blockerQuestion")}</span>
                            <textarea
                              className="textarea textarea-bordered min-h-24"
                              value={draft.question}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { question: event.target.value })
                              }
                              placeholder={t("blockerQuestionPlaceholder")}
                            />
                          </label>
                          <label className="form-control gap-2">
                            <span className="label-text text-xs">{t("blockerOptions")}</span>
                            <textarea
                              className="textarea textarea-bordered min-h-24"
                              value={draft.optionsText}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { optionsText: event.target.value })
                              }
                              placeholder={t("blockerOptionsPlaceholder")}
                            />
                          </label>
                          <label className="form-control gap-2">
                            <span className="label-text text-xs">{t("blockerContext")}</span>
                            <textarea
                              className="textarea textarea-bordered min-h-24"
                              value={draft.context}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { context: event.target.value })
                              }
                              placeholder={t("blockerContextPlaceholder")}
                            />
                          </label>
                          <label className="label cursor-pointer justify-start gap-3">
                            <input
                              type="checkbox"
                              className="checkbox checkbox-sm"
                              checked={draft.allowFreeForm}
                              onChange={(event) =>
                                updateDraft(taskBlocker, { allowFreeForm: event.target.checked })
                              }
                            />
                            <span className="label-text text-xs">{t("blockerAllowFreeForm")}</span>
                          </label>
                        </>
                      )}

                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          className="btn btn-primary btn-sm"
                          disabled={!resolution || isResolving}
                          onClick={() => {
                            void handleResolveBlocker(taskBlocker);
                          }}
                        >
                          {isResolving ? t("resolvingBlocker") : t("submitResolution")}
                        </button>
                        {task ? (
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            onClick={() => onJumpToTaskBoard(task.id)}
                          >
                            {t("openTask")}
                          </button>
                        ) : null}
                      </div>
                    </div>
                  ) : null}
                </div>
              );
            })}

            {ownerBlockers.length === 0 && (
              <div className="alert alert-soft">
                <span className="text-sm">{t("noOpenBlockers")}</span>
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
