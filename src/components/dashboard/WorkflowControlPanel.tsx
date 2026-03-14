import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  AddWorkflowStageInput,
  AgentProfile,
  TaskCard,
  UpdateWorkflowStageInput,
  WorkflowExecutionMode,
  WorkflowRecord,
  WorkflowStageRecord,
} from "../../types";

interface WorkflowControlPanelProps {
  language: Language;
  workflows: WorkflowRecord[];
  workflowStages: WorkflowStageRecord[];
  tasks: TaskCard[];
  agents: AgentProfile[];
  onCancelWorkflow: (workflowId: string) => Promise<void>;
  onPauseWorkflow: (workflowId: string) => Promise<void>;
  onResumeWorkflow: (workflowId: string) => Promise<void>;
  onSkipStage: (workflowId: string, stageId: string) => Promise<void>;
  onAddStage: (input: AddWorkflowStageInput) => Promise<void>;
  onUpdateStage: (input: UpdateWorkflowStageInput) => Promise<void>;
  onRemoveStage: (stageId: string) => Promise<void>;
}

type ModalMode =
  | { kind: "add"; workflowId: string; afterStageId?: string }
  | { kind: "edit"; stage: WorkflowStageRecord }
  | null;

const executionModes: WorkflowExecutionMode[] = ["serial", "parallel"];

const statusBadge: Record<string, string> = {
  planning: "badge-info",
  running: "badge-primary",
  blocked: "badge-error",
  needs_user_input: "badge-warning",
  completed: "badge-success",
  needs_review: "badge-warning",
  cancelled: "badge-neutral",
  pending: "badge-ghost",
  ready: "badge-info",
};

function stageIcon(status: string) {
  switch (status) {
    case "completed":
      return "✅";
    case "running":
      return "🔄";
    case "blocked":
      return "🚫";
    case "cancelled":
      return "⏹";
    case "needs_review":
      return "👀";
    default:
      return "⏳";
  }
}

export function WorkflowControlPanel({
  language: _language,
  workflows,
  workflowStages,
  tasks: _tasks,
  agents,
  onCancelWorkflow,
  onPauseWorkflow,
  onResumeWorkflow,
  onSkipStage,
  onAddStage,
  onUpdateStage,
  onRemoveStage,
}: WorkflowControlPanelProps) {
  const { t } = useTranslation();
  const [expandedWorkflowId, setExpandedWorkflowId] = useState<string | null>(null);
  const [loadingAction, setLoadingAction] = useState<string | null>(null);
  const [modal, setModal] = useState<ModalMode>(null);
  const [formTitle, setFormTitle] = useState("");
  const [formGoal, setFormGoal] = useState("");
  const [formMode, setFormMode] = useState<WorkflowExecutionMode>("serial");

  const activeWorkflows = useMemo(
    () => workflows.filter((w) => w.status !== "completed" && w.status !== "cancelled"),
    [workflows],
  );

  const stagesByWorkflow = useMemo(() => {
    const map = new Map<string, WorkflowStageRecord[]>();
    for (const stage of workflowStages) {
      const list = map.get(stage.workflowId) ?? [];
      list.push(stage);
      map.set(stage.workflowId, list);
    }
    for (const [, list] of map) {
      list.sort((a, b) => a.orderIndex - b.orderIndex);
    }
    return map;
  }, [workflowStages]);

  const agentMap = useMemo(() => {
    const map = new Map<string, AgentProfile>();
    for (const a of agents) map.set(a.id, a);
    return map;
  }, [agents]);

  if (activeWorkflows.length === 0) return null;

  async function wrapAction(key: string, fn: () => Promise<void>) {
    setLoadingAction(key);
    try {
      await fn();
    } finally {
      setLoadingAction(null);
    }
  }

  function openAddModal(workflowId: string, afterStageId?: string) {
    setFormTitle("");
    setFormGoal("");
    setFormMode("serial");
    setModal({ kind: "add", workflowId, afterStageId });
  }

  function openEditModal(stage: WorkflowStageRecord) {
    setFormTitle(stage.title);
    setFormGoal(stage.goal);
    setFormMode(stage.executionMode);
    setModal({ kind: "edit", stage });
  }

  async function submitModal() {
    if (!modal) return;
    if (modal.kind === "add") {
      await wrapAction("modal-submit", () =>
        onAddStage({
          workflowId: modal.workflowId,
          title: formTitle.trim(),
          goal: formGoal.trim(),
          executionMode: formMode,
          afterStageId: modal.afterStageId ?? null,
        }),
      );
    } else {
      await wrapAction("modal-submit", () =>
        onUpdateStage({
          stageId: modal.stage.id,
          title: formTitle.trim() || null,
          goal: formGoal.trim() || null,
          executionMode: formMode,
        }),
      );
    }
    setModal(null);
  }

  return (
    <>
      {/* Stage add/edit modal */}
      {modal && (
        <dialog className="modal modal-open">
          <div className="modal-box">
            <h3 className="mb-4 text-lg font-bold">
              {modal.kind === "add"
                ? t("addStage", "Add Stage")
                : t("editStage", "Edit Stage")}
            </h3>
            <div className="form-control gap-3">
              <label className="label">
                <span className="label-text">{t("stageTitle", "Title")}</span>
              </label>
              <input
                className="input input-bordered w-full"
                value={formTitle}
                onChange={(e) => setFormTitle(e.target.value)}
                placeholder={t("stageTitlePlaceholder", "Stage title")}
              />

              <label className="label">
                <span className="label-text">{t("stageGoal", "Goal")}</span>
              </label>
              <textarea
                className="textarea textarea-bordered w-full"
                rows={3}
                value={formGoal}
                onChange={(e) => setFormGoal(e.target.value)}
                placeholder={t("stageGoalPlaceholder", "Describe stage goal")}
              />

              <label className="label">
                <span className="label-text">{t("executionMode", "Execution Mode")}</span>
              </label>
              <select
                className="select select-bordered w-full"
                value={formMode}
                onChange={(e) => setFormMode(e.target.value as WorkflowExecutionMode)}
              >
                {executionModes.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>

            <div className="modal-action">
              <button
                type="button"
                className="btn"
                onClick={() => setModal(null)}
              >
                {t("cancel", "Cancel")}
              </button>
              <button
                type="button"
                className="btn btn-primary"
                disabled={!formTitle.trim() || !formGoal.trim() || loadingAction !== null}
                onClick={submitModal}
              >
                {loadingAction === "modal-submit" ? (
                  <span className="loading loading-spinner loading-xs" />
                ) : modal.kind === "add" ? (
                  t("add", "Add")
                ) : (
                  t("save", "Save")
                )}
              </button>
            </div>
          </div>
          <div className="modal-backdrop" onClick={() => setModal(null)} />
        </dialog>
      )}

      <section className="card card-border bg-base-100">
        <div className="card-body gap-3">
          <div className="flex items-center justify-between">
            <h3 className="card-title text-base">{t("workflowControl", "Workflow Control")}</h3>
            <span className="badge badge-primary">{activeWorkflows.length}</span>
          </div>

          <div className="space-y-3">
            {activeWorkflows.map((wf) => {
              const stages = stagesByWorkflow.get(wf.id) ?? [];
              const owner = agentMap.get(wf.ownerAgentId);
              const expanded = expandedWorkflowId === wf.id;
              const canPause = wf.status === "running";
              const canResume =
                wf.status === "needs_user_input" || wf.status === "blocked";
              const canCancel =
                wf.status !== "completed" && wf.status !== "cancelled";
              const canModifyStages = canCancel;

              return (
                <div
                  key={wf.id}
                  className="rounded-box border border-base-300 bg-base-200/50 p-3"
                >
                  {/* Header */}
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs btn-circle"
                      onClick={() =>
                        setExpandedWorkflowId(expanded ? null : wf.id)
                      }
                      aria-label={expanded ? "Collapse" : "Expand"}
                    >
                      <span
                        className={`transition-transform ${expanded ? "rotate-90" : ""}`}
                      >
                        ▶
                      </span>
                    </button>

                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium">
                          {wf.title || wf.normalizedIntent}
                        </span>
                        <span
                          className={`badge badge-xs ${statusBadge[wf.status] ?? "badge-ghost"}`}
                        >
                          {wf.status}
                        </span>
                      </div>
                      {owner && (
                        <span className="text-xs text-base-content/50">
                          Owner: {owner.name}
                        </span>
                      )}
                    </div>

                    {/* Workflow-level action buttons */}
                    <div className="flex items-center gap-1">
                      {canPause && (
                        <button
                          type="button"
                          className="btn btn-warning btn-xs"
                          disabled={loadingAction !== null}
                          onClick={() =>
                            wrapAction(`pause-${wf.id}`, () =>
                              onPauseWorkflow(wf.id),
                            )
                          }
                          title={t("pauseWorkflow", "Pause")}
                        >
                          {loadingAction === `pause-${wf.id}` ? (
                            <span className="loading loading-spinner loading-xs" />
                          ) : (
                            "⏸"
                          )}
                        </button>
                      )}
                      {canResume && (
                        <button
                          type="button"
                          className="btn btn-success btn-xs"
                          disabled={loadingAction !== null}
                          onClick={() =>
                            wrapAction(`resume-${wf.id}`, () =>
                              onResumeWorkflow(wf.id),
                            )
                          }
                          title={t("resumeWorkflow", "Resume")}
                        >
                          {loadingAction === `resume-${wf.id}` ? (
                            <span className="loading loading-spinner loading-xs" />
                          ) : (
                            "▶️"
                          )}
                        </button>
                      )}
                      {canModifyStages && (
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs"
                          disabled={loadingAction !== null}
                          onClick={() => openAddModal(wf.id)}
                          title={t("appendStage", "Append new stage")}
                        >
                          ＋
                        </button>
                      )}
                      {canCancel && (
                        <button
                          type="button"
                          className="btn btn-error btn-xs"
                          disabled={loadingAction !== null}
                          onClick={() =>
                            wrapAction(`cancel-${wf.id}`, () =>
                              onCancelWorkflow(wf.id),
                            )
                          }
                          title={t("cancelWorkflow", "Cancel")}
                        >
                          {loadingAction === `cancel-${wf.id}` ? (
                            <span className="loading loading-spinner loading-xs" />
                          ) : (
                            "⏹"
                          )}
                        </button>
                      )}
                    </div>
                  </div>

                  {/* Stage pipeline (expanded) */}
                  {expanded && stages.length > 0 && (
                    <div className="mt-3 space-y-2 pl-6">
                      <div className="text-xs font-semibold uppercase tracking-wide text-base-content/50">
                        {t("stages", "Stages")} ({stages.length})
                      </div>
                      {stages.map((stage) => {
                        const isCurrent = wf.currentStageId === stage.id;
                        const canSkip =
                          (stage.status === "running" ||
                            stage.status === "blocked") &&
                          canCancel;
                        const canEditStage =
                          (stage.status === "pending" || stage.status === "ready") &&
                          canModifyStages;
                        const canRemoveStage = canEditStage;

                        return (
                          <div
                            key={stage.id}
                            className={`flex items-start gap-2 rounded-box px-3 py-2 ${
                              isCurrent
                                ? "border border-primary/30 bg-primary/5"
                                : "bg-base-200/30"
                            }`}
                          >
                            <span className="mt-0.5 text-sm">
                              {stageIcon(stage.status)}
                            </span>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2">
                                <span className="truncate text-sm font-medium">
                                  {stage.title}
                                </span>
                                <span
                                  className={`badge badge-xs ${statusBadge[stage.status] ?? "badge-ghost"}`}
                                >
                                  {stage.status}
                                </span>
                                {isCurrent && (
                                  <span className="badge badge-xs badge-primary">
                                    current
                                  </span>
                                )}
                              </div>
                              {stage.goal && (
                                <p className="mt-0.5 text-xs text-base-content/60">
                                  {stage.goal}
                                </p>
                              )}
                            </div>
                            <div className="flex items-center gap-1 shrink-0">
                              {canSkip && (
                                <button
                                  type="button"
                                  className="btn btn-ghost btn-xs"
                                  title={t("skipStage", "Skip this stage")}
                                  disabled={loadingAction !== null}
                                  onClick={() =>
                                    wrapAction(
                                      `skip-${stage.id}`,
                                      () => onSkipStage(wf.id, stage.id),
                                    )
                                  }
                                >
                                  {loadingAction === `skip-${stage.id}` ? (
                                    <span className="loading loading-spinner loading-xs" />
                                  ) : (
                                    "⏭"
                                  )}
                                </button>
                              )}
                              {canModifyStages && (
                                <button
                                  type="button"
                                  className="btn btn-ghost btn-xs"
                                  title={t("insertStageAfter", "Insert stage after")}
                                  disabled={loadingAction !== null}
                                  onClick={() => openAddModal(wf.id, stage.id)}
                                >
                                  ＋
                                </button>
                              )}
                              {canEditStage && (
                                <button
                                  type="button"
                                  className="btn btn-ghost btn-xs"
                                  title={t("editStage", "Edit stage")}
                                  disabled={loadingAction !== null}
                                  onClick={() => openEditModal(stage)}
                                >
                                  ✏️
                                </button>
                              )}
                              {canRemoveStage && (
                                <button
                                  type="button"
                                  className="btn btn-ghost btn-xs text-error"
                                  title={t("removeStage", "Remove stage")}
                                  disabled={loadingAction !== null}
                                  onClick={() =>
                                    wrapAction(
                                      `remove-${stage.id}`,
                                      () => onRemoveStage(stage.id),
                                    )
                                  }
                                >
                                  {loadingAction === `remove-${stage.id}` ? (
                                    <span className="loading loading-spinner loading-xs" />
                                  ) : (
                                    "🗑"
                                  )}
                                </button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}

                  {/* Stage progress bar (collapsed) */}
                  {!expanded && stages.length > 0 && (
                    <div className="mt-2 flex gap-0.5 pl-8">
                      {stages.map((stage) => (
                        <div
                          key={stage.id}
                          className={`h-1.5 flex-1 rounded-full ${
                            stage.status === "completed"
                              ? "bg-success"
                              : stage.status === "running"
                                ? "bg-primary"
                                : stage.status === "blocked" ||
                                    stage.status === "cancelled"
                                  ? "bg-error"
                                  : "bg-base-300"
                          }`}
                          title={`${stage.title}: ${stage.status}`}
                        />
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </section>
    </>
  );
}

