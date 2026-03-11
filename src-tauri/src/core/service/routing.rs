use anyhow::{Context, Result};
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{
    collect_allowed_tools, emit, narrative_messages::render_question_text, scored_candidates,
    should_skip_implicit_todowrite_fallback, AppService,
};
use crate::core::domain::{
    new_id, now, AgentProfile, ClaimScorer, ConversationMessage, MessageKind,
    SendHumanMessageInput, SenderKind, TaskCard, TaskStatus, Visibility, WorkGroup,
};
use crate::core::workflow::{
    BlockerCategory, BlockerResolutionTarget, BlockerStatus, NarrativeEnvelope,
    NarrativeMessageType, RequestRouteMode, StageStatus, TaskBlockerRecord, TaskDispatchRecord,
    TaskDispatchSource, WorkflowPlan, WorkflowStatus,
};

const OWNER_ROUTING_KEYWORDS: &[&str] = &["安排", "规划", "组织", "分配", "推进"];
const DIRECT_QUESTION_KEYWORDS: &[&str] = &[
    "什么",
    "为什么",
    "如何",
    "怎么",
    "区别",
    "建议",
    "解释",
    "介绍",
    "是否",
    "吗",
    "?",
    "？",
    "how",
    "what",
    "why",
    "explain",
    "suggest",
];
const PROJECT_KEYWORDS: &[&str] = &[
    "开发", "实现", "系统", "方案", "规划", "搭建", "上线", "改造",
];

impl AppService {
    pub fn send_human_message<R: Runtime>(
        &self,
        app: AppHandle<R>,
        input: SendHumanMessageInput,
    ) -> Result<ConversationMessage> {
        let work_group = self.storage.get_work_group(&input.work_group_id)?;
        let agents = self.storage.list_agents()?;
        let members: Vec<AgentProfile> = agents
            .into_iter()
            .filter(|agent| work_group.member_agent_ids.contains(&agent.id))
            .collect();

        let mentions =
            crate::core::coordinator::Coordinator::extract_mentions(&input.content, &members);
        let human_message = ConversationMessage {
            id: new_id(),
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::Human,
            sender_id: "human".into(),
            sender_name: "Human".into(),
            kind: MessageKind::Text,
            visibility: Visibility::Main,
            content: input.content.clone(),
            mentions,
            task_card_id: None,
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_message(&human_message)?;
        self.remember_human_directive(&app, &human_message)?;
        emit(&app, "chat:message-created", &human_message)?;

        if self
            .try_answer_pending_user_question(&app, &human_message)?
            .is_some()
        {
            return Ok(human_message);
        }

        self.preempt_active_leases(&app, &work_group.id)?;

        match self.classify_request_route_mode(&work_group, &human_message, &members)? {
            RequestRouteMode::DirectAnswer => {
                self.dispatch_direct_answer(&app, &work_group, &human_message, &members)?;
            }
            RequestRouteMode::DirectAgentAssign => {
                let target_agent_ids =
                    self.resolve_direct_target_agents(&work_group, &human_message, &members)?;
                if target_agent_ids.is_empty() {
                    return Ok(human_message);
                }
                self.dispatch_direct_agent_task(app, &human_message, &members, &target_agent_ids)?;
            }
            RequestRouteMode::OwnerOrchestrated => {
                if should_use_owner_workflow(&human_message.content) {
                    let plan =
                        self.build_owner_workflow_plan(&work_group, &human_message, &members)?;
                    self.dispatch_owner_workflow(app, plan)?;
                } else {
                    self.dispatch_legacy_claim_flow(app, &work_group, &human_message, &members)?;
                }
            }
        }

        Ok(human_message)
    }

    pub(super) fn classify_request_route_mode(
        &self,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        members: &[AgentProfile],
    ) -> Result<RequestRouteMode> {
        let owner = self.group_owner_for_work_group(&work_group.id)?;
        let has_owner_mention = owner
            .as_ref()
            .is_some_and(|item| source_message.mentions.contains(&item.id))
            || source_message.content.contains("@群主");
        let non_owner_mentions = source_message
            .mentions
            .iter()
            .filter(|agent_id| owner.as_ref().map(|item| &item.id) != Some(*agent_id))
            .count();
        let lowered = source_message.content.to_lowercase();
        let asks_owner = OWNER_ROUTING_KEYWORDS
            .iter()
            .any(|keyword| source_message.content.contains(keyword));
        let looks_like_question = DIRECT_QUESTION_KEYWORDS
            .iter()
            .any(|keyword| source_message.content.contains(keyword) || lowered.contains(keyword));
        let looks_like_project = PROJECT_KEYWORDS
            .iter()
            .any(|keyword| source_message.content.contains(keyword) || lowered.contains(keyword));

        if non_owner_mentions > 0 {
            if has_owner_mention || asks_owner {
                return Ok(RequestRouteMode::OwnerOrchestrated);
            }
            return Ok(RequestRouteMode::DirectAgentAssign);
        }

        let active_members = members
            .iter()
            .filter(|agent| !self.is_builtin_group_owner_profile(agent))
            .count();
        if looks_like_question && !looks_like_project && active_members > 0 {
            return Ok(RequestRouteMode::DirectAnswer);
        }

        Ok(RequestRouteMode::OwnerOrchestrated)
    }

    pub(super) fn build_task_narrative_content(
        &self,
        task: &TaskCard,
        narrative_type: NarrativeMessageType,
        text: impl Into<String>,
    ) -> Result<String> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let mut envelope = NarrativeEnvelope::new(narrative_type, text);
        envelope.task_id = Some(task.id.clone());
        if let Some(dispatch) = dispatch {
            envelope.workflow_id = dispatch.workflow_id;
            envelope.stage_id = dispatch.stage_id;
            envelope.stage_title = dispatch.narrative_stage_label;
            envelope.task_title = dispatch
                .narrative_task_label
                .or_else(|| Some(task.title.clone()));
        }
        serde_json::to_string(&envelope).map_err(Into::into)
    }

    pub(super) fn build_pending_user_question_message(
        &self,
        task: &TaskCard,
        agent: &AgentProfile,
        question: &str,
        options: &[String],
        context: Option<&str>,
    ) -> Result<(ConversationMessage, TaskBlockerRecord)> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let resolution_target = if dispatch
            .as_ref()
            .is_some_and(|item| item.route_mode == RequestRouteMode::OwnerOrchestrated)
        {
            BlockerResolutionTarget::Owner
        } else {
            BlockerResolutionTarget::User
        };
        let text = render_question_text(question, options, context, &resolution_target);
        let blocker = TaskBlockerRecord {
            id: new_id(),
            task_id: task.id.clone(),
            workflow_id: dispatch.as_ref().and_then(|item| item.workflow_id.clone()),
            raised_by_agent_id: agent.id.clone(),
            resolution_target: resolution_target.clone(),
            category: BlockerCategory::NeedUserDecision,
            summary: question.to_string(),
            details: context.unwrap_or_default().to_string(),
            status: BlockerStatus::Open,
            created_at: now(),
            resolved_at: None,
        };
        let content = if dispatch.is_some() {
            let mut envelope = NarrativeEnvelope::new(NarrativeMessageType::BlockerRaised, text);
            envelope.blocked = Some(true);
            envelope.task_id = Some(task.id.clone());
            if let Some(dispatch) = dispatch.as_ref() {
                envelope.workflow_id = dispatch.workflow_id.clone();
                envelope.stage_id = dispatch.stage_id.clone();
                envelope.stage_title = dispatch.narrative_stage_label.clone();
                envelope.task_title = dispatch
                    .narrative_task_label
                    .clone()
                    .or_else(|| Some(task.title.clone()));
            }
            serde_json::to_string(&envelope)?
        } else {
            text
        };
        let mut message = ConversationMessage {
            id: new_id(),
            conversation_id: task.work_group_id.clone(),
            work_group_id: task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content,
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        if matches!(resolution_target, BlockerResolutionTarget::Owner) {
            self.assign_group_owner_sender(&mut message)?;
        }
        Ok((message, blocker))
    }

    pub(super) fn build_pending_user_question_resume_message(
        &self,
        task: &TaskCard,
        question: &str,
    ) -> Result<ConversationMessage> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let content = if dispatch.is_some() {
            let mut envelope = NarrativeEnvelope::new(
                NarrativeMessageType::BlockerResolved,
                format!("已收到你的回答，继续处理：{question}"),
            );
            envelope.task_id = Some(task.id.clone());
            envelope.blocked = Some(false);
            if let Some(dispatch) = dispatch.as_ref() {
                envelope.workflow_id = dispatch.workflow_id.clone();
                envelope.stage_id = dispatch.stage_id.clone();
                envelope.stage_title = dispatch.narrative_stage_label.clone();
                envelope.task_title = dispatch
                    .narrative_task_label
                    .clone()
                    .or_else(|| Some(task.title.clone()));
            }
            serde_json::to_string(&envelope)?
        } else {
            format!("已收到你的回答，继续处理：{question}")
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

    pub(super) fn try_answer_pending_user_question<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        human_message: &ConversationMessage,
    ) -> Result<Option<()>> {
        let Some(mut pending) = self
            .storage
            .latest_pending_user_question_for_group(&human_message.work_group_id)?
        else {
            return Ok(None);
        };

        pending.status = crate::core::domain::PendingUserQuestionStatus::Answered;
        pending.answer_message_id = Some(human_message.id.clone());
        pending.answered_at = Some(now());
        self.storage.insert_pending_user_question(&pending)?;
        emit(app, "pending-user-question:updated", &pending)?;

        let mut task = self.storage.get_task_card(&pending.task_card_id)?;
        task.status = TaskStatus::Leased;
        self.storage.update_task_card(&task)?;
        emit(app, "task:status-changed", &task)?;

        if let Some(mut lease) = self.storage.get_lease_by_task(&pending.task_card_id)? {
            lease.state = crate::core::domain::LeaseState::Active;
            lease.preempt_requested_at = None;
            self.storage.update_lease(&lease)?;
            emit(app, "lease:granted", &lease)?;
        }

        if let Some(mut blocker) = self.storage.latest_open_blocker_for_task(&task.id)? {
            blocker.status = BlockerStatus::Resolved;
            blocker.resolved_at = Some(now());
            self.storage.insert_task_blocker(&blocker)?;
        }
        self.restore_route_after_blocker_resolution(&task)?;

        let resume_message =
            self.build_pending_user_question_resume_message(&task, &pending.question)?;
        self.storage.insert_message(&resume_message)?;
        emit(app, "chat:message-created", &resume_message)?;

        self.record_audit(
            "task.user_input_received",
            "task_card",
            &task.id,
            json!({
                "questionId": pending.id,
                "answerMessageId": human_message.id,
            }),
        )?;

        self.spawn_task_execution(app.clone(), task.id.clone(), pending.tool_run_id.clone());
        Ok(Some(()))
    }

    pub(super) fn handle_narrative_task_completion<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &TaskCard,
        result_message_id: &str,
    ) -> Result<()> {
        let Some(mut dispatch) = self.storage.get_task_dispatch(&task.id)? else {
            return Ok(());
        };
        dispatch.result_message_id = Some(result_message_id.to_string());
        self.storage.insert_task_dispatch(&dispatch)?;

        if dispatch.route_mode == RequestRouteMode::DirectAgentAssign {
            if let Some(mut blocker) = self.storage.latest_open_blocker_for_task(&task.id)? {
                blocker.status = BlockerStatus::Resolved;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
            }
            return Ok(());
        }

        let Some(workflow_id) = dispatch.workflow_id.clone() else {
            return Ok(());
        };
        let Some(stage_id) = dispatch.stage_id.clone() else {
            return Ok(());
        };

        let stage_dispatches = self.storage.list_stage_task_dispatches(&stage_id)?;
        let stage_tasks = stage_dispatches
            .iter()
            .map(|item| self.storage.get_task_card(&item.task_id))
            .collect::<Result<Vec<_>>>()?;

        let next_ready = stage_dispatches
            .iter()
            .find(|item| {
                stage_tasks
                    .iter()
                    .find(|task_item| task_item.id == item.task_id)
                    .is_some_and(|task_item| {
                        matches!(task_item.status, TaskStatus::Pending)
                            && item.depends_on_task_ids.iter().all(|dependency| {
                                stage_tasks.iter().any(|candidate| {
                                    candidate.id == *dependency
                                        && matches!(candidate.status, TaskStatus::Completed)
                                })
                            })
                    })
            })
            .map(|item| item.task_id.clone());

        if let Some(task_id) = next_ready {
            self.activate_stage_tasks(app, &workflow_id, &stage_id, &[task_id], false)?;
            return Ok(());
        }

        let all_completed = stage_tasks
            .iter()
            .all(|task_item| matches!(task_item.status, TaskStatus::Completed));
        if !all_completed {
            return Ok(());
        }

        let mut stage = self.storage.get_workflow_stage(&stage_id)?;
        stage.status = StageStatus::Completed;
        self.storage.insert_workflow_stage(&stage)?;
        self.record_stage_checkpoint(
            &stage.id,
            crate::core::workflow::WorkflowCheckpointStatus::StageCompleted,
        )?;

        let stages = self.storage.list_workflow_stages(&workflow_id)?;
        let next_stage = stages
            .iter()
            .find(|candidate| candidate.order_index > stage.order_index)
            .cloned();

        if let Some(next_stage) = next_stage {
            let mut workflow = self.storage.get_workflow(&workflow_id)?;
            workflow.current_stage_id = Some(next_stage.id.clone());
            workflow.status = WorkflowStatus::Running;
            self.storage.insert_workflow(&workflow)?;
            self.record_workflow_checkpoint(
                &workflow.id,
                crate::core::workflow::WorkflowCheckpointStatus::WorkflowRunning,
            )?;
            let mut transition = NarrativeEnvelope::new(
                NarrativeMessageType::OwnerStageTransition,
                self.build_owner_stage_transition_text(&workflow, &stage, &next_stage)?,
            );
            transition.workflow_id = Some(workflow.id.clone());
            transition.stage_id = Some(next_stage.id.clone());
            transition.stage_title = Some(next_stage.title.clone());
            let message = self.owner_message_from_envelope(
                &workflow.work_group_id,
                transition,
                MessageKind::Status,
            )?;
            self.storage.insert_message(&message)?;
            emit(app, "chat:message-created", &message)?;
            self.activate_next_stage(app, &workflow_id, &next_stage.id)?;
            return Ok(());
        }

        let mut workflow = self.storage.get_workflow(&workflow_id)?;
        workflow.status = WorkflowStatus::Completed;
        self.storage.insert_workflow(&workflow)?;
        self.record_workflow_checkpoint(
            &workflow.id,
            crate::core::workflow::WorkflowCheckpointStatus::WorkflowCompleted,
        )?;
        let mut summary = NarrativeEnvelope::new(
            NarrativeMessageType::OwnerSummary,
            self.build_owner_workflow_summary_text(&workflow)?,
        );
        summary.workflow_id = Some(workflow.id.clone());
        let message = self.owner_message_from_envelope(
            &workflow.work_group_id,
            summary,
            MessageKind::Summary,
        )?;
        self.storage.insert_message(&message)?;
        emit(app, "chat:message-created", &message)?;
        Ok(())
    }

    fn resolve_direct_target_agents(
        &self,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        members: &[AgentProfile],
    ) -> Result<Vec<String>> {
        let owner = self.group_owner_for_work_group(&work_group.id)?;
        Ok(source_message
            .mentions
            .iter()
            .filter(|agent_id| owner.as_ref().map(|item| &item.id) != Some(*agent_id))
            .filter(|agent_id| members.iter().any(|member| &member.id == *agent_id))
            .cloned()
            .collect())
    }

    fn dispatch_legacy_claim_flow<R: Runtime>(
        &self,
        app: AppHandle<R>,
        work_group: &WorkGroup,
        human_message: &ConversationMessage,
        members: &[AgentProfile],
    ) -> Result<()> {
        let routing_members =
            self.routing_members_for_message(&work_group.id, members, &human_message.mentions)?;
        let scored_members = scored_candidates(&self.tool_runtime, &routing_members);
        let mut requested_tool = self.tool_runtime.select_tool_for_text(
            &human_message.content,
            &collect_allowed_tools(&self.tool_runtime, &routing_members),
        );
        let active_loads = self
            .storage
            .counts_for_agents(&work_group.member_agent_ids)?;

        let task_card = TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: human_message.id.clone(),
            title: crate::core::coordinator::Coordinator::build_task_title(&human_message.content),
            normalized_goal: human_message.content.trim().to_string(),
            input_payload: human_message.content.clone(),
            priority: 100,
            status: TaskStatus::Bidding,
            work_group_id: work_group.id.clone(),
            created_by: "human".into(),
            assigned_agent_id: None,
            created_at: now(),
        };

        let mut claim_plan = self.coordinator.score(crate::core::domain::ClaimContext {
            task_card,
            work_group: work_group.clone(),
            candidates: scored_members,
            content: human_message.content.clone(),
            mentioned_agent_ids: human_message.mentions.clone(),
            active_loads,
            requested_tool: requested_tool.clone(),
        })?;
        let owner_decision = self.build_owner_task_assignment_decision(
            work_group,
            human_message,
            &routing_members,
            &claim_plan.task_card,
            &claim_plan.bids,
            claim_plan
                .lease
                .as_ref()
                .map(|lease| lease.owner_agent_id.as_str()),
        )?;
        let explicitly_mentioned_agents = human_message
            .mentions
            .iter()
            .filter(|agent_id| {
                routing_members
                    .iter()
                    .any(|candidate| candidate.id == agent_id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        if let Some(chosen_agent_id) = owner_decision
            .assignee_agent_id
            .clone()
            .filter(|agent_id| {
                routing_members
                    .iter()
                    .any(|candidate| candidate.id == *agent_id)
            })
            .filter(|agent_id| {
                explicitly_mentioned_agents.is_empty()
                    || explicitly_mentioned_agents.contains(agent_id)
            })
        {
            match claim_plan.lease.as_mut() {
                Some(lease) => lease.owner_agent_id = chosen_agent_id.clone(),
                None => {
                    claim_plan.lease = Some(crate::core::domain::Lease {
                        id: new_id(),
                        task_card_id: claim_plan.task_card.id.clone(),
                        owner_agent_id: chosen_agent_id.clone(),
                        state: crate::core::domain::LeaseState::Active,
                        granted_at: now(),
                        expires_at: None,
                        preempt_requested_at: None,
                        released_at: None,
                    });
                }
            }
            claim_plan.task_card.assigned_agent_id = Some(chosen_agent_id);
            claim_plan.task_card.status = TaskStatus::Leased;
        } else {
            claim_plan.lease = None;
            claim_plan.task_card.assigned_agent_id = None;
            claim_plan.task_card.status = TaskStatus::Pending;
        }

        let mut denied_request: Option<(AgentProfile, crate::core::domain::ToolManifest, String)> =
            None;
        let mut requested_tool_requires_approval = false;
        if let Some(lease) = claim_plan.lease.as_ref() {
            if let Some(agent) = routing_members
                .iter()
                .find(|candidate| candidate.id == lease.owner_agent_id)
                .cloned()
            {
                if let Some(tool) = requested_tool.clone() {
                    if should_skip_implicit_todowrite_fallback(&human_message.content, &tool) {
                        requested_tool = None;
                    }
                }
                if let Some(tool) = requested_tool.clone() {
                    let decision = self.tool_runtime.authorize_tool_call(
                        &agent,
                        &tool,
                        &claim_plan.task_card.input_payload,
                        &work_group.working_directory,
                    )?;
                    if !decision.allowed {
                        claim_plan.task_card.status = TaskStatus::NeedsReview;
                        denied_request = Some((
                            agent,
                            tool,
                            decision
                                .reason
                                .unwrap_or_else(|| "tool access rejected".to_string()),
                        ));
                        claim_plan.lease = None;
                    } else if decision.approval_required {
                        claim_plan.task_card.status = TaskStatus::WaitingApproval;
                        requested_tool_requires_approval = true;
                    }
                }
            }
        }

        self.storage.insert_task_card(&claim_plan.task_card)?;
        emit(&app, "task:card-created", &claim_plan.task_card)?;
        self.record_audit(
            "task.created",
            "task_card",
            &claim_plan.task_card.id,
            json!({
                "sourceMessageId": human_message.id,
                "title": claim_plan.task_card.title,
            }),
        )?;

        for bid in &claim_plan.bids {
            self.storage.insert_claim_bid(bid)?;
            emit(&app, "claim:bid-submitted", bid)?;
        }

        let owner_ack = self.owner_message_from_envelope(
            &work_group.id,
            NarrativeEnvelope::new(
                NarrativeMessageType::OwnerAck,
                owner_decision
                    .owner_ack_text
                    .clone()
                    .context("owner assignment missing owner ack text")?,
            ),
            MessageKind::Status,
        )?;
        self.storage.insert_message(&owner_ack)?;
        emit(&app, "chat:message-created", &owner_ack)?;

        if let Some(lease) = claim_plan.lease.as_ref() {
            let dispatch_agent = routing_members
                .iter()
                .find(|candidate| candidate.id == lease.owner_agent_id)
                .cloned()
                .context("legacy dispatch agent missing")?;
            self.storage.insert_task_dispatch(&TaskDispatchRecord {
                task_id: claim_plan.task_card.id.clone(),
                workflow_id: None,
                stage_id: None,
                dispatch_source: TaskDispatchSource::OwnerAssign,
                depends_on_task_ids: vec![],
                acknowledged_at: None,
                result_message_id: None,
                locked_by_user_mention: !human_message.mentions.is_empty(),
                target_agent_id: dispatch_agent.id.clone(),
                route_mode: RequestRouteMode::OwnerOrchestrated,
                narrative_stage_label: None,
                narrative_task_label: Some(claim_plan.task_card.title.clone()),
            })?;
            let mut dispatch_envelope = NarrativeEnvelope::new(
                NarrativeMessageType::OwnerDispatch,
                owner_decision
                    .owner_dispatch_text
                    .clone()
                    .context("owner assignment missing owner dispatch text")?,
            );
            dispatch_envelope.task_id = Some(claim_plan.task_card.id.clone());
            dispatch_envelope.task_title = Some(claim_plan.task_card.title.clone());
            let dispatch_message = self.owner_message_from_envelope(
                &work_group.id,
                dispatch_envelope,
                MessageKind::Status,
            )?;
            self.storage.insert_message(&dispatch_message)?;
            emit(&app, "chat:message-created", &dispatch_message)?;
        } else if denied_request.is_none() {
            let mut blocker = NarrativeEnvelope::new(
                NarrativeMessageType::BlockerRaised,
                owner_decision
                    .owner_blocker_text
                    .clone()
                    .context("owner assignment missing owner blocker text")?,
            );
            blocker.blocked = Some(true);
            blocker.task_id = Some(claim_plan.task_card.id.clone());
            blocker.task_title = Some(claim_plan.task_card.title.clone());
            let blocker_message =
                self.owner_message_from_envelope(&work_group.id, blocker, MessageKind::Status)?;
            self.storage.insert_message(&blocker_message)?;
            emit(&app, "chat:message-created", &blocker_message)?;
        }

        for message in &claim_plan.coordinator_messages {
            let mut message = message.clone();
            message.visibility = Visibility::Backstage;
            self.assign_group_owner_sender(&mut message)?;
            self.storage.insert_message(&message)?;
            emit(&app, "chat:message-created", &message)?;
        }

        if let Some(ref lease) = claim_plan.lease {
            self.storage.insert_lease(lease)?;
            emit(&app, "lease:granted", lease)?;
            self.record_audit(
                "lease.granted",
                "lease",
                &lease.id,
                json!({ "taskCardId": lease.task_card_id, "agentId": lease.owner_agent_id }),
            )?;
            if let Some(agent) = routing_members
                .iter()
                .find(|candidate| candidate.id == lease.owner_agent_id)
            {
                let ack = ConversationMessage {
                    id: new_id(),
                    conversation_id: work_group.id.clone(),
                    work_group_id: work_group.id.clone(),
                    sender_kind: SenderKind::Agent,
                    sender_id: agent.id.clone(),
                    sender_name: agent.name.clone(),
                    kind: MessageKind::Status,
                    visibility: Visibility::Main,
                    content: self.build_task_narrative_content(
                        &claim_plan.task_card,
                        NarrativeMessageType::AgentAck,
                        self.build_agent_ack_text(&claim_plan.task_card, agent)?,
                    )?,
                    mentions: vec![],
                    task_card_id: Some(claim_plan.task_card.id.clone()),
                    execution_mode: None,
                    created_at: now(),
                };
                self.storage.insert_message(&ack)?;
                emit(&app, "chat:message-created", &ack)?;
                if let Some(mut dispatch) =
                    self.storage.get_task_dispatch(&claim_plan.task_card.id)?
                {
                    dispatch.acknowledged_at = Some(now());
                    self.storage.insert_task_dispatch(&dispatch)?;
                }
            }
        }

        if let Some((agent, tool, reason)) = denied_request {
            self.handle_permission_denial(
                &app,
                &work_group.id,
                &mut claim_plan.task_card,
                None,
                &agent,
                &tool,
                &reason,
            )?;
        } else if let Some(tool) = requested_tool {
            if let Some(ref lease) = claim_plan.lease {
                let tool_run = crate::core::domain::ToolRun {
                    id: new_id(),
                    tool_id: tool.id.clone(),
                    task_card_id: claim_plan.task_card.id.clone(),
                    agent_id: lease.owner_agent_id.clone(),
                    state: if requested_tool_requires_approval {
                        crate::core::domain::ToolRunState::PendingApproval
                    } else {
                        crate::core::domain::ToolRunState::Queued
                    },
                    approval_required: requested_tool_requires_approval,
                    started_at: None,
                    finished_at: None,
                    result_ref: None,
                };
                self.storage.insert_tool_run(&tool_run)?;
                if tool_run.approval_required {
                    let mut approval_envelope = NarrativeEnvelope::new(
                        NarrativeMessageType::BlockerRaised,
                        format!("当前任务需要审批后才能继续执行：{}。", tool.name),
                    );
                    approval_envelope.blocked = Some(true);
                    approval_envelope.task_id = Some(claim_plan.task_card.id.clone());
                    approval_envelope.task_title = Some(claim_plan.task_card.title.clone());
                    let mut approval_message = self.owner_message_from_envelope(
                        &work_group.id,
                        approval_envelope,
                        MessageKind::Approval,
                    )?;
                    approval_message.mentions = vec![lease.owner_agent_id.clone()];
                    self.storage.insert_message(&approval_message)?;
                    emit(&app, "chat:message-created", &approval_message)?;
                    emit(&app, "approval:requested", &tool_run)?;
                } else {
                    self.spawn_task_execution(
                        app.clone(),
                        claim_plan.task_card.id.clone(),
                        Some(tool_run.id.clone()),
                    );
                }
            }
        } else if claim_plan.lease.is_some() {
            self.spawn_task_execution(app.clone(), claim_plan.task_card.id.clone(), None);
        }

        Ok(())
    }

    fn dispatch_owner_workflow<R: Runtime>(
        &self,
        app: AppHandle<R>,
        plan: WorkflowPlan,
    ) -> Result<()> {
        self.storage.insert_workflow(&plan.workflow)?;
        self.record_workflow_checkpoint(
            &plan.workflow.id,
            crate::core::workflow::WorkflowCheckpointStatus::WorkflowPlanned,
        )?;

        for stage in &plan.stages {
            self.storage.insert_workflow_stage(&stage.stage)?;
            self.record_stage_checkpoint(
                &stage.stage.id,
                crate::core::workflow::WorkflowCheckpointStatus::StagePending,
            )?;
            for task in &stage.tasks {
                let task_card = TaskCard {
                    id: task.id.clone(),
                    parent_id: None,
                    source_message_id: plan.workflow.source_message_id.clone(),
                    title: task.title.clone(),
                    normalized_goal: task.goal.clone(),
                    input_payload: task.goal.clone(),
                    priority: 100 - (stage.stage.order_index * 5),
                    status: TaskStatus::Pending,
                    work_group_id: plan.workflow.work_group_id.clone(),
                    created_by: plan.workflow.owner_agent_id.clone(),
                    assigned_agent_id: Some(task.assignee_agent_id.clone()),
                    created_at: now(),
                };
                self.storage.insert_task_card(&task_card)?;
                emit(&app, "task:card-created", &task_card)?;
                self.storage.insert_task_dispatch(&TaskDispatchRecord {
                    task_id: task.id.clone(),
                    workflow_id: Some(plan.workflow.id.clone()),
                    stage_id: Some(stage.stage.id.clone()),
                    dispatch_source: TaskDispatchSource::OwnerAssign,
                    depends_on_task_ids: task.depends_on_task_ids.clone(),
                    acknowledged_at: None,
                    result_message_id: None,
                    locked_by_user_mention: task.locked_by_user_mention,
                    target_agent_id: task.assignee_agent_id.clone(),
                    route_mode: RequestRouteMode::OwnerOrchestrated,
                    narrative_stage_label: Some(stage.stage.title.clone()),
                    narrative_task_label: Some(task.title.clone()),
                })?;
            }
        }

        let owner_ack = self.owner_message_from_envelope(
            &plan.workflow.work_group_id,
            NarrativeEnvelope::new(
                NarrativeMessageType::OwnerAck,
                plan.owner_ack_text
                    .clone()
                    .context("owner workflow plan missing owner ack text")?,
            ),
            MessageKind::Status,
        )?;
        self.storage.insert_message(&owner_ack)?;
        emit(&app, "chat:message-created", &owner_ack)?;

        let plan_message = self.owner_plan_message(&plan)?;
        self.storage.insert_message(&plan_message)?;
        emit(&app, "chat:message-created", &plan_message)?;

        let first_stage_id = plan
            .stages
            .first()
            .map(|item| item.stage.id.clone())
            .context("workflow plan missing stages")?;
        let mut workflow = plan.workflow;
        workflow.current_stage_id = Some(first_stage_id.clone());
        workflow.status = WorkflowStatus::Running;
        self.storage.insert_workflow(&workflow)?;
        self.record_workflow_checkpoint(
            &workflow.id,
            crate::core::workflow::WorkflowCheckpointStatus::WorkflowRunning,
        )?;
        self.activate_next_stage(&app, &workflow.id, &first_stage_id)
    }

    pub(super) fn activate_next_stage<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        workflow_id: &str,
        stage_id: &str,
    ) -> Result<()> {
        let stage = self.storage.get_workflow_stage(stage_id)?;
        let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
        let pending_task_ids = dispatches
            .iter()
            .filter(|item| item.depends_on_task_ids.is_empty())
            .map(|item| item.task_id.clone())
            .collect::<Vec<_>>();
        self.activate_stage_tasks(app, workflow_id, &stage.id, &pending_task_ids, true)
    }

    fn activate_stage_tasks<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        workflow_id: &str,
        stage_id: &str,
        task_ids: &[String],
        mark_stage_running: bool,
    ) -> Result<()> {
        if task_ids.is_empty() {
            return Ok(());
        }
        let stage = self.storage.get_workflow_stage(stage_id)?;
        let workflow = self.storage.get_workflow(workflow_id)?;
        let work_group = self.storage.get_work_group(&workflow.work_group_id)?;
        let members = self
            .storage
            .list_agents()?
            .into_iter()
            .filter(|agent| work_group.member_agent_ids.contains(&agent.id))
            .collect::<Vec<_>>();
        let tasks = task_ids
            .iter()
            .map(|task_id| self.storage.get_task_card(task_id))
            .collect::<Result<Vec<_>>>()?;

        if mark_stage_running {
            let mut next_stage = stage.clone();
            next_stage.status = StageStatus::Running;
            self.storage.insert_workflow_stage(&next_stage)?;
            self.record_stage_checkpoint(
                &next_stage.id,
                crate::core::workflow::WorkflowCheckpointStatus::StageRunning,
            )?;
        }

        let dispatch_text =
            self.build_owner_stage_dispatch_text(&workflow, &stage, &tasks, &members)?;

        let mut envelope =
            NarrativeEnvelope::new(NarrativeMessageType::OwnerDispatch, dispatch_text);
        envelope.workflow_id = Some(workflow_id.to_string());
        envelope.stage_id = Some(stage_id.to_string());
        envelope.stage_title = Some(stage.title.clone());
        let dispatch_message = self.owner_message_from_envelope(
            &workflow.work_group_id,
            envelope,
            MessageKind::Status,
        )?;
        self.storage.insert_message(&dispatch_message)?;
        emit(app, "chat:message-created", &dispatch_message)?;

        for task in tasks {
            let agent_id = task
                .assigned_agent_id
                .clone()
                .context("assigned task missing agent")?;
            let agent = self.storage.get_agent(&agent_id)?;
            let mut active_task = task.clone();
            active_task.status = TaskStatus::Leased;
            self.storage.update_task_card(&active_task)?;
            self.record_task_checkpoint(
                &active_task,
                crate::core::workflow::WorkflowCheckpointStatus::TaskReady,
                0,
                None,
                None,
            )?;
            emit(app, "task:status-changed", &active_task)?;
            self.storage.insert_lease(&crate::core::domain::Lease {
                id: new_id(),
                task_card_id: active_task.id.clone(),
                owner_agent_id: agent.id.clone(),
                state: crate::core::domain::LeaseState::Active,
                granted_at: now(),
                expires_at: None,
                preempt_requested_at: None,
                released_at: None,
            })?;
            if let Some(lease) = self.storage.get_lease_by_task(&active_task.id)? {
                emit(app, "lease:granted", &lease)?;
            }
            self.spawn_task_execution(app.clone(), active_task.id.clone(), None);
            let ack_content = self.build_task_narrative_content(
                &active_task,
                NarrativeMessageType::AgentAck,
                self.build_agent_ack_text(&active_task, &agent)?,
            )?;
            let ack = ConversationMessage {
                id: new_id(),
                conversation_id: active_task.work_group_id.clone(),
                work_group_id: active_task.work_group_id.clone(),
                sender_kind: SenderKind::Agent,
                sender_id: agent.id.clone(),
                sender_name: agent.name.clone(),
                kind: MessageKind::Status,
                visibility: Visibility::Main,
                content: ack_content,
                mentions: vec![],
                task_card_id: Some(active_task.id.clone()),
                execution_mode: None,
                created_at: now(),
            };
            self.storage.insert_message(&ack)?;
            emit(app, "chat:message-created", &ack)?;
            if let Some(mut dispatch) = self.storage.get_task_dispatch(&active_task.id)? {
                dispatch.acknowledged_at = Some(now());
                self.storage.insert_task_dispatch(&dispatch)?;
            }
        }

        Ok(())
    }
}

fn should_use_owner_workflow(content: &str) -> bool {
    OWNER_ROUTING_KEYWORDS
        .iter()
        .chain(PROJECT_KEYWORDS.iter())
        .any(|keyword| content.contains(keyword))
}
