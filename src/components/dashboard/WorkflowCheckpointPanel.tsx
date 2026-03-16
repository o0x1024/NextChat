import { useMemo } from "react";
import type { Language } from "../../store/preferencesStore";
import type {
  AgentProfile,
  TaskCard,
  WorkflowCheckpointRecord,
  WorkflowCheckpointStatus,
} from "../../types";
import { formatTime } from "./ui";

interface WorkflowCheckpointPanelProps {
  language: Language;
  tasks: TaskCard[];
  agents: AgentProfile[];
  checkpoints: WorkflowCheckpointRecord[];
  onOpenTask: (taskId?: string) => void;
}

const checkpointTone: Record<WorkflowCheckpointStatus, string> = {
  workflowPlanned: "badge-ghost",
  workflowRunning: "badge-info",
  workflowCompleted: "badge-success",
  stagePending: "badge-ghost",
  stageRunning: "badge-info",
  stageCompleted: "badge-success",
  taskReady: "badge-ghost",
  taskRunning: "badge-info",
  taskRetryableFailure: "badge-error",
  taskRetryScheduled: "badge-warning",
  taskReassigned: "badge-secondary",
  taskCompleted: "badge-success",
};

function checkpointLabel(status: WorkflowCheckpointStatus, language: Language) {
  const labels = {
    en: {
      workflowPlanned: "Workflow planned",
      workflowRunning: "Workflow running",
      workflowCompleted: "Workflow completed",
      stagePending: "Stage pending",
      stageRunning: "Stage running",
      stageCompleted: "Stage completed",
      taskReady: "Task ready",
      taskRunning: "Task running",
      taskRetryableFailure: "Retryable failure",
      taskRetryScheduled: "Retry scheduled",
      taskReassigned: "Task reassigned",
      taskCompleted: "Task completed",
      title: "Resume checkpoints",
      hint: "Latest checkpoint per active task. Use this to inspect retries, reassignment, and resume context.",
      empty: "No checkpoints for active tasks in this work group.",
      workspaceEmpty: "Empty workspace",
      workspaceEntries: "{{count}} entries",
      assignee: "Assignee",
      updated: "Updated",
      failures: "Failures",
      resumeHint: "Resume hint",
      lastError: "Last error",
      artifactSummary: "Artifacts",
      todoSnapshot: "Todo snapshot",
      openTask: "Open task",
    },
    zh: {
      workflowPlanned: "工作流已规划",
      workflowRunning: "工作流运行中",
      workflowCompleted: "工作流已完成",
      stagePending: "阶段待开始",
      stageRunning: "阶段运行中",
      stageCompleted: "阶段已完成",
      taskReady: "任务就绪",
      taskRunning: "任务执行中",
      taskRetryableFailure: "可重试失败",
      taskRetryScheduled: "已安排重试",
      taskReassigned: "任务已改派",
      taskCompleted: "任务已完成",
      title: "断点恢复",
      hint: "按任务展示最近一次 checkpoint，用来查看重试、改派和恢复上下文。",
      empty: "当前群组的活跃任务还没有 checkpoint。",
      workspaceEmpty: "空工作目录",
      workspaceEntries: "{{count}} 个条目",
      assignee: "执行人",
      updated: "更新时间",
      failures: "失败次数",
      resumeHint: "恢复提示",
      lastError: "最近错误",
      artifactSummary: "产物摘要",
      todoSnapshot: "待办快照",
      openTask: "打开任务",
    },
  } as const;

  return labels[language][status];
}

function interpolate(template: string, count: number) {
  return template.replace("{{count}}", String(count));
}

function previewLines(items: string[], limit: number) {
  return items.slice(0, limit);
}

export function WorkflowCheckpointPanel({
  language,
  tasks,
  agents,
  checkpoints,
  onOpenTask,
}: WorkflowCheckpointPanelProps) {
  const labels = useMemo(
    () => ({
      title: checkpointLabel("taskReady", language),
      titleText: language === "zh" ? "断点恢复" : "Resume checkpoints",
      hint:
        language === "zh"
          ? "按任务展示最近一次 checkpoint，用来查看重试、改派和恢复上下文。"
          : "Latest checkpoint per active task. Use this to inspect retries, reassignment, and resume context.",
      empty:
        language === "zh"
          ? "当前群组的活跃任务还没有 checkpoint。"
          : "No checkpoints for active tasks in this work group.",
      workspaceEmpty: language === "zh" ? "空工作目录" : "Empty workspace",
      assignee: language === "zh" ? "执行人" : "Assignee",
      updated: language === "zh" ? "更新时间" : "Updated",
      failures: language === "zh" ? "失败次数" : "Failures",
      resumeHint: language === "zh" ? "恢复提示" : "Resume hint",
      lastError: language === "zh" ? "最近错误" : "Last error",
      artifactSummary: language === "zh" ? "产物摘要" : "Artifacts",
      todoSnapshot: language === "zh" ? "待办快照" : "Todo snapshot",
      openTask: language === "zh" ? "打开任务" : "Open task",
      workspaceEntries: (count: number) =>
        interpolate(language === "zh" ? "{{count}} 个条目" : "{{count}} entries", count),
    }),
    [language],
  );

  const taskMap = useMemo(() => new Map(tasks.map((task) => [task.id, task])), [tasks]);
  const agentMap = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents]);

  const visibleCheckpoints = useMemo(() => {
    const activeTaskIds = new Set(
      tasks
        .filter((task) => !["completed", "cancelled"].includes(task.status))
        .map((task) => task.id),
    );
    const seenTaskIds = new Set<string>();
    return checkpoints
      .filter((checkpoint) => checkpoint.taskId && activeTaskIds.has(checkpoint.taskId))
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .filter((checkpoint) => {
        const taskId = checkpoint.taskId as string;
        if (seenTaskIds.has(taskId)) {
          return false;
        }
        seenTaskIds.add(taskId);
        return true;
      })
      .slice(0, 6);
  }, [checkpoints, tasks]);

  return (
    <section className="card card-border bg-base-100">
      <div className="card-body gap-3">
        <div className="flex items-center justify-between">
          <h3 className="card-title text-base">{labels.titleText}</h3>
          <span className="badge badge-secondary">{visibleCheckpoints.length}</span>
        </div>
        <p className="text-sm text-base-content/60">{labels.hint}</p>

        <div className="space-y-3">
          {visibleCheckpoints.map((checkpoint) => {
            const task = checkpoint.taskId ? taskMap.get(checkpoint.taskId) : null;
            const assignee =
              checkpoint.assigneeName ??
              (checkpoint.assigneeAgentId ? agentMap.get(checkpoint.assigneeAgentId)?.name : null) ??
              (language === "zh" ? "未分配" : "Unassigned");

            return (
              <div key={checkpoint.id} className="rounded-box bg-base-200 px-4 py-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 space-y-2">
                    <div className="flex flex-wrap items-center gap-2 text-xs">
                      <span className={`badge ${checkpointTone[checkpoint.status]}`}>
                        {checkpointLabel(checkpoint.status, language)}
                      </span>
                      <span className="badge badge-ghost">
                        {checkpoint.repoSnapshot.isEmpty
                          ? labels.workspaceEmpty
                          : labels.workspaceEntries(checkpoint.repoSnapshot.entryCount)}
                      </span>
                      {checkpoint.failureCount > 0 && (
                        <span className="badge badge-warning">
                          {labels.failures}: {checkpoint.failureCount}
                        </span>
                      )}
                    </div>
                    <strong className="block text-sm">
                      {checkpoint.taskTitle ?? task?.title ?? checkpoint.taskId ?? checkpoint.id}
                    </strong>
                    {checkpoint.stageTitle && (
                      <p className="text-xs text-base-content/70">{checkpoint.stageTitle}</p>
                    )}
                    <div className="flex flex-wrap gap-3 text-xs text-base-content/65">
                      <span>
                        {labels.assignee}: {assignee}
                      </span>
                      <span>
                        {labels.updated}: {formatTime(checkpoint.updatedAt, language)}
                      </span>
                    </div>
                  </div>
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs shrink-0"
                    onClick={() => onOpenTask(checkpoint.taskId ?? undefined)}
                  >
                    {labels.openTask}
                  </button>
                </div>

                {(checkpoint.resumeHint || checkpoint.lastError) && (
                  <div className="mt-3 space-y-2 border-t border-base-content/10 pt-3">
                    {checkpoint.resumeHint && (
                      <div className="space-y-1">
                        <div className="text-[11px] font-semibold uppercase tracking-wide text-base-content/50">
                          {labels.resumeHint}
                        </div>
                        <p className="whitespace-pre-wrap text-xs text-base-content/75">
                          {checkpoint.resumeHint}
                        </p>
                      </div>
                    )}
                    {checkpoint.lastError && (
                      <div className="space-y-1">
                        <div className="text-[11px] font-semibold uppercase tracking-wide text-base-content/50">
                          {labels.lastError}
                        </div>
                        <p className="line-clamp-3 whitespace-pre-wrap text-xs text-error/80">
                          {checkpoint.lastError}
                        </p>
                      </div>
                    )}
                  </div>
                )}

                {(checkpoint.artifactSummary.length > 0 || checkpoint.todoSnapshot.length > 0) && (
                  <div className="mt-3 grid gap-3 border-t border-base-content/10 pt-3 md:grid-cols-2">
                    {checkpoint.artifactSummary.length > 0 && (
                      <div className="space-y-2">
                        <div className="text-[11px] font-semibold uppercase tracking-wide text-base-content/50">
                          {labels.artifactSummary}
                        </div>
                        <div className="space-y-2">
                          {previewLines(checkpoint.artifactSummary, 4).map((item) => (
                            <p
                              key={item}
                              title={item}
                              className="rounded-box bg-base-100/70 px-3 py-2 text-xs leading-5 text-base-content/75 break-all whitespace-pre-wrap line-clamp-3"
                            >
                              {item}
                            </p>
                          ))}
                        </div>
                      </div>
                    )}
                    {checkpoint.todoSnapshot.length > 0 && (
                      <div className="space-y-2">
                        <div className="text-[11px] font-semibold uppercase tracking-wide text-base-content/50">
                          {labels.todoSnapshot}
                        </div>
                        <div className="space-y-1">
                          {previewLines(checkpoint.todoSnapshot, 3).map((item) => (
                            <p key={item} className="line-clamp-2 text-xs text-base-content/75">
                              {item}
                            </p>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}

          {visibleCheckpoints.length === 0 && (
            <div className="alert alert-soft">
              <span className="text-sm">{labels.empty}</span>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
