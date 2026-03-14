use std::{fs, path::Path, time::Duration};

use anyhow::Result;
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, runtime_recovery::TaskFailureReport, AppService};
use crate::core::domain::{
    new_id, now, AgentProfile, AuditEvent, ConversationMessage, Lease, LeaseState, MessageKind,
    SenderKind, TaskCard, TaskStatus, ToolRunState, Visibility,
};
use crate::core::workflow::{
    NarrativeMessageType, RequestRouteMode, TaskDispatchRecord, WorkflowCheckpointRecord,
    WorkflowCheckpointStatus, WorkflowRepoSnapshot,
};

const MAX_RETRYABLE_FAILURE_RETRIES: i64 = 2;
const MAX_REPO_SNAPSHOT_ENTRIES: usize = 12;
const MAX_ARTIFACT_SUMMARIES: usize = 3;
const GREENFIELD_RESUME_HINT: &str = "恢复执行要求：上一轮在规划或架构阶段遇到可恢复错误。不要重复需求访谈或长篇方案文档；如果当前工作目录为空，直接初始化项目并先实现一个可运行的 MVP。";

impl AppService {
    pub(super) fn record_workflow_checkpoint(
        &self,
        workflow_id: &str,
        status: WorkflowCheckpointStatus,
    ) -> Result<WorkflowCheckpointRecord> {
        let workflow = self.storage.get_workflow(workflow_id)?;
        let work_group = self.storage.get_work_group(&workflow.work_group_id)?;
        let stages = self.storage.list_workflow_stages(workflow_id)?;
        let repo_snapshot = build_repo_snapshot(&work_group.working_directory);
        let checkpoint = WorkflowCheckpointRecord {
            id: new_id(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: workflow.current_stage_id.clone(),
            task_id: None,
            stage_title: workflow
                .current_stage_id
                .as_ref()
                .and_then(|stage_id| stages.iter().find(|stage| &stage.id == stage_id))
                .map(|stage| stage.title.clone()),
            task_title: None,
            assignee_agent_id: None,
            assignee_name: None,
            status,
            working_directory: work_group.working_directory.clone(),
            repo_snapshot,
            artifact_summary: stages
                .iter()
                .map(|stage| format!("{}: {:?}", stage.title, stage.status))
                .collect(),
            todo_snapshot: stages
                .iter()
                .map(|stage| format!("{} => {:?}", stage.title, stage.status))
                .collect(),
            resume_hint: None,
            failure_count: 0,
            last_error: None,
            created_at: now(),
            updated_at: now(),
        };
        self.storage.insert_workflow_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub(super) fn record_stage_checkpoint(
        &self,
        stage_id: &str,
        status: WorkflowCheckpointStatus,
    ) -> Result<WorkflowCheckpointRecord> {
        let stage = self.storage.get_workflow_stage(stage_id)?;
        let workflow = self.storage.get_workflow(&stage.workflow_id)?;
        let work_group = self.storage.get_work_group(&workflow.work_group_id)?;
        let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
        let todo_snapshot = dispatches
            .iter()
            .map(|dispatch| {
                let task = self.storage.get_task_card(&dispatch.task_id)?;
                Ok(format!("{} => {:?}", task.title, task.status))
            })
            .collect::<Result<Vec<_>>>()?;
        let checkpoint = WorkflowCheckpointRecord {
            id: new_id(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: Some(stage.id.clone()),
            task_id: None,
            stage_title: Some(stage.title.clone()),
            task_title: None,
            assignee_agent_id: None,
            assignee_name: None,
            status,
            working_directory: work_group.working_directory.clone(),
            repo_snapshot: build_repo_snapshot(&work_group.working_directory),
            artifact_summary: todo_snapshot
                .iter()
                .take(MAX_ARTIFACT_SUMMARIES)
                .cloned()
                .collect(),
            todo_snapshot,
            resume_hint: None,
            failure_count: 0,
            last_error: None,
            created_at: now(),
            updated_at: now(),
        };
        self.storage.insert_workflow_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub(super) fn record_task_checkpoint(
        &self,
        task: &TaskCard,
        status: WorkflowCheckpointStatus,
        failure_count: i64,
        resume_hint: Option<String>,
        last_error: Option<String>,
    ) -> Result<WorkflowCheckpointRecord> {
        let work_group = self.storage.get_work_group(&task.work_group_id)?;
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let assignee = task
            .assigned_agent_id
            .as_deref()
            .map(|agent_id| self.storage.get_agent(agent_id))
            .transpose()?;
        let checkpoint = WorkflowCheckpointRecord {
            id: new_id(),
            workflow_id: dispatch.as_ref().and_then(|item| item.workflow_id.clone()),
            stage_id: dispatch.as_ref().and_then(|item| item.stage_id.clone()),
            task_id: Some(task.id.clone()),
            stage_title: dispatch
                .as_ref()
                .and_then(|item| item.narrative_stage_label.clone()),
            task_title: Some(task.title.clone()),
            assignee_agent_id: assignee.as_ref().map(|agent| agent.id.clone()),
            assignee_name: assignee.as_ref().map(|agent| agent.name.clone()),
            status,
            working_directory: work_group.working_directory.clone(),
            repo_snapshot: build_repo_snapshot(&work_group.working_directory),
            artifact_summary: self.recent_task_artifacts(task)?,
            todo_snapshot: build_task_todo_snapshot(task, dispatch.as_ref()),
            resume_hint,
            failure_count,
            last_error,
            created_at: now(),
            updated_at: now(),
        };
        self.storage.insert_workflow_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub(super) fn handle_retryable_task_execution_failure<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task_card_id: &str,
        tool_run_id: Option<&str>,
        error: &anyhow::Error,
    ) -> Result<Option<TaskFailureReport>> {
        if !is_retryable_execution_error(error) {
            return Ok(None);
        }

        let task = self.storage.get_task_card(task_card_id)?;
        let dispatch = self.storage.get_task_dispatch(task_card_id)?;
        let latest_failure_count = self
            .storage
            .latest_workflow_checkpoint_for_task(task_card_id)?
            .map(|checkpoint| checkpoint.failure_count)
            .unwrap_or(0);
        let next_failure_count = latest_failure_count + 1;
        let work_group = self.storage.get_work_group(&task.work_group_id)?;
        let repo_snapshot = build_repo_snapshot(&work_group.working_directory);
        let retry_hint = build_retry_resume_hint(&task, &dispatch, &repo_snapshot);

        self.record_task_checkpoint(
            &task,
            WorkflowCheckpointStatus::TaskRetryableFailure,
            next_failure_count,
            retry_hint.clone(),
            Some(error.to_string()),
        )?;

        if next_failure_count <= MAX_RETRYABLE_FAILURE_RETRIES {
            return self
                .schedule_retryable_task_retry(
                    app,
                    task,
                    tool_run_id,
                    next_failure_count,
                    retry_hint,
                )
                .map(Some);
        }

        if let Some(report) = self.try_reassign_retryable_task(
            app,
            task,
            tool_run_id,
            next_failure_count,
            retry_hint,
            &repo_snapshot,
        )? {
            return Ok(Some(report));
        }

        Ok(None)
    }

    fn schedule_retryable_task_retry<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        mut task: TaskCard,
        tool_run_id: Option<&str>,
        failure_count: i64,
        resume_hint: Option<String>,
    ) -> Result<TaskFailureReport> {
        task.status = TaskStatus::Paused;
        self.storage.update_task_card(&task)?;

        if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
            lease.state = LeaseState::Paused;
            lease.preempt_requested_at = None;
            self.storage.update_lease(&lease)?;
        }

        if let Some(tool_run_id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(tool_run_id)?;
            if tool_run.state != ToolRunState::Completed {
                tool_run.state = ToolRunState::Queued;
                tool_run.started_at = None;
                tool_run.finished_at = None;
                self.storage.insert_tool_run(&tool_run)?;
            }
        }

        self.record_task_checkpoint(
            &task,
            WorkflowCheckpointStatus::TaskRetryScheduled,
            failure_count,
            resume_hint,
            None,
        )?;

        let message = self.build_resume_status_message(
            &task,
            format!("遇到可恢复错误，已从最近断点安排重试（{failure_count}/{MAX_RETRYABLE_FAILURE_RETRIES}）。"),
        )?;
        self.storage.insert_message(&message)?;

        let audit_event = AuditEvent {
            id: new_id(),
            event_type: "task.execution_retry_scheduled".into(),
            entity_type: "task_card".into(),
            entity_id: task.id.clone(),
            payload_json: json!({
                "failureCount": failure_count,
                "taskId": task.id,
            })
            .to_string(),
            created_at: now(),
        };
        self.storage.insert_audit_event(&audit_event)?;

        let service = self.clone();
        let retry_app = app.clone();
        let retry_task_id = task.id.clone();
        let retry_tool_run_id = tool_run_id.map(|value| value.to_string());
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(retry_backoff_duration(failure_count)).await;
            let Ok(mut current_task) = service.storage.get_task_card(&retry_task_id) else {
                return;
            };
            if current_task.status != TaskStatus::Paused {
                return;
            }
            current_task.status = TaskStatus::Leased;
            let _ = service.storage.update_task_card(&current_task);
            let _ = emit(&retry_app, "task:status-changed", &current_task);
            if let Ok(Some(mut lease)) = service.storage.get_lease_by_task(&retry_task_id) {
                lease.state = LeaseState::Active;
                lease.preempt_requested_at = None;
                let _ = service.storage.update_lease(&lease);
                let _ = emit(&retry_app, "lease:granted", &lease);
            }
            service.spawn_task_execution(retry_app, retry_task_id, retry_tool_run_id);
        });

        Ok(TaskFailureReport {
            task: Some(task),
            cancelled_tool_runs: Vec::new(),
            audit_event,
            message: Some(message),
        })
    }

    fn try_reassign_retryable_task<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        mut task: TaskCard,
        tool_run_id: Option<&str>,
        failure_count: i64,
        resume_hint: Option<String>,
        repo_snapshot: &WorkflowRepoSnapshot,
    ) -> Result<Option<TaskFailureReport>> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let Some(dispatch) = dispatch else {
            return Ok(None);
        };
        if dispatch.route_mode != RequestRouteMode::OwnerOrchestrated {
            return Ok(None);
        }
        if !should_force_greenfield_handoff(&task, repo_snapshot) {
            return Ok(None);
        }

        let fallback_agent = self
            .find_resume_fallback_agent(&task.work_group_id, task.assigned_agent_id.as_deref())?;
        let Some(fallback_agent) = fallback_agent else {
            return Ok(None);
        };

        task.assigned_agent_id = Some(fallback_agent.id.clone());
        task.status = TaskStatus::Leased;
        if !task.input_payload.contains(GREENFIELD_RESUME_HINT) {
            task.input_payload = format!(
                "{}\n\n{}",
                task.input_payload.trim(),
                GREENFIELD_RESUME_HINT
            );
        }
        if !task.normalized_goal.contains("恢复执行要求") {
            task.normalized_goal = format!(
                "{}\n\n{}",
                task.normalized_goal.trim(),
                GREENFIELD_RESUME_HINT
            );
        }
        self.storage.update_task_card(&task)?;

        if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
            lease.owner_agent_id = fallback_agent.id.clone();
            lease.state = LeaseState::Active;
            lease.preempt_requested_at = None;
            lease.released_at = None;
            self.storage.update_lease(&lease)?;
        } else {
            self.storage.insert_lease(&Lease {
                id: new_id(),
                task_card_id: task.id.clone(),
                owner_agent_id: fallback_agent.id.clone(),
                state: LeaseState::Active,
                granted_at: now(),
                expires_at: None,
                preempt_requested_at: None,
                released_at: None,
            })?;
        }

        let mut updated_dispatch = dispatch.clone();
        updated_dispatch.target_agent_id = fallback_agent.id.clone();
        updated_dispatch.acknowledged_at = Some(now());
        self.storage.insert_task_dispatch(&updated_dispatch)?;

        if let Some(tool_run_id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(tool_run_id)?;
            tool_run.agent_id = fallback_agent.id.clone();
            tool_run.state = ToolRunState::Queued;
            tool_run.started_at = None;
            tool_run.finished_at = None;
            self.storage.insert_tool_run(&tool_run)?;
        }

        self.record_task_checkpoint(
            &task,
            WorkflowCheckpointStatus::TaskReassigned,
            failure_count,
            resume_hint.or_else(|| Some(GREENFIELD_RESUME_HINT.to_string())),
            None,
        )?;

        let owner_message = self.build_resume_status_message(
            &task,
            format!(
                "上一轮在规划阶段遇到可恢复错误，已从断点切换给 {} 直接实现 MVP。",
                fallback_agent.name
            ),
        )?;
        self.storage.insert_message(&owner_message)?;

        let audit_event = AuditEvent {
            id: new_id(),
            event_type: "task.execution_reassigned_after_retryable_failure".into(),
            entity_type: "task_card".into(),
            entity_id: task.id.clone(),
            payload_json: json!({
                "failureCount": failure_count,
                "agentId": fallback_agent.id,
                "taskId": task.id,
            })
            .to_string(),
            created_at: now(),
        };
        self.storage.insert_audit_event(&audit_event)?;

        self.spawn_task_execution(
            app.clone(),
            task.id.clone(),
            tool_run_id.map(|value| value.to_string()),
        );

        Ok(Some(TaskFailureReport {
            task: Some(task),
            cancelled_tool_runs: Vec::new(),
            audit_event,
            message: Some(owner_message),
        }))
    }

    fn find_resume_fallback_agent(
        &self,
        work_group_id: &str,
        current_agent_id: Option<&str>,
    ) -> Result<Option<AgentProfile>> {
        let work_group = self.storage.get_work_group(work_group_id)?;
        let owner_id = self.storage.get_work_group_owner_id(work_group_id)?;
        let agents = self.storage.list_agents()?;
        Ok(agents
            .into_iter()
            .filter(|agent| work_group.member_agent_ids.contains(&agent.id))
            .filter(|agent| owner_id.as_deref() != Some(agent.id.as_str()))
            .filter(|agent| Some(agent.id.as_str()) != current_agent_id)
            .find(|agent| looks_like_execution_agent(agent)))
    }

    fn recent_task_artifacts(&self, task: &TaskCard) -> Result<Vec<String>> {
        let messages = self.storage.list_messages_for_group(&task.work_group_id)?;
        Ok(messages
            .into_iter()
            .filter(|message| message.task_card_id.as_deref() == Some(task.id.as_str()))
            .filter(|message| {
                matches!(
                    message.kind,
                    MessageKind::Summary | MessageKind::Status | MessageKind::ToolResult
                )
            })
            .rev()
            .take(MAX_ARTIFACT_SUMMARIES)
            .map(|message| {
                message
                    .content
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .filter(|line| !line.is_empty())
            .collect())
    }

    fn build_resume_status_message(
        &self,
        task: &TaskCard,
        text: String,
    ) -> Result<ConversationMessage> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let content = if dispatch
            .as_ref()
            .is_some_and(|item| item.route_mode == RequestRouteMode::OwnerOrchestrated)
        {
            self.build_task_narrative_content(task, NarrativeMessageType::OwnerDispatch, text)?
        } else {
            text
        };
        let mut message = ConversationMessage {
            id: new_id(),
            conversation_id: task.work_group_id.clone(),
            work_group_id: task.work_group_id.clone(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content,
            narrative_meta: None,
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        if dispatch
            .as_ref()
            .is_some_and(|item| item.route_mode == RequestRouteMode::OwnerOrchestrated)
        {
            self.assign_group_owner_sender(&mut message)?;
        }
        Ok(message)
    }
}

fn build_repo_snapshot(working_directory: &str) -> WorkflowRepoSnapshot {
    let path = Path::new(working_directory);
    let entries = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|reader| reader.filter_map(|entry| entry.ok()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();

    let mut top_level_entries = entries;
    top_level_entries.sort();
    if top_level_entries.len() > MAX_REPO_SNAPSHOT_ENTRIES {
        top_level_entries.truncate(MAX_REPO_SNAPSHOT_ENTRIES);
    }
    let entry_count = top_level_entries.len() as i64;

    WorkflowRepoSnapshot {
        entry_count,
        is_empty: entry_count == 0,
        top_level_entries,
    }
}

fn build_task_todo_snapshot(task: &TaskCard, dispatch: Option<&TaskDispatchRecord>) -> Vec<String> {
    let mut snapshot = vec![format!("task_status => {:?}", task.status)];
    if let Some(dispatch) = dispatch {
        if !dispatch.depends_on_task_ids.is_empty() {
            snapshot.push(format!(
                "depends_on => {}",
                dispatch.depends_on_task_ids.join(", ")
            ));
        }
        snapshot.push(format!("target_agent => {}", dispatch.target_agent_id));
    }
    snapshot
}

fn is_retryable_execution_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    let lowered = message.to_lowercase();
    lowered.contains("500 internal server error")
        || lowered.contains("internalserviceerror")
        || lowered.contains("http error 500")
        || (lowered.contains("invalid status code 500") && lowered.contains("internal"))
}

fn build_retry_resume_hint(
    task: &TaskCard,
    dispatch: &Option<TaskDispatchRecord>,
    repo_snapshot: &WorkflowRepoSnapshot,
) -> Option<String> {
    if should_force_greenfield_handoff(task, repo_snapshot) {
        return Some(GREENFIELD_RESUME_HINT.to_string());
    }
    dispatch
        .as_ref()
        .and_then(|item| item.narrative_stage_label.clone())
        .map(|stage| format!("恢复执行要求：从当前阶段“{stage}”继续，不要重复前序已完成内容。"))
}

fn should_force_greenfield_handoff(task: &TaskCard, repo_snapshot: &WorkflowRepoSnapshot) -> bool {
    if !repo_snapshot.is_empty {
        return false;
    }
    let lowered = task.normalized_goal.to_lowercase();
    ["架构", "技术选型", "模块", "规划", "方案", "需求"]
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

fn looks_like_execution_agent(agent: &AgentProfile) -> bool {
    let lowered = format!("{} {} {}", agent.name, agent.role, agent.objective).to_lowercase();
    lowered.contains("全栈")
        || lowered.contains("builder")
        || lowered.contains("frontend")
        || lowered.contains("full-stack")
        || agent
            .tool_ids
            .iter()
            .any(|tool_id| matches!(tool_id.as_str(), "Write" | "Edit" | "Bash"))
}

fn retry_backoff_duration(failure_count: i64) -> Duration {
    match failure_count {
        0 | 1 => Duration::from_secs(2),
        2 => Duration::from_secs(5),
        _ => Duration::from_secs(10),
    }
}
