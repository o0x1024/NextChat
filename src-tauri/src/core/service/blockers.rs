use anyhow::{anyhow, Context, Result};
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::domain::{
    new_id, now, ConversationMessage, Lease, LeaseState, MessageKind, PendingUserQuestion,
    PendingUserQuestionStatus, SenderKind, TaskCard, TaskStatus, Visibility,
};
use crate::core::workflow::{
    BlockerResolutionTarget, BlockerStatus, NarrativeEnvelope, NarrativeMessageType,
    OwnerBlockerResolution, RaiseTaskBlockerInput, StageStatus, TaskBlockerRecord,
    TaskDispatchRecord, TaskDispatchSource, WorkflowStatus,
};

impl AppService {
    pub fn raise_task_blocker<R: Runtime>(
        &self,
        app: AppHandle<R>,
        task_id: &str,
        blocker: RaiseTaskBlockerInput,
    ) -> Result<TaskBlockerRecord> {
        let mut task = self.storage.get_task_card(task_id)?;
        let agent = self.storage.get_agent(&blocker.raised_by_agent_id)?;
        if task.assigned_agent_id.as_deref() != Some(agent.id.as_str()) {
            return Err(anyhow!("only the assigned agent can raise a blocker"));
        }

        let dispatch = self.storage.get_task_dispatch(task_id)?;
        let record = TaskBlockerRecord {
            id: new_id(),
            task_id: task.id.clone(),
            workflow_id: dispatch.as_ref().and_then(|item| item.workflow_id.clone()),
            raised_by_agent_id: agent.id.clone(),
            resolution_target: blocker.resolution_target.clone(),
            category: blocker.category,
            summary: blocker.summary.trim().to_string(),
            details: blocker.details.trim().to_string(),
            status: BlockerStatus::Open,
            created_at: now(),
            resolved_at: None,
        };

        self.pause_task_for_blocker(&app, &mut task, dispatch.as_ref(), &record)?;

        let text = format_blocker_text(
            &task.title,
            &record.summary,
            &record.details,
            &record.resolution_target,
        );
        let mut envelope = NarrativeEnvelope::new(NarrativeMessageType::BlockerRaised, text);
        envelope.blocked = Some(true);
        envelope.task_id = Some(task.id.clone());
        envelope.blocker_id = Some(record.id.clone());
        envelope.task_title = Some(task.title.clone());
        if let Some(dispatch) = dispatch.as_ref() {
            envelope.workflow_id = dispatch.workflow_id.clone();
            envelope.stage_id = dispatch.stage_id.clone();
            envelope.stage_title = dispatch.narrative_stage_label.clone();
        }
        let mentions = if matches!(record.resolution_target, BlockerResolutionTarget::Owner) {
            self.group_owner_for_work_group(&task.work_group_id)?
                .map(|owner| vec![owner.id])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let message = ConversationMessage {
            id: new_id(),
            conversation_id: task.work_group_id.clone(),
            work_group_id: task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content: envelope.text.clone(),
            narrative_meta: Some(serde_json::to_string(&envelope)?),
            mentions,
            task_card_id: Some(task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_task_blocker(&record)?;
        self.storage.insert_message(&message)?;
        emit(&app, "chat:message-created", &message)?;
        self.record_audit(
            "task.blocker_raised",
            "task_card",
            &task.id,
            json!({
                "blockerId": record.id,
                "resolutionTarget": record.resolution_target,
                "category": record.category,
            }),
        )?;

        if matches!(record.resolution_target, BlockerResolutionTarget::Owner) {
            self.auto_resolve_owner_blocker(app.clone(), &record.id)
                .with_context(|| format!("owner blocker decision failed for task {}", task.id))?;
        }

        self.storage.get_task_blocker(&record.id).or(Ok(record))
    }

    pub fn resolve_owner_blocker<R: Runtime>(
        &self,
        app: AppHandle<R>,
        blocker_id: &str,
        resolution: OwnerBlockerResolution,
    ) -> Result<()> {
        self.apply_owner_blocker_resolution(app, blocker_id, resolution, None)
    }

    fn auto_resolve_owner_blocker<R: Runtime>(
        &self,
        app: AppHandle<R>,
        blocker_id: &str,
    ) -> Result<()> {
        let blocker = self.storage.get_task_blocker(blocker_id)?;
        if !matches!(blocker.status, BlockerStatus::Open) {
            return Ok(());
        }
        if !matches!(blocker.resolution_target, BlockerResolutionTarget::Owner) {
            return Err(anyhow!("blocker does not target the group owner"));
        }

        let task = self.storage.get_task_card(&blocker.task_id)?;
        let work_group = self.storage.get_work_group(&task.work_group_id)?;
        let members = self
            .storage
            .list_agents()?
            .into_iter()
            .filter(|agent| work_group.member_agent_ids.contains(&agent.id))
            .collect::<Vec<_>>();
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let decision = self.build_owner_blocker_decision(
            &work_group,
            &task,
            &blocker,
            dispatch.as_ref(),
            &members,
        )?;
        self.record_audit(
            "owner.blocker_decision.applied",
            "task_blocker",
            blocker_id,
            json!({
                "taskId": task.id,
                "action": owner_blocker_action(&decision.resolution),
            }),
        )?;
        self.apply_owner_blocker_resolution(
            app,
            blocker_id,
            decision.resolution,
            decision.owner_narrative_text,
        )
    }

    fn apply_owner_blocker_resolution<R: Runtime>(
        &self,
        app: AppHandle<R>,
        blocker_id: &str,
        resolution: OwnerBlockerResolution,
        owner_narrative_text: Option<String>,
    ) -> Result<()> {
        let mut blocker = self.storage.get_task_blocker(blocker_id)?;
        if !matches!(blocker.status, BlockerStatus::Open) {
            return Err(anyhow!("blocker is not open"));
        }
        if !matches!(blocker.resolution_target, BlockerResolutionTarget::Owner) {
            return Err(anyhow!("blocker does not target the group owner"));
        }

        let mut task = self.storage.get_task_card(&blocker.task_id)?;
        let mut dispatch = self.storage.get_task_dispatch(&task.id)?;
        let work_group = self.storage.get_work_group(&task.work_group_id)?;
        let owner = self
            .group_owner_for_work_group(&task.work_group_id)?
            .context("group owner missing")?;

        match resolution {
            OwnerBlockerResolution::ProvideContext { message } => {
                blocker.status = BlockerStatus::Resolved;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
                self.restore_route_after_blocker_resolution(&task)?;
                self.activate_task_for_agent(&app, &mut task, None)?;

                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::BlockerResolved,
                    owner_narrative_text
                        .clone()
                        .unwrap_or_else(|| normalize_sentence(&message)),
                );
                envelope.blocked = Some(false);
                envelope.task_id = Some(task.id.clone());
                envelope.blocker_id = Some(blocker.id.clone());
                envelope.task_title = Some(task.title.clone());
                if let Some(dispatch) = dispatch.as_ref() {
                    envelope.workflow_id = dispatch.workflow_id.clone();
                    envelope.stage_id = dispatch.stage_id.clone();
                    envelope.stage_title = dispatch.narrative_stage_label.clone();
                }
                let mut message = self.owner_message_from_envelope(
                    &work_group.id,
                    envelope,
                    MessageKind::Status,
                )?;
                message.mentions = task.assigned_agent_id.clone().into_iter().collect();
                self.storage.insert_message(&message)?;
                emit(&app, "chat:message-created", &message)?;
                self.spawn_task_execution(app.clone(), task.id.clone(), None);
            }
            OwnerBlockerResolution::ReassignTask {
                target_agent_id,
                message,
            } => {
                let dispatch_record = dispatch
                    .as_mut()
                    .context("task dispatch missing for reassign")?;
                if dispatch_record.locked_by_user_mention {
                    return Err(anyhow!(
                        "task is locked by a user mention and cannot be reassigned"
                    ));
                }
                let target = self.storage.get_agent(&target_agent_id)?;
                if !work_group.member_agent_ids.contains(&target.id)
                    || self.is_builtin_group_owner_profile(&target)
                {
                    return Err(anyhow!("reassign target must be a non-owner group member"));
                }

                blocker.status = BlockerStatus::Resolved;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
                task.assigned_agent_id = Some(target.id.clone());
                self.storage.update_task_card(&task)?;
                emit(&app, "task:status-changed", &task)?;
                dispatch_record.target_agent_id = target.id.clone();
                dispatch_record.acknowledged_at = None;
                self.storage.insert_task_dispatch(dispatch_record)?;
                self.restore_route_after_blocker_resolution(&task)?;
                self.activate_task_for_agent(&app, &mut task, Some(target.clone()))?;

                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::OwnerDispatch,
                    owner_narrative_text.clone().unwrap_or_else(|| {
                        format!("@{} {}", target.name, normalize_sentence(&message))
                    }),
                );
                envelope.task_id = Some(task.id.clone());
                envelope.task_title = Some(task.title.clone());
                envelope.workflow_id = dispatch_record.workflow_id.clone();
                envelope.stage_id = dispatch_record.stage_id.clone();
                envelope.stage_title = dispatch_record.narrative_stage_label.clone();
                let mut message = self.owner_message_from_envelope(
                    &work_group.id,
                    envelope,
                    MessageKind::Status,
                )?;
                message.mentions = vec![target.id.clone()];
                self.storage.insert_message(&message)?;
                emit(&app, "chat:message-created", &message)?;

                if let Some(mut dispatch) = self.storage.get_task_dispatch(&task.id)? {
                    dispatch.acknowledged_at = Some(now());
                    self.storage.insert_task_dispatch(&dispatch)?;
                }
                self.spawn_task_execution(app.clone(), task.id.clone(), None);
            }
            OwnerBlockerResolution::CreateDependencyTask {
                target_agent_id,
                title,
                goal,
                message,
            } => {
                let dispatch_record = dispatch
                    .as_mut()
                    .context("task dispatch missing for dependency creation")?;
                let dependency_agent = self.storage.get_agent(&target_agent_id)?;
                if !work_group.member_agent_ids.contains(&dependency_agent.id)
                    || self.is_builtin_group_owner_profile(&dependency_agent)
                {
                    return Err(anyhow!(
                        "dependency assignee must be a non-owner group member"
                    ));
                }

                blocker.status = BlockerStatus::Resolved;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;

                let mut dependency_task = TaskCard {
                    id: new_id(),
                    parent_id: None,
                    source_message_id: task.source_message_id.clone(),
                    title: title.trim().chars().take(72).collect(),
                    normalized_goal: goal.trim().to_string(),
                    input_payload: goal.trim().to_string(),
                    priority: task.priority,
                    status: TaskStatus::Pending,
                    work_group_id: task.work_group_id.clone(),
                    created_by: owner.id.clone(),
                    assigned_agent_id: Some(dependency_agent.id.clone()),
                    output_summary: None,
                    created_at: now(),
                };
                self.storage.insert_task_card(&dependency_task)?;
                emit(&app, "task:card-created", &dependency_task)?;
                self.storage.insert_task_dispatch(&TaskDispatchRecord {
                    task_id: dependency_task.id.clone(),
                    workflow_id: dispatch_record.workflow_id.clone(),
                    stage_id: dispatch_record.stage_id.clone(),
                    dispatch_source: TaskDispatchSource::OwnerAssign,
                    depends_on_task_ids: vec![],
                    acknowledged_at: None,
                    result_message_id: None,
                    locked_by_user_mention: false,
                    target_agent_id: dependency_agent.id.clone(),
                    route_mode: dispatch_record.route_mode.clone(),
                    narrative_stage_label: dispatch_record.narrative_stage_label.clone(),
                    narrative_task_label: Some(dependency_task.title.clone()),
                })?;
                if !dispatch_record
                    .depends_on_task_ids
                    .contains(&dependency_task.id)
                {
                    dispatch_record
                        .depends_on_task_ids
                        .push(dependency_task.id.clone());
                }
                self.storage.insert_task_dispatch(dispatch_record)?;

                task.status = TaskStatus::Pending;
                self.storage.update_task_card(&task)?;
                emit(&app, "task:status-changed", &task)?;
                self.restore_route_after_blocker_resolution(&task)?;
                self.activate_task_for_agent(
                    &app,
                    &mut dependency_task,
                    Some(dependency_agent.clone()),
                )?;

                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::OwnerDispatch,
                    owner_narrative_text.clone().unwrap_or_else(|| {
                        format!(
                            "@{} {}",
                            dependency_agent.name,
                            normalize_sentence(&message)
                        )
                    }),
                );
                envelope.task_id = Some(dependency_task.id.clone());
                envelope.task_title = Some(dependency_task.title.clone());
                envelope.workflow_id = dispatch_record.workflow_id.clone();
                envelope.stage_id = dispatch_record.stage_id.clone();
                envelope.stage_title = dispatch_record.narrative_stage_label.clone();
                let mut dispatch_message = self.owner_message_from_envelope(
                    &work_group.id,
                    envelope,
                    MessageKind::Status,
                )?;
                dispatch_message.mentions = vec![dependency_agent.id.clone()];
                self.storage.insert_message(&dispatch_message)?;
                emit(&app, "chat:message-created", &dispatch_message)?;

                if let Some(mut dependency_dispatch) =
                    self.storage.get_task_dispatch(&dependency_task.id)?
                {
                    dependency_dispatch.acknowledged_at = Some(now());
                    self.storage.insert_task_dispatch(&dependency_dispatch)?;
                }
                self.spawn_task_execution(app.clone(), dependency_task.id.clone(), None);
            }
            OwnerBlockerResolution::RequestApproval {
                question,
                options,
                context,
                allow_free_form,
            } => {
                blocker.status = BlockerStatus::Resolved;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
                task.status = TaskStatus::WaitingApproval;
                self.storage.update_task_card(&task)?;
                emit(&app, "task:status-changed", &task)?;
                self.pause_lease_for_task(&task.id)?;
                self.mark_route_as_waiting_user_input(&task)?;

                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::BlockerRaised,
                    owner_narrative_text
                        .clone()
                        .unwrap_or_else(|| normalize_sentence(&question)),
                );
                envelope.blocked = Some(true);
                envelope.task_id = Some(task.id.clone());
                envelope.blocker_id = Some(blocker.id.clone());
                envelope.task_title = Some(task.title.clone());
                if let Some(dispatch) = dispatch.as_ref() {
                    envelope.workflow_id = dispatch.workflow_id.clone();
                    envelope.stage_id = dispatch.stage_id.clone();
                    envelope.stage_title = dispatch.narrative_stage_label.clone();
                }
                let approval_message = self.owner_message_from_envelope(
                    &work_group.id,
                    envelope,
                    MessageKind::Approval,
                )?;
                self.storage.insert_message(&approval_message)?;
                emit(&app, "chat:message-created", &approval_message)?;
                let pending_question = PendingUserQuestion {
                    id: new_id(),
                    work_group_id: work_group.id.clone(),
                    task_card_id: task.id.clone(),
                    agent_id: owner.id.clone(),
                    tool_run_id: None,
                    question: question.clone(),
                    options: if options.is_empty() {
                        vec!["批准".into(), "拒绝".into()]
                    } else {
                        options.clone()
                    },
                    context: context.clone(),
                    allow_free_form: allow_free_form.unwrap_or(false),
                    asked_message_id: approval_message.id.clone(),
                    answer_message_id: None,
                    status: PendingUserQuestionStatus::Pending,
                    created_at: now(),
                    answered_at: None,
                };
                self.storage
                    .insert_pending_user_question(&pending_question)?;
                emit(&app, "pending-user-question:updated", &pending_question)?;
            }
            OwnerBlockerResolution::AskUser {
                question,
                options,
                context,
                allow_free_form,
            } => {
                blocker.status = BlockerStatus::Resolved;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
                task.status = TaskStatus::WaitingUserInput;
                self.storage.update_task_card(&task)?;
                emit(&app, "task:status-changed", &task)?;
                self.pause_lease_for_task(&task.id)?;
                self.mark_route_as_waiting_user_input(&task)?;

                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::BlockerRaised,
                    owner_narrative_text
                        .clone()
                        .unwrap_or_else(|| normalize_sentence(&question)),
                );
                envelope.blocked = Some(true);
                envelope.task_id = Some(task.id.clone());
                envelope.blocker_id = Some(blocker.id.clone());
                envelope.task_title = Some(task.title.clone());
                if let Some(dispatch) = dispatch.as_ref() {
                    envelope.workflow_id = dispatch.workflow_id.clone();
                    envelope.stage_id = dispatch.stage_id.clone();
                    envelope.stage_title = dispatch.narrative_stage_label.clone();
                }
                let question_message = self.owner_message_from_envelope(
                    &work_group.id,
                    envelope,
                    MessageKind::Status,
                )?;
                self.storage.insert_message(&question_message)?;
                emit(&app, "chat:message-created", &question_message)?;
                let pending_question = PendingUserQuestion {
                    id: new_id(),
                    work_group_id: work_group.id.clone(),
                    task_card_id: task.id.clone(),
                    agent_id: owner.id.clone(),
                    tool_run_id: None,
                    question: question.clone(),
                    options: options.clone(),
                    context: context.clone(),
                    allow_free_form: allow_free_form.unwrap_or(true),
                    asked_message_id: question_message.id.clone(),
                    answer_message_id: None,
                    status: PendingUserQuestionStatus::Pending,
                    created_at: now(),
                    answered_at: None,
                };
                self.storage
                    .insert_pending_user_question(&pending_question)?;
                emit(&app, "pending-user-question:updated", &pending_question)?;
            }
            OwnerBlockerResolution::PauseTask { message } => {
                blocker.status = BlockerStatus::Cancelled;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
                task.status = TaskStatus::Paused;
                self.storage.update_task_card(&task)?;
                emit(&app, "task:status-changed", &task)?;
                self.pause_lease_for_task(&task.id)?;

                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::BlockerResolved,
                    owner_narrative_text
                        .clone()
                        .unwrap_or_else(|| normalize_sentence(&message)),
                );
                envelope.blocked = Some(false);
                envelope.task_id = Some(task.id.clone());
                envelope.blocker_id = Some(blocker.id.clone());
                envelope.task_title = Some(task.title.clone());
                if let Some(dispatch) = dispatch.as_ref() {
                    envelope.workflow_id = dispatch.workflow_id.clone();
                    envelope.stage_id = dispatch.stage_id.clone();
                    envelope.stage_title = dispatch.narrative_stage_label.clone();
                }
                let message = self.owner_message_from_envelope(
                    &work_group.id,
                    envelope,
                    MessageKind::Status,
                )?;
                self.storage.insert_message(&message)?;
                emit(&app, "chat:message-created", &message)?;
            }
        }

        self.record_audit(
            "task.blocker_resolved",
            "task_card",
            &task.id,
            json!({ "blockerId": blocker_id }),
        )?;
        Ok(())
    }

    pub(super) fn restore_route_after_blocker_resolution(&self, task: &TaskCard) -> Result<()> {
        let Some(dispatch) = self.storage.get_task_dispatch(&task.id)? else {
            return Ok(());
        };
        if let Some(stage_id) = dispatch.stage_id.as_deref() {
            let mut stage = self.storage.get_workflow_stage(stage_id)?;
            if matches!(
                stage.status,
                StageStatus::Blocked | StageStatus::Pending | StageStatus::Ready
            ) {
                stage.status = StageStatus::Running;
                self.storage.insert_workflow_stage(&stage)?;
            }
        }
        if let Some(workflow_id) = dispatch.workflow_id.as_deref() {
            let mut workflow = self.storage.get_workflow(workflow_id)?;
            if matches!(
                workflow.status,
                WorkflowStatus::Blocked | WorkflowStatus::NeedsUserInput
            ) {
                workflow.status = WorkflowStatus::Running;
                self.storage.insert_workflow(&workflow)?;
            }
        }
        Ok(())
    }

    fn pause_task_for_blocker<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &mut TaskCard,
        dispatch: Option<&crate::core::workflow::TaskDispatchRecord>,
        blocker: &TaskBlockerRecord,
    ) -> Result<()> {
        task.status = TaskStatus::Paused;
        self.storage.update_task_card(task)?;
        emit(app, "task:status-changed", task)?;
        self.pause_lease_for_task(&task.id)?;
        self.requeue_active_tool_runs(&task.id)?;

        if let Some(dispatch) = dispatch {
            if let Some(stage_id) = dispatch.stage_id.as_deref() {
                let mut stage = self.storage.get_workflow_stage(stage_id)?;
                stage.status = StageStatus::Blocked;
                self.storage.insert_workflow_stage(&stage)?;
            }
            if let Some(workflow_id) = dispatch.workflow_id.as_deref() {
                let mut workflow = self.storage.get_workflow(workflow_id)?;
                workflow.status =
                    if matches!(blocker.resolution_target, BlockerResolutionTarget::User) {
                        WorkflowStatus::NeedsUserInput
                    } else {
                        WorkflowStatus::Blocked
                    };
                self.storage.insert_workflow(&workflow)?;
            }
        }
        Ok(())
    }

    pub(super) fn activate_task_for_agent<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &mut TaskCard,
        replacement_agent: Option<crate::core::domain::AgentProfile>,
    ) -> Result<()> {
        let agent = if let Some(agent) = replacement_agent {
            agent
        } else {
            let agent_id = task
                .assigned_agent_id
                .clone()
                .context("task is missing assignee")?;
            self.storage.get_agent(&agent_id)?
        };
        task.assigned_agent_id = Some(agent.id.clone());
        task.status = TaskStatus::Leased;
        self.storage.update_task_card(task)?;
        emit(app, "task:status-changed", task)?;

        let mut lease = self.storage.get_lease_by_task(&task.id)?.unwrap_or(Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: agent.id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        });
        lease.owner_agent_id = agent.id.clone();
        lease.state = LeaseState::Active;
        lease.released_at = None;
        lease.preempt_requested_at = None;
        if lease.granted_at.trim().is_empty() {
            lease.granted_at = now();
        }
        self.storage.update_lease(&lease)?;
        emit(app, "lease:granted", &lease)?;
        Ok(())
    }

    pub(super) fn pause_lease_for_task(&self, task_id: &str) -> Result<()> {
        if let Some(mut lease) = self.storage.get_lease_by_task(task_id)? {
            lease.state = LeaseState::Paused;
            lease.preempt_requested_at = None;
            self.storage.update_lease(&lease)?;
        }
        Ok(())
    }

    pub(super) fn requeue_active_tool_runs(&self, task_id: &str) -> Result<()> {
        for mut tool_run in self
            .storage
            .list_tool_runs()?
            .into_iter()
            .filter(|run| run.task_card_id == task_id)
        {
            if matches!(
                tool_run.state,
                crate::core::domain::ToolRunState::Completed
                    | crate::core::domain::ToolRunState::Cancelled
            ) {
                continue;
            }
            tool_run.state = crate::core::domain::ToolRunState::Queued;
            tool_run.started_at = None;
            tool_run.finished_at = None;
            self.storage.insert_tool_run(&tool_run)?;
        }
        Ok(())
    }

    fn mark_route_as_waiting_user_input(&self, task: &TaskCard) -> Result<()> {
        let Some(dispatch) = self.storage.get_task_dispatch(&task.id)? else {
            return Ok(());
        };
        if let Some(stage_id) = dispatch.stage_id.as_deref() {
            let mut stage = self.storage.get_workflow_stage(stage_id)?;
            stage.status = StageStatus::Blocked;
            self.storage.insert_workflow_stage(&stage)?;
        }
        if let Some(workflow_id) = dispatch.workflow_id.as_deref() {
            let mut workflow = self.storage.get_workflow(workflow_id)?;
            workflow.status = WorkflowStatus::NeedsUserInput;
            self.storage.insert_workflow(&workflow)?;
        }
        Ok(())
    }
}

fn format_blocker_text(
    task_title: &str,
    summary: &str,
    details: &str,
    target: &BlockerResolutionTarget,
) -> String {
    let prefix = match target {
        BlockerResolutionTarget::Owner => "@群主",
        BlockerResolutionTarget::User => "@用户",
    };
    if details.trim().is_empty() {
        format!(
            "{prefix} 当前{task_title}被阻塞，{}。",
            normalize_sentence(summary)
        )
    } else {
        format!(
            "{prefix} 当前{task_title}被阻塞，{} {}",
            normalize_sentence(summary),
            normalize_sentence(details)
        )
    }
}

fn normalize_sentence(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with(['。', '.', '!', '！', '?', '？']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}。")
    }
}

fn owner_blocker_action(resolution: &OwnerBlockerResolution) -> &'static str {
    match resolution {
        OwnerBlockerResolution::ProvideContext { .. } => "provide_context",
        OwnerBlockerResolution::ReassignTask { .. } => "reassign_task",
        OwnerBlockerResolution::CreateDependencyTask { .. } => "create_dependency_task",
        OwnerBlockerResolution::RequestApproval { .. } => "request_approval",
        OwnerBlockerResolution::AskUser { .. } => "ask_user",
        OwnerBlockerResolution::PauseTask { .. } => "pause_task",
    }
}
