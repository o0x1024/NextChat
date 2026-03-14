/// Peer collaboration: handles the RequestPeerInput tool signal.
///
/// When an agent calls RequestPeerInput (targetAgentId, question), this module:
/// 1. Creates a sub-TaskCard assigned to the peer agent with the question as its goal.
/// 2. Adds the sub-task as a dependency on the calling task's dispatch record.
/// 3. Pauses the calling task (status = Paused) and blocks the stage if applicable.
/// 4. When the sub-task later completes (`output_summary` set), the existing
///    dependency resolution path in `runtime.rs` will unblock and resume the
///    calling task automatically.
use anyhow::{Context, Result};
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::buildin_tools::request_peer_input::{
    parse_peer_input_signal, RequestPeerInputSignal,
};
use crate::core::domain::{
    new_id, now, ConversationMessage, MessageKind, SenderKind, TaskCard, TaskStatus, Visibility,
};
use crate::core::workflow::{
    BlockerCategory, BlockerResolutionTarget, BlockerStatus, NarrativeEnvelope,
    NarrativeMessageType, StageStatus, TaskBlockerRecord, TaskDispatchRecord, TaskDispatchSource,
    WorkflowStatus,
};

/// Context returned from a successful peer input request.
pub(super) struct PeerInputRequestReport {
    pub(super) calling_task: TaskCard,
    pub(super) peer_task: TaskCard,
}

impl AppService {
    /// Attempt to handle a `RequestPeerInput` signal embedded in an error
    /// string that propagated from a tool execution.
    ///
    /// Returns `Some(report)` if the signal was present and handled, `None`
    /// if the error string does not embed a `RequestPeerInput` signal.
    pub(super) fn handle_peer_input_request<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task_card_id: &str,
        error: &anyhow::Error,
    ) -> Result<Option<PeerInputRequestReport>> {
        let Some(signal) = parse_peer_input_signal(&error.to_string())? else {
            return Ok(None);
        };

        let result = self.create_peer_sub_task(app, task_card_id, &signal)?;
        Ok(Some(result))
    }

    fn create_peer_sub_task<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        calling_task_id: &str,
        signal: &RequestPeerInputSignal,
    ) -> Result<PeerInputRequestReport> {
        let mut calling_task = self.storage.get_task_card(calling_task_id)?;

        // Validate the peer agent exists and is a member of the same work group.
        let peer_agent = self.storage.get_agent(&signal.target_agent_id)?;
        let work_group = self.storage.get_work_group(&calling_task.work_group_id)?;
        if !work_group.member_agent_ids.contains(&peer_agent.id) {
            anyhow::bail!(
                "RequestPeerInput: target agent '{}' is not a member of work group '{}'",
                peer_agent.id,
                work_group.id
            );
        }

        let calling_agent_id = calling_task
            .assigned_agent_id
            .clone()
            .context("calling task has no assigned agent")?;
        let calling_agent = self.storage.get_agent(&calling_agent_id)?;

        // Retrieve dispatch info so we can inherit workflow / stage context.
        let calling_dispatch = self.storage.get_task_dispatch(calling_task_id)?;

        // --- Build the goal text for the peer sub-task ---
        let peer_goal = if let Some(ctx) = &signal.context {
            format!("{}\n\n上下文：{ctx}", signal.question)
        } else {
            signal.question.clone()
        };
        let peer_title: String = signal
            .question
            .trim()
            .chars()
            .take(72)
            .collect::<String>();
        let peer_title = if peer_title.len() < signal.question.trim().len() {
            format!("{}…", peer_title)
        } else {
            peer_title
        };

        // --- Create the peer sub-task ---
        let mut peer_task = TaskCard {
            id: new_id(),
            parent_id: Some(calling_task.id.clone()),
            source_message_id: calling_task.source_message_id.clone(),
            title: peer_title,
            normalized_goal: peer_goal.clone(),
            input_payload: peer_goal.clone(),
            priority: calling_task.priority,
            status: TaskStatus::Pending,
            work_group_id: calling_task.work_group_id.clone(),
            created_by: calling_agent_id.clone(),
            assigned_agent_id: Some(peer_agent.id.clone()),
            output_summary: None,
            created_at: now(),
        };
        self.storage.insert_task_card(&peer_task)?;
        emit(app, "task:card-created", &peer_task)?;

        // Dispatch the peer sub-task with the same workflow/stage context.
        let peer_dispatch = TaskDispatchRecord {
            task_id: peer_task.id.clone(),
            workflow_id: calling_dispatch
                .as_ref()
                .and_then(|d| d.workflow_id.clone()),
            stage_id: calling_dispatch.as_ref().and_then(|d| d.stage_id.clone()),
            dispatch_source: TaskDispatchSource::OwnerAssign,
            depends_on_task_ids: vec![],
            acknowledged_at: None,
            result_message_id: None,
            locked_by_user_mention: false,
            target_agent_id: peer_agent.id.clone(),
            route_mode: calling_dispatch
                .as_ref()
                .map(|d| d.route_mode.clone())
                .unwrap_or(crate::core::workflow::RequestRouteMode::DirectAgentAssign),
            narrative_stage_label: calling_dispatch
                .as_ref()
                .and_then(|d| d.narrative_stage_label.clone()),
            narrative_task_label: Some(peer_task.title.clone()),
        };
        self.storage.insert_task_dispatch(&peer_dispatch)?;

        // --- Make the calling task depend on the peer sub-task ---
        if let Some(mut dispatch) = calling_dispatch.clone() {
            if !dispatch.depends_on_task_ids.contains(&peer_task.id) {
                dispatch.depends_on_task_ids.push(peer_task.id.clone());
            }
            self.storage.insert_task_dispatch(&dispatch)?;
        }

        // --- Create a blocker record on the calling task ---
        let blocker = TaskBlockerRecord {
            id: new_id(),
            task_id: calling_task.id.clone(),
            workflow_id: calling_dispatch
                .as_ref()
                .and_then(|d| d.workflow_id.clone()),
            raised_by_agent_id: calling_agent_id.clone(),
            resolution_target: BlockerResolutionTarget::Owner,
            category: BlockerCategory::PeerInputRequired,
            summary: format!(
                "等待 {} 提供输入：{}",
                peer_agent.name,
                signal.question.chars().take(100).collect::<String>()
            ),
            details: format!(
                "子任务 ID：{}\n请求者：{}\n目标专家：{}",
                peer_task.id, calling_agent.name, peer_agent.name
            ),
            status: BlockerStatus::Open,
            created_at: now(),
            resolved_at: None,
        };
        self.storage.insert_task_blocker(&blocker)?;

        // --- Pause the calling task ---
        calling_task.status = TaskStatus::Paused;
        self.storage.update_task_card(&calling_task)?;
        emit(app, "task:status-changed", &calling_task)?;
        self.pause_lease_for_task(&calling_task.id)?;
        self.requeue_active_tool_runs(&calling_task.id)?;

        // Propagate Blocked to stage / workflow if applicable.
        if let Some(dispatch) = calling_dispatch.as_ref() {
            if let Some(stage_id) = dispatch.stage_id.as_deref() {
                let mut stage = self.storage.get_workflow_stage(stage_id)?;
                if !matches!(stage.status, StageStatus::Blocked | StageStatus::Completed) {
                    stage.status = StageStatus::Blocked;
                    self.storage.insert_workflow_stage(&stage)?;
                }
            }
            if let Some(workflow_id) = dispatch.workflow_id.as_deref() {
                let mut workflow = self.storage.get_workflow(workflow_id)?;
                if !matches!(
                    workflow.status,
                    WorkflowStatus::Blocked | WorkflowStatus::Completed
                ) {
                    workflow.status = WorkflowStatus::Blocked;
                    self.storage.insert_workflow(&workflow)?;
                }
            }
        }

        // --- Emit narrative message: calling agent requests peer input ---
        let narrative_text = format!(
            "@{} 请协助处理：{}",
            peer_agent.name, signal.question
        );
        let mut envelope = NarrativeEnvelope::new(NarrativeMessageType::BlockerRaised, narrative_text);
        envelope.blocked = Some(true);
        envelope.task_id = Some(calling_task.id.clone());
        envelope.blocker_id = Some(blocker.id.clone());
        envelope.task_title = Some(calling_task.title.clone());
        if let Some(dispatch) = calling_dispatch.as_ref() {
            envelope.workflow_id = dispatch.workflow_id.clone();
            envelope.stage_id = dispatch.stage_id.clone();
            envelope.stage_title = dispatch.narrative_stage_label.clone();
        }
        let block_message = ConversationMessage {
            id: new_id(),
            conversation_id: calling_task.work_group_id.clone(),
            work_group_id: calling_task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: calling_agent_id.clone(),
            sender_name: calling_agent.name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content: serde_json::to_string(&envelope)?,
            narrative_meta: Some(serde_json::to_string(&envelope)?),
            mentions: vec![peer_agent.id.clone()],
            task_card_id: Some(calling_task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_message(&block_message)?;
        emit(app, "chat:message-created", &block_message)?;

        self.record_audit(
            "task.peer_input_requested",
            "task_card",
            &calling_task.id,
            json!({
                "peerAgentId": peer_agent.id,
                "peerTaskId": peer_task.id,
                "blockerId": blocker.id,
                "question": signal.question,
            }),
        )?;

        // Now activate the peer sub-task.
        self.activate_task_for_agent_direct(app, &mut peer_task)?;

        Ok(PeerInputRequestReport {
            calling_task,
            peer_task,
        })
    }

    /// Activates a task for its assigned agent without requiring
    /// a `replacement_agent` parameter.
    fn activate_task_for_agent_direct<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &mut TaskCard,
    ) -> Result<()> {
        self.activate_task_for_agent(app, task, None)
    }

    /// Called when a peer sub-task completes. Resolves the peer-input blocker
    /// on the calling (parent) task and re-activates it so it can continue.
    pub(super) fn resolve_peer_input_blocker<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        peer_task: &TaskCard,
        peer_output: &str,
    ) -> Result<()> {
        // Find the parent task (the one that requested input).
        let Some(parent_id) = &peer_task.parent_id else {
            return Ok(());
        };
        // Confirm parent task has a PeerInputRequired open blocker.
        let all_blockers = self.storage.list_task_blockers()?;
        // The open PeerInputRequired blocker on the parent is the one we want.
        let open_blocker = all_blockers.into_iter().find(|b| {
            b.task_id == *parent_id
                && matches!(b.category, BlockerCategory::PeerInputRequired)
                && matches!(b.status, BlockerStatus::Open)
                && b.details.contains(&peer_task.id)
        });
        let Some(mut blocker) = open_blocker else {
            // No matching blocker — nothing to do.
            return Ok(());
        };

        // Mark the blocker resolved.
        blocker.status = BlockerStatus::Resolved;
        blocker.resolved_at = Some(now());
        self.storage.insert_task_blocker(&blocker)?;

        let mut parent_task = self.storage.get_task_card(parent_id)?;
        let peer_agent_id = peer_task.assigned_agent_id.clone().unwrap_or_default();
        let peer_agent_name = self
            .storage
            .get_agent(&peer_agent_id)
            .map(|a| a.name)
            .unwrap_or_else(|_| peer_agent_id.clone());
        let parent_agent_name = parent_task
            .assigned_agent_id
            .as_ref()
            .and_then(|id| self.storage.get_agent(id).ok())
            .map(|a| a.name)
            .unwrap_or_default();

        // Check if all remains of `depends_on_task_ids` are complete.
        let all_done = self
            .check_peer_dependencies_satisfied(&parent_task.id)?;
        if !all_done {
            // Other peer tasks still running; stay paused.
            return Ok(());
        }

        // Re-activate the parent task.
        self.restore_route_after_blocker_resolution(&parent_task)?;
        self.activate_task_for_agent(app, &mut parent_task, None)?;

        // Emit a narrative message: peer agent delivered result.
        let narrative_text = format!(
            "@{} 已完成协作请求，结果已注入上下文。",
            peer_agent_name
        );
        let mut envelope =
            NarrativeEnvelope::new(NarrativeMessageType::BlockerResolved, narrative_text);
        envelope.blocked = Some(false);
        envelope.task_id = Some(parent_task.id.clone());
        envelope.blocker_id = Some(blocker.id.clone());
        envelope.task_title = Some(parent_task.title.clone());

        let peer_dispatch = self.storage.get_task_dispatch(&peer_task.id)?;
        if let Some(dispatch) = peer_dispatch.as_ref() {
            envelope.workflow_id = dispatch.workflow_id.clone();
            envelope.stage_id = dispatch.stage_id.clone();
            envelope.stage_title = dispatch.narrative_stage_label.clone();
        }
        let resolved_message = ConversationMessage {
            id: new_id(),
            conversation_id: parent_task.work_group_id.clone(),
            work_group_id: parent_task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: peer_agent_id.clone(),
            sender_name: peer_agent_name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content: serde_json::to_string(&envelope)?,
            narrative_meta: Some(serde_json::to_string(&envelope)?),
            mentions: parent_task
                .assigned_agent_id
                .clone()
                .into_iter()
                .collect(),
            task_card_id: Some(parent_task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_message(&resolved_message)?;
        emit(app, "chat:message-created", &resolved_message)?;
        self.record_audit(
            "task.peer_input_resolved",
            "task_card",
            &parent_task.id,
            json!({
                "peerTaskId": peer_task.id,
                "blockerId": blocker.id,
                "outputLength": peer_output.len(),
                "parentAgentName": parent_agent_name,
            }),
        )?;

        self.spawn_task_execution(app.clone(), parent_task.id.clone(), None);

        Ok(())
    }

    /// Returns true when no pending peer-input sub-tasks remain for `parent_task_id`.
    fn check_peer_dependencies_satisfied(&self, parent_task_id: &str) -> Result<bool> {
        let Some(dispatch) = self.storage.get_task_dispatch(parent_task_id)? else {
            return Ok(true);
        };
        for dep_id in &dispatch.depends_on_task_ids {
            let dep = self.storage.get_task_card(dep_id)?;
            if !matches!(
                dep.status,
                TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
            ) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
