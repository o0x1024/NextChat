use std::collections::HashSet;

use anyhow::Result;
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::buildin_tools::ask_user_question::parse_signal_from_error;
use crate::core::domain::{
    new_id, now, AuditEvent, ConversationMessage, LeaseState, MessageKind, PendingUserQuestion,
    PendingUserQuestionStatus, SenderKind, TaskCard, TaskStatus, ToolRun, ToolRunState, Visibility,
};
use crate::core::permissions::is_permission_guard_error;

#[derive(Default)]
struct RecoveryReport {
    paused_tasks: usize,
    review_tasks: usize,
    resumed_approvals: usize,
    requeued_tool_runs: usize,
    cancelled_tool_runs: usize,
    paused_leases: usize,
    released_leases: usize,
    reconciled_parents: usize,
}

pub(super) struct TaskFailureReport {
    pub(super) task: Option<TaskCard>,
    pub(super) cancelled_tool_runs: Vec<ToolRun>,
    pub(super) audit_event: AuditEvent,
    pub(super) message: Option<ConversationMessage>,
}

impl AppService {
    pub(super) fn handle_pending_user_question_request<R: Runtime>(
        &self,
        _app: &AppHandle<R>,
        task_card_id: &str,
        tool_run_id: Option<&str>,
        error: &anyhow::Error,
    ) -> Result<Option<TaskFailureReport>> {
        let Some(signal) = parse_signal_from_error(&error.to_string())? else {
            return Ok(None);
        };

        let mut task = self.storage.get_task_card(task_card_id)?;
        task.status = TaskStatus::WaitingUserInput;
        self.storage.update_task_card(&task)?;

        if let Some(mut lease) = self.storage.get_lease_by_task(task_card_id)? {
            lease.state = LeaseState::Paused;
            lease.preempt_requested_at = None;
            self.storage.update_lease(&lease)?;
        }

        if let Some(tool_run_id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(tool_run_id)?;
            if !matches!(
                tool_run.state,
                ToolRunState::Completed | ToolRunState::Cancelled
            ) {
                tool_run.state = ToolRunState::Queued;
                tool_run.started_at = None;
                tool_run.finished_at = None;
                self.storage.insert_tool_run(&tool_run)?;
            }
        }

        let agent = task
            .assigned_agent_id
            .as_deref()
            .map(|agent_id| self.storage.get_agent(agent_id))
            .transpose()?
            .unwrap_or_else(|| crate::core::domain::AgentProfile {
                id: "unknown".into(),
                name: "Unknown".into(),
                avatar: "??".into(),
                role: "Unknown".into(),
                objective: "".into(),
                model_policy: crate::core::domain::ModelPolicy::default(),
                skill_ids: vec![],
                tool_ids: vec![],
                max_parallel_runs: 1,
                can_spawn_subtasks: false,
                memory_policy: crate::core::domain::MemoryPolicy::default(),
                permission_policy: crate::core::domain::AgentPermissionPolicy::default(),
            });
        let (question_message, blocker) = self.build_pending_user_question_message(
            &task,
            &agent,
            &signal.question,
            &signal.options,
            signal.context.as_deref(),
        )?;
        self.storage.insert_message(&question_message)?;
        self.storage.insert_task_blocker(&blocker)?;

        self.storage
            .insert_pending_user_question(&PendingUserQuestion {
                id: new_id(),
                work_group_id: task.work_group_id.clone(),
                task_card_id: task.id.clone(),
                agent_id: agent.id.clone(),
                tool_run_id: tool_run_id.map(ToOwned::to_owned),
                question: signal.question.clone(),
                options: signal.options.clone(),
                context: signal.context.clone(),
                allow_free_form: signal.allow_free_form,
                asked_message_id: question_message.id.clone(),
                answer_message_id: None,
                status: PendingUserQuestionStatus::Pending,
                created_at: now(),
                answered_at: None,
            })?;

        let audit_event = AuditEvent {
            id: new_id(),
            event_type: "task.awaiting_user_input".into(),
            entity_type: "task_card".into(),
            entity_id: task.id.clone(),
            payload_json: json!({
                "question": signal.question,
                "toolRunId": tool_run_id,
            })
            .to_string(),
            created_at: now(),
        };
        self.storage.insert_audit_event(&audit_event)?;

        Ok(Some(TaskFailureReport {
            task: Some(task),
            cancelled_tool_runs: Vec::new(),
            audit_event,
            message: Some(question_message),
        }))
    }

    pub(super) fn preempt_active_leases<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        work_group_id: &str,
    ) -> Result<()> {
        let leases = self.storage.list_active_leases_for_group(work_group_id)?;
        for mut lease in leases {
            lease.state = LeaseState::PreemptRequested;
            lease.preempt_requested_at = Some(now());
            self.storage.update_lease(&lease)?;
            emit(app, "lease:preempt-requested", &lease)?;
            self.record_audit(
                "lease.preempt_requested",
                "lease",
                &lease.id,
                json!({ "taskCardId": lease.task_card_id }),
            )?;
        }
        Ok(())
    }

    pub(super) fn recover_runtime_state(&self) -> Result<()> {
        let mut report = RecoveryReport::default();
        let tool_runs = self.storage.list_tool_runs()?;

        for mut tool_run in tool_runs {
            match tool_run.state {
                ToolRunState::Completed | ToolRunState::Cancelled => continue,
                ToolRunState::PendingApproval => {
                    let mut task = self.storage.get_task_card(&tool_run.task_card_id)?;
                    if matches!(
                        task.status,
                        TaskStatus::Completed
                            | TaskStatus::Cancelled
                            | TaskStatus::NeedsReview
                            | TaskStatus::WaitingUserInput
                    ) {
                        continue;
                    }
                    if task.status != TaskStatus::WaitingApproval {
                        task.status = TaskStatus::WaitingApproval;
                        self.storage.update_task_card(&task)?;
                        report.resumed_approvals += 1;
                    }
                    if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                        if lease.state != LeaseState::Paused {
                            lease.state = LeaseState::Paused;
                            lease.preempt_requested_at = None;
                            self.storage.update_lease(&lease)?;
                            report.paused_leases += 1;
                        }
                    }
                }
                ToolRunState::Queued => {
                    let mut task = self.storage.get_task_card(&tool_run.task_card_id)?;
                    if task.status == TaskStatus::WaitingUserInput {
                        if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                            if lease.state != LeaseState::Paused {
                                lease.state = LeaseState::Paused;
                                lease.preempt_requested_at = None;
                                self.storage.update_lease(&lease)?;
                                report.paused_leases += 1;
                            }
                        }
                        continue;
                    }
                    if matches!(
                        task.status,
                        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
                    ) {
                        tool_run.state = ToolRunState::Cancelled;
                        tool_run.finished_at = Some(now());
                        self.storage.insert_tool_run(&tool_run)?;
                        report.cancelled_tool_runs += 1;
                        continue;
                    }
                    if task.status != TaskStatus::Paused {
                        task.status = TaskStatus::Paused;
                        self.storage.update_task_card(&task)?;
                        report.paused_tasks += 1;
                    }
                    if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                        if lease.state != LeaseState::Paused {
                            lease.state = LeaseState::Paused;
                            lease.preempt_requested_at = None;
                            self.storage.update_lease(&lease)?;
                            report.paused_leases += 1;
                        }
                    }
                    report.requeued_tool_runs += 1;
                }
                ToolRunState::Running => {
                    let mut task = self.storage.get_task_card(&tool_run.task_card_id)?;
                    if tool_run.approval_required {
                        tool_run.state = ToolRunState::Cancelled;
                        tool_run.finished_at = Some(now());
                        self.storage.insert_tool_run(&tool_run)?;
                        report.cancelled_tool_runs += 1;

                        if task.status != TaskStatus::NeedsReview {
                            task.status = TaskStatus::NeedsReview;
                            self.storage.update_task_card(&task)?;
                            report.review_tasks += 1;
                        }
                    } else {
                        tool_run.state = ToolRunState::Queued;
                        tool_run.started_at = None;
                        tool_run.finished_at = None;
                        self.storage.insert_tool_run(&tool_run)?;
                        report.requeued_tool_runs += 1;

                        if task.status != TaskStatus::Paused {
                            task.status = TaskStatus::Paused;
                            self.storage.update_task_card(&task)?;
                            report.paused_tasks += 1;
                        }
                    }

                    if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                        if matches!(task.status, TaskStatus::NeedsReview) {
                            if lease.state != LeaseState::Released {
                                lease.state = LeaseState::Released;
                                lease.released_at = Some(now());
                                lease.preempt_requested_at = None;
                                self.storage.update_lease(&lease)?;
                                report.released_leases += 1;
                            }
                        } else if lease.state != LeaseState::Paused {
                            lease.state = LeaseState::Paused;
                            lease.preempt_requested_at = None;
                            self.storage.update_lease(&lease)?;
                            report.paused_leases += 1;
                        }
                    }
                }
            }
        }

        for mut lease in self.storage.list_leases()? {
            if matches!(lease.state, LeaseState::Released | LeaseState::Paused) {
                continue;
            }
            let task = self.storage.get_task_card(&lease.task_card_id)?;
            if matches!(
                task.status,
                TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
            ) {
                lease.state = LeaseState::Released;
                lease.released_at = Some(lease.released_at.unwrap_or_else(now));
                lease.preempt_requested_at = None;
                self.storage.update_lease(&lease)?;
                report.released_leases += 1;
            } else {
                lease.state = LeaseState::Paused;
                lease.preempt_requested_at = None;
                self.storage.update_lease(&lease)?;
                report.paused_leases += 1;
            }
        }

        for mut task in self.storage.list_task_cards(None)? {
            if matches!(
                task.status,
                TaskStatus::Completed
                    | TaskStatus::Cancelled
                    | TaskStatus::NeedsReview
                    | TaskStatus::WaitingApproval
                    | TaskStatus::WaitingUserInput
            ) {
                continue;
            }
            if self.storage.get_lease_by_task(&task.id)?.is_none() {
                task.status = TaskStatus::NeedsReview;
                self.storage.update_task_card(&task)?;
                report.review_tasks += 1;
            }
        }

        let parent_ids: HashSet<String> = self
            .storage
            .list_task_cards(None)?
            .into_iter()
            .filter_map(|task| task.parent_id)
            .collect();
        for parent_id in parent_ids {
            if self.reconcile_parent_task_state(&parent_id)? {
                report.reconciled_parents += 1;
            }
        }

        if report.paused_tasks > 0
            || report.review_tasks > 0
            || report.resumed_approvals > 0
            || report.requeued_tool_runs > 0
            || report.cancelled_tool_runs > 0
            || report.paused_leases > 0
            || report.released_leases > 0
            || report.reconciled_parents > 0
        {
            self.record_audit(
                "runtime.recovered",
                "system",
                "startup",
                json!({
                    "pausedTasks": report.paused_tasks,
                    "reviewTasks": report.review_tasks,
                    "resumedApprovals": report.resumed_approvals,
                    "requeuedToolRuns": report.requeued_tool_runs,
                    "cancelledToolRuns": report.cancelled_tool_runs,
                    "pausedLeases": report.paused_leases,
                    "releasedLeases": report.released_leases,
                    "reconciledParents": report.reconciled_parents,
                }),
            )?;
        }

        Ok(())
    }

    pub(super) fn handle_task_execution_failure(
        &self,
        task_card_id: &str,
        error: &anyhow::Error,
    ) -> Result<TaskFailureReport> {
        let mut task = self.storage.get_task_card(task_card_id).ok();
        let mut cancelled_tool_runs = Vec::new();
        let error_message = error.to_string();
        let permission_guard_error = is_permission_guard_error(&error_message);

        if let Some(ref mut current_task) = task {
            if !matches!(
                current_task.status,
                TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
            ) {
                current_task.status = TaskStatus::NeedsReview;
                self.storage.update_task_card(current_task)?;
            }
        }

        if let Some(mut lease) = self.storage.get_lease_by_task(task_card_id)? {
            if lease.state != LeaseState::Released {
                lease.state = LeaseState::Released;
                lease.released_at = Some(now());
                lease.preempt_requested_at = None;
                self.storage.update_lease(&lease)?;
            }
        }

        for mut tool_run in self
            .storage
            .list_tool_runs()?
            .into_iter()
            .filter(|run| run.task_card_id == task_card_id)
        {
            if matches!(
                tool_run.state,
                ToolRunState::Completed | ToolRunState::Cancelled
            ) {
                continue;
            }
            tool_run.state = ToolRunState::Cancelled;
            tool_run.finished_at = Some(now());
            self.storage.insert_tool_run(&tool_run)?;
            cancelled_tool_runs.push(tool_run);
        }

        let message = task.as_ref().map(|current_task| {
            let mut message = ConversationMessage {
                id: new_id(),
                conversation_id: current_task.work_group_id.clone(),
                work_group_id: current_task.work_group_id.clone(),
                sender_kind: SenderKind::System,
                sender_id: "coordinator".into(),
                sender_name: "Coordinator".into(),
                kind: MessageKind::Status,
                visibility: Visibility::Main,
                content: if permission_guard_error {
                    format!("Execution blocked by permission guard. {error_message}")
                } else {
                    format!("Task execution failed and was moved to review. {error_message}")
                },
                mentions: vec![],
                task_card_id: Some(current_task.id.clone()),
                execution_mode: None,
                created_at: now(),
            };
            self.assign_group_owner_sender(&mut message)?;
            Ok::<ConversationMessage, anyhow::Error>(message)
        });
        let message = message.transpose()?;
        if let Some(ref message) = message {
            self.storage.insert_message(message)?;
        }

        let audit_event = AuditEvent {
            id: new_id(),
            event_type: if permission_guard_error {
                "tool_run.permission_denied".into()
            } else {
                "task.execution_error".into()
            },
            entity_type: "task_card".into(),
            entity_id: task_card_id.into(),
            payload_json: json!({ "error": error_message }).to_string(),
            created_at: now(),
        };
        self.storage.insert_audit_event(&audit_event)?;

        Ok(TaskFailureReport {
            task,
            cancelled_tool_runs,
            audit_event,
            message,
        })
    }
}
