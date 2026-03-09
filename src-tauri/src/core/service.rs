use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::core::{
    agent_runtime::AgentRuntime,
    coordinator::Coordinator,
    domain::{
        new_id, now, AgentExecutor, AgentProfile, AuditEvent, ClaimContext, ClaimScorer,
        ConversationMessage, CreateAgentInput, CreateWorkGroupInput, DashboardState, Lease,
        LeaseState, MemoryItem, MemoryPolicy, MemoryScope, MessageKind, ModelPolicy,
        SendHumanMessageInput, SenderKind, SystemSettings, TaskCard, TaskExecutionContext,
        TaskStatus, ToolRun, ToolRunState, UpdateAgentInput, Visibility, WorkGroup,
    },
    llm_rig::RigModelAdapter,
    storage::Storage,
    tool_runtime::ToolRuntime,
};

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

struct TaskFailureReport {
    task: Option<TaskCard>,
    cancelled_tool_runs: Vec<ToolRun>,
    audit_event: AuditEvent,
}

#[derive(Clone)]
pub struct AppService {
    storage: Storage,
    coordinator: Coordinator,
    tool_runtime: Arc<ToolRuntime>,
    agent_runtime: Arc<AgentRuntime<RigModelAdapter, ToolRuntime>>,
}

impl AppService {
    pub fn new(
        workspace_root: std::path::PathBuf,
        app_data_dir: std::path::PathBuf,
    ) -> Result<Self> {
        let storage = Storage::new(app_data_dir.clone())?;
        let tool_runtime = Arc::new(ToolRuntime::new(workspace_root, app_data_dir)?);
        let model_adapter = Arc::new(RigModelAdapter);
        let agent_runtime = Arc::new(AgentRuntime::new(model_adapter, tool_runtime.clone()));
        let service = Self {
            storage,
            coordinator: Coordinator,
            tool_runtime,
            agent_runtime,
        };
        service.recover_runtime_state()?;
        Ok(service)
    }

    pub fn dashboard_state(&self) -> Result<DashboardState> {
        let mut state = self.storage.dashboard_state()?;
        state.tools = self.tool_runtime.builtin_tools();
        state.skills = self.tool_runtime.builtin_skills();
        Ok(state)
    }

    pub fn create_agent_profile(&self, input: CreateAgentInput) -> Result<AgentProfile> {
        let agent = AgentProfile {
            id: new_id(),
            name: input.name,
            avatar: input.avatar,
            role: input.role,
            objective: input.objective,
            model_policy: ModelPolicy {
                provider: input.provider,
                model: input.model,
                temperature: input.temperature,
            },
            skill_ids: input.skill_ids,
            tool_ids: input.tool_ids,
            max_parallel_runs: input.max_parallel_runs,
            can_spawn_subtasks: input.can_spawn_subtasks,
            memory_policy: MemoryPolicy::default(),
        };
        self.storage.insert_agent(&agent)?;
        self.record_audit(
            "agent.created",
            "agent",
            &agent.id,
            json!({ "name": agent.name, "role": agent.role }),
        )?;
        Ok(agent)
    }

    pub fn update_agent_profile(&self, input: UpdateAgentInput) -> Result<AgentProfile> {
        let agent = AgentProfile {
            id: input.id,
            name: input.name,
            avatar: input.avatar,
            role: input.role,
            objective: input.objective,
            model_policy: ModelPolicy {
                provider: input.provider,
                model: input.model,
                temperature: input.temperature,
            },
            skill_ids: input.skill_ids,
            tool_ids: input.tool_ids,
            max_parallel_runs: input.max_parallel_runs,
            can_spawn_subtasks: input.can_spawn_subtasks,
            memory_policy: MemoryPolicy::default(),
        };
        self.storage.insert_agent(&agent)?;
        self.record_audit(
            "agent.updated",
            "agent",
            &agent.id,
            json!({ "name": agent.name, "role": agent.role }),
        )?;
        Ok(agent)
    }

    pub fn delete_agent_profile(&self, agent_id: &str) -> Result<()> {
        let has_active_lease = self.storage.list_leases()?.into_iter().any(|lease| {
            lease.owner_agent_id == agent_id && !matches!(lease.state, LeaseState::Released)
        });
        if has_active_lease {
            return Err(anyhow!("cannot delete agent with active leases"));
        }

        let has_open_task = self.storage.list_task_cards(None)?.into_iter().any(|task| {
            task.assigned_agent_id.as_deref() == Some(agent_id)
                && !matches!(
                    task.status,
                    TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
                )
        });
        if has_open_task {
            return Err(anyhow!("cannot delete agent with unfinished assigned tasks"));
        }

        self.storage.delete_agent(agent_id)?;
        self.record_audit("agent.deleted", "agent", agent_id, json!({}))?;
        Ok(())
    }

    pub fn create_work_group(&self, input: CreateWorkGroupInput) -> Result<WorkGroup> {
        let group = WorkGroup {
            id: new_id(),
            kind: input.kind,
            name: input.name,
            goal: input.goal,
            member_agent_ids: vec![],
            default_visibility: input.default_visibility,
            auto_archive: input.auto_archive,
            created_at: now(),
            archived_at: None,
        };
        self.storage.insert_work_group(&group)?;
        self.record_audit(
            "work_group.created",
            "work_group",
            &group.id,
            json!({ "name": group.name }),
        )?;
        Ok(group)
    }

    pub fn add_agent_to_work_group(
        &self,
        work_group_id: &str,
        agent_id: &str,
    ) -> Result<WorkGroup> {
        let group = self
            .storage
            .add_agent_to_work_group(work_group_id, agent_id)?;
        self.record_audit(
            "work_group.member_added",
            "work_group",
            &group.id,
            json!({ "agentId": agent_id }),
        )?;
        Ok(group)
    }

    pub fn remove_agent_from_work_group(
        &self,
        work_group_id: &str,
        agent_id: &str,
    ) -> Result<WorkGroup> {
        let group = self
            .storage
            .remove_agent_from_work_group(work_group_id, agent_id)?;
        self.record_audit(
            "work_group.member_removed",
            "work_group",
            &group.id,
            json!({ "agentId": agent_id }),
        )?;
        Ok(group)
    }

    pub fn list_task_cards(&self, work_group_id: Option<&str>) -> Result<Vec<TaskCard>> {
        self.storage.list_task_cards(work_group_id)
    }

    pub fn get_audit_events(&self, limit: Option<usize>) -> Result<Vec<AuditEvent>> {
        self.storage.list_audit_events(limit)
    }

    pub fn get_settings(&self) -> Result<SystemSettings> {
        self.storage.get_settings()
    }

    pub fn update_settings(&self, settings: SystemSettings) -> Result<()> {
        self.storage.update_settings(&settings)?;
        self.record_audit(
            "settings.updated",
            "system",
            "settings",
            json!({ "providersCount": settings.providers.len() }),
        )?;
        Ok(())
    }

    pub fn send_human_message(
        &self,
        app: AppHandle,
        input: SendHumanMessageInput,
    ) -> Result<ConversationMessage> {
        let work_group = self.storage.get_work_group(&input.work_group_id)?;
        let agents = self.storage.list_agents()?;
        let members: Vec<AgentProfile> = agents
            .into_iter()
            .filter(|agent| work_group.member_agent_ids.contains(&agent.id))
            .collect();

        let mentions = Coordinator::extract_mentions(&input.content, &members);
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
            created_at: now(),
        };
        self.storage.insert_message(&human_message)?;
        emit(&app, "chat.message.created", &human_message)?;

        self.preempt_active_leases(&app, &work_group.id)?;

        let requested_tool = self
            .tool_runtime
            .select_tool_for_text(&input.content, &collect_allowed_tools(&members));
        let active_loads = self
            .storage
            .counts_for_agents(&work_group.member_agent_ids)?;

        let task_card = TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: human_message.id.clone(),
            title: Coordinator::build_task_title(&input.content),
            normalized_goal: input.content.trim().to_string(),
            input_payload: input.content.clone(),
            priority: 100,
            status: TaskStatus::Bidding,
            work_group_id: work_group.id.clone(),
            created_by: "human".into(),
            assigned_agent_id: None,
            created_at: now(),
        };

        let mut claim_plan = self.coordinator.score(ClaimContext {
            task_card,
            work_group: work_group.clone(),
            candidates: members.clone(),
            content: input.content.clone(),
            mentioned_agent_ids: human_message.mentions.clone(),
            active_loads,
            requested_tool: requested_tool.clone(),
        })?;

        if requested_tool
            .as_ref()
            .map(|tool| tool.risk_level == crate::core::domain::ToolRiskLevel::High)
            .unwrap_or(false)
        {
            claim_plan.task_card.status = TaskStatus::WaitingApproval;
        }

        self.storage.insert_task_card(&claim_plan.task_card)?;
        emit(&app, "task.card.created", &claim_plan.task_card)?;
        self.record_audit(
            "task.created",
            "task_card",
            &claim_plan.task_card.id,
            json!({ "sourceMessageId": human_message.id, "title": claim_plan.task_card.title }),
        )?;

        for bid in &claim_plan.bids {
            self.storage.insert_claim_bid(bid)?;
            emit(&app, "claim.bid.submitted", bid)?;
        }

        for message in &claim_plan.coordinator_messages {
            self.storage.insert_message(message)?;
            emit(&app, "chat.message.created", message)?;
        }

        if let Some(ref lease) = claim_plan.lease {
            self.storage.insert_lease(lease)?;
            emit(&app, "lease.granted", lease)?;
            self.record_audit(
                "lease.granted",
                "lease",
                &lease.id,
                json!({ "taskCardId": lease.task_card_id, "agentId": lease.owner_agent_id }),
            )?;
        }

        if let Some(tool) = requested_tool {
            if let Some(ref lease) = claim_plan.lease {
                let tool_run = ToolRun {
                    id: new_id(),
                    tool_id: tool.id.clone(),
                    task_card_id: claim_plan.task_card.id.clone(),
                    agent_id: lease.owner_agent_id.clone(),
                    state: if tool.risk_level == crate::core::domain::ToolRiskLevel::High {
                        ToolRunState::PendingApproval
                    } else {
                        ToolRunState::Queued
                    },
                    approval_required: tool.risk_level == crate::core::domain::ToolRiskLevel::High,
                    started_at: None,
                    finished_at: None,
                    result_ref: None,
                };
                self.storage.insert_tool_run(&tool_run)?;
                if tool_run.approval_required {
                    let approval_message = ConversationMessage {
                        id: new_id(),
                        conversation_id: work_group.id.clone(),
                        work_group_id: work_group.id.clone(),
                        sender_kind: SenderKind::System,
                        sender_id: "coordinator".into(),
                        sender_name: "Coordinator".into(),
                        kind: MessageKind::Approval,
                        visibility: Visibility::Main,
                        content: format!("Approval required for {} before execution.", tool.name),
                        mentions: vec![lease.owner_agent_id.clone()],
                        task_card_id: Some(claim_plan.task_card.id.clone()),
                        created_at: now(),
                    };
                    self.storage.insert_message(&approval_message)?;
                    emit(&app, "chat.message.created", &approval_message)?;
                    emit(&app, "approval.requested", &tool_run)?;
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

        Ok(human_message)
    }

    pub fn approve_tool_run(
        &self,
        app: AppHandle,
        tool_run_id: &str,
        approved: bool,
    ) -> Result<ToolRun> {
        let mut tool_run = self.storage.get_tool_run(tool_run_id)?;
        if approved {
            tool_run.state = ToolRunState::Queued;
            self.storage.insert_tool_run(&tool_run)?;
            emit(&app, "tool.run.started", &tool_run)?;
            self.record_audit(
                "tool_run.approved",
                "tool_run",
                &tool_run.id,
                json!({ "taskCardId": tool_run.task_card_id }),
            )?;
            self.spawn_task_execution(
                app.clone(),
                tool_run.task_card_id.clone(),
                Some(tool_run.id.clone()),
            );
        } else {
            tool_run.state = ToolRunState::Cancelled;
            tool_run.finished_at = Some(now());
            self.storage.insert_tool_run(&tool_run)?;
            let mut task_card = self.storage.get_task_card(&tool_run.task_card_id)?;
            task_card.status = TaskStatus::NeedsReview;
            self.storage.update_task_card(&task_card)?;
            emit(&app, "task.status.changed", &task_card)?;
            self.record_audit(
                "tool_run.rejected",
                "tool_run",
                &tool_run.id,
                json!({ "taskCardId": tool_run.task_card_id }),
            )?;
        }
        Ok(tool_run)
    }

    pub fn cancel_task_card(&self, app: AppHandle, task_card_id: &str) -> Result<TaskCard> {
        let mut task = self.storage.get_task_card(task_card_id)?;
        task.status = TaskStatus::Cancelled;
        self.storage.update_task_card(&task)?;
        if let Some(mut lease) = self.storage.get_lease_by_task(task_card_id)? {
            lease.state = LeaseState::Released;
            lease.released_at = Some(now());
            self.storage.update_lease(&lease)?;
        }
        self.record_audit("task.cancelled", "task_card", &task.id, json!({}))?;
        emit(&app, "task.status.changed", &task)?;
        Ok(task)
    }

    pub fn pause_lease(&self, app: AppHandle, lease_id: &str) -> Result<Lease> {
        let mut lease = self
            .storage
            .list_leases()?
            .into_iter()
            .find(|item| item.id == lease_id)
            .context("lease not found")?;
        lease.state = LeaseState::Paused;
        self.storage.update_lease(&lease)?;
        if let Ok(mut task) = self.storage.get_task_card(&lease.task_card_id) {
            task.status = TaskStatus::Paused;
            self.storage.update_task_card(&task)?;
            emit(&app, "task.status.changed", &task)?;
        }
        Ok(lease)
    }

    pub fn resume_task_card(&self, app: AppHandle, task_card_id: &str) -> Result<TaskCard> {
        let mut task = self.storage.get_task_card(task_card_id)?;
        task.status = TaskStatus::Leased;
        self.storage.update_task_card(&task)?;
        if let Some(mut lease) = self.storage.get_lease_by_task(task_card_id)? {
            lease.state = LeaseState::Active;
            lease.preempt_requested_at = None;
            self.storage.update_lease(&lease)?;
            emit(&app, "lease.granted", &lease)?;
        }
        emit(&app, "task.status.changed", &task)?;
        self.spawn_task_execution(app, task.id.clone(), None);
        Ok(task)
    }

    fn preempt_active_leases(&self, app: &AppHandle, work_group_id: &str) -> Result<()> {
        let leases = self.storage.list_active_leases_for_group(work_group_id)?;
        for mut lease in leases {
            lease.state = LeaseState::PreemptRequested;
            lease.preempt_requested_at = Some(now());
            self.storage.update_lease(&lease)?;
            emit(app, "lease.preempt_requested", &lease)?;
            self.record_audit(
                "lease.preempt_requested",
                "lease",
                &lease.id,
                json!({ "taskCardId": lease.task_card_id }),
            )?;
        }
        Ok(())
    }

    fn recover_runtime_state(&self) -> Result<()> {
        let mut report = RecoveryReport::default();
        let tool_runs = self.storage.list_tool_runs()?;

        for mut tool_run in tool_runs {
            match tool_run.state {
                ToolRunState::Completed | ToolRunState::Cancelled => continue,
                ToolRunState::PendingApproval => {
                    let mut task = self.storage.get_task_card(&tool_run.task_card_id)?;
                    if matches!(
                        task.status,
                        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
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
            ) {
                continue;
            }
            if self.storage.get_lease_by_task(&task.id)?.is_none() {
                task.status = TaskStatus::NeedsReview;
                self.storage.update_task_card(&task)?;
                report.review_tasks += 1;
            }
        }

        let parent_ids: std::collections::HashSet<String> = self
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

    fn spawn_task_execution(
        &self,
        app: AppHandle,
        task_card_id: String,
        tool_run_id: Option<String>,
    ) {
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = service
                .run_task(app.clone(), &task_card_id, tool_run_id.as_deref())
                .await
            {
                if let Ok(report) = service.handle_task_execution_failure(&task_card_id, &error) {
                    if let Some(task) = report.task {
                        let _ = emit(&app, "task.status.changed", &task);
                    }
                    for tool_run in report.cancelled_tool_runs {
                        let _ = emit(&app, "tool.run.completed", &tool_run);
                    }
                    let _ = emit(&app, "audit.event.created", &report.audit_event);
                }
            }
        });
    }

    fn handle_task_execution_failure(
        &self,
        task_card_id: &str,
        error: &anyhow::Error,
    ) -> Result<TaskFailureReport> {
        let mut task = self.storage.get_task_card(task_card_id).ok();
        let mut cancelled_tool_runs = Vec::new();

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

        let audit_event = AuditEvent {
            id: new_id(),
            event_type: "task.execution_error".into(),
            entity_type: "task_card".into(),
            entity_id: task_card_id.into(),
            payload_json: json!({ "error": error.to_string() }).to_string(),
            created_at: now(),
        };
        self.storage.insert_audit_event(&audit_event)?;

        Ok(TaskFailureReport {
            task,
            cancelled_tool_runs,
            audit_event,
        })
    }

    async fn run_task(
        &self,
        app: AppHandle,
        task_card_id: &str,
        tool_run_id: Option<&str>,
    ) -> Result<()> {
        let mut task = self.storage.get_task_card(task_card_id)?;
        let mut lease = self
            .storage
            .get_lease_by_task(task_card_id)?
            .ok_or_else(|| anyhow!("lease missing for task"))?;
        if lease.state == LeaseState::Paused {
            return Ok(());
        }

        task.status = TaskStatus::InProgress;
        self.storage.update_task_card(&task)?;
        emit(&app, "task.status.changed", &task)?;

        let work_group = self.storage.get_work_group(&task.work_group_id)?;
        let owner_id = lease.owner_agent_id.clone();
        let agent = self.storage.get_agent(&owner_id)?;
        let work_group_members = self
            .storage
            .list_agents()?
            .into_iter()
            .filter(|candidate| work_group.member_agent_ids.contains(&candidate.id))
            .collect();
        let messages = self.storage.list_messages_for_group(&work_group.id)?;
        let approved_tool = if let Some(id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(id)?;
            tool_run.state = ToolRunState::Running;
            tool_run.started_at = Some(now());
            self.storage.insert_tool_run(&tool_run)?;
            emit(&app, "tool.run.started", &tool_run)?;
            self.tool_runtime.tool_by_id(&tool_run.tool_id)
        } else {
            self.tool_runtime
                .select_tool_for_text(&task.input_payload, &agent.tool_ids)
                .filter(|tool| tool.risk_level != crate::core::domain::ToolRiskLevel::High)
        };

        if let Some(tool) = approved_tool.clone() {
            let tool_call_message = ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::System,
                sender_id: "coordinator".into(),
                sender_name: "Coordinator".into(),
                kind: MessageKind::ToolCall,
                visibility: Visibility::Backstage,
                content: format!("Executing tool '{}' for task '{}'.", tool.name, task.title),
                mentions: vec![agent.id.clone()],
                task_card_id: Some(task.id.clone()),
                created_at: now(),
            };
            self.storage.insert_message(&tool_call_message)?;
            emit(&app, "chat.message.created", &tool_call_message)?;
            self.record_audit(
                "tool_run.started",
                "task_card",
                &task.id,
                json!({ "toolId": tool.id, "agentId": agent.id }),
            )?;
        }

        let available_skills = self
            .tool_runtime
            .builtin_skills()
            .into_iter()
            .filter(|skill| agent.skill_ids.contains(&skill.id))
            .collect();
        let available_tools = self
            .tool_runtime
            .builtin_tools()
            .into_iter()
            .filter(|tool| agent.tool_ids.contains(&tool.id))
            .collect();

        let execution = self
            .agent_runtime
            .execute_task(TaskExecutionContext {
                agent: agent.clone(),
                work_group: work_group.clone(),
                work_group_members,
                task_card: task.clone(),
                conversation_window: messages,
                available_tools,
                available_skills,
                approved_tool: approved_tool.clone(),
                settings: self.storage.get_settings()?,
            })
            .await?;

        if let Some(id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(id)?;
            tool_run.state = ToolRunState::Completed;
            tool_run.finished_at = Some(now());
            tool_run.result_ref = execution.tool_output.clone();
            self.storage.insert_tool_run(&tool_run)?;
            emit(&app, "tool.run.completed", &tool_run)?;
        }

        if let Some(tool_output) = execution.tool_output.clone() {
            let tool_result_message = ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::System,
                sender_id: "coordinator".into(),
                sender_name: "Coordinator".into(),
                kind: MessageKind::ToolResult,
                visibility: Visibility::Backstage,
                content: tool_output.clone(),
                mentions: vec![agent.id.clone()],
                task_card_id: Some(task.id.clone()),
                created_at: now(),
            };
            self.storage.insert_message(&tool_result_message)?;
            emit(&app, "chat.message.created", &tool_result_message)?;
            self.record_audit(
                "tool_run.completed",
                "task_card",
                &task.id,
                json!({ "agentId": agent.id, "result": tool_output }),
            )?;
        }

        let summary_message = ConversationMessage {
            id: new_id(),
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Summary,
            visibility: Visibility::Main,
            content: execution.summary.clone(),
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            created_at: now(),
        };
        self.storage.insert_message(&summary_message)?;
        emit(&app, "chat.message.created", &summary_message)?;

        let backstage_message = ConversationMessage {
            id: new_id(),
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Backstage,
            content: execution.backstage_notes.clone(),
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            created_at: now(),
        };
        self.storage.insert_message(&backstage_message)?;
        emit(&app, "chat.message.created", &backstage_message)?;

        self.storage.insert_memory_item(&MemoryItem {
            id: new_id(),
            scope: MemoryScope::Agent,
            scope_id: agent.id.clone(),
            content: execution.summary.clone(),
            tags: vec!["summary".into(), "execution".into()],
            embedding_ref: None,
            pinned: false,
            ttl: None,
            created_at: now(),
        })?;
        emit(
            &app,
            "memory.updated",
            &json!({ "agentId": agent.id, "taskCardId": task.id }),
        )?;

        let mut spawned_subtasks = Vec::new();
        if !execution.suggested_subtasks.is_empty() {
            for subtask in execution.suggested_subtasks.iter().take(1) {
                if let Some(created_task) = self.spawn_subtask(&app, &task, &agent, subtask)? {
                    spawned_subtasks.push(created_task);
                }
            }
        }

        if !spawned_subtasks.is_empty() {
            task.status = TaskStatus::WaitingChildren;
            self.storage.update_task_card(&task)?;
            emit(&app, "task.status.changed", &task)?;
            let waiting_message = ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::System,
                sender_id: "coordinator".into(),
                sender_name: "Coordinator".into(),
                kind: MessageKind::Status,
                visibility: Visibility::Main,
                content: format!(
                    "{} is waiting on {} child task(s) before completion.",
                    task.title,
                    spawned_subtasks.len()
                ),
                mentions: vec![agent.id.clone()],
                task_card_id: Some(task.id.clone()),
                created_at: now(),
            };
            self.storage.insert_message(&waiting_message)?;
            emit(&app, "chat.message.created", &waiting_message)?;
            self.record_audit(
                "task.waiting_children",
                "task_card",
                &task.id,
                json!({ "children": spawned_subtasks.iter().map(|item| item.id.clone()).collect::<Vec<_>>() }),
            )?;
            return Ok(());
        }

        lease = self
            .storage
            .get_lease_by_task(&task.id)?
            .ok_or_else(|| anyhow!("lease disappeared"))?;
        if lease.state == LeaseState::PreemptRequested {
            lease.state = LeaseState::Paused;
            self.storage.update_lease(&lease)?;
            task.status = TaskStatus::Paused;
            self.storage.update_task_card(&task)?;
            emit(&app, "task.status.changed", &task)?;
            return Ok(());
        }

        lease.state = LeaseState::Released;
        lease.released_at = Some(now());
        self.storage.update_lease(&lease)?;

        task.status = TaskStatus::Completed;
        self.storage.update_task_card(&task)?;
        emit(&app, "task.status.changed", &task)?;
        self.record_audit(
            "task.completed",
            "task_card",
            &task.id,
            json!({ "ownerAgentId": agent.id }),
        )?;
        if let Some(parent_id) = task.parent_id.clone() {
            self.reconcile_parent_task(&app, &parent_id)?;
        }
        Ok(())
    }

    fn spawn_subtask(
        &self,
        app: &AppHandle,
        parent_task: &TaskCard,
        owner_agent: &AgentProfile,
        content: &str,
    ) -> Result<Option<TaskCard>> {
        let work_group = self.storage.get_work_group(&parent_task.work_group_id)?;
        let all_agents = self.storage.list_agents()?;
        let members: Vec<AgentProfile> = all_agents
            .into_iter()
            .filter(|agent| {
                work_group.member_agent_ids.contains(&agent.id) && agent.id != owner_agent.id
            })
            .collect();
        if members.is_empty() {
            return Ok(None);
        }

        let task_card = TaskCard {
            id: new_id(),
            parent_id: Some(parent_task.id.clone()),
            source_message_id: parent_task.source_message_id.clone(),
            title: Coordinator::build_task_title(content),
            normalized_goal: content.to_string(),
            input_payload: content.to_string(),
            priority: 60,
            status: TaskStatus::Bidding,
            work_group_id: work_group.id.clone(),
            created_by: owner_agent.id.clone(),
            assigned_agent_id: None,
            created_at: now(),
        };
        let active_loads = self
            .storage
            .counts_for_agents(&work_group.member_agent_ids)?;
        let selected_tool = self
            .tool_runtime
            .select_tool_for_text(content, &collect_allowed_tools(&members));
        let mentioned_agent_ids = Coordinator::extract_mentions(content, &members);
        let claim_plan = self.coordinator.score(ClaimContext {
            task_card,
            work_group: work_group.clone(),
            candidates: members.clone(),
            content: content.to_string(),
            mentioned_agent_ids,
            active_loads,
            requested_tool: selected_tool.clone(),
        })?;
        let selected_tool = claim_plan.requested_tool.clone();
        self.storage.insert_task_card(&claim_plan.task_card)?;
        emit(app, "task.card.created", &claim_plan.task_card)?;
        for bid in &claim_plan.bids {
            self.storage.insert_claim_bid(bid)?;
            emit(app, "claim.bid.submitted", bid)?;
        }
        for message in &claim_plan.coordinator_messages {
            self.storage.insert_message(message)?;
            emit(app, "chat.message.created", message)?;
        }
        if let Some(ref lease) = claim_plan.lease {
            self.storage.insert_lease(lease)?;
            emit(app, "lease.granted", lease)?;
        }
        if let Some(tool) = selected_tool {
            if let Some(ref lease) = claim_plan.lease {
                let tool_run = ToolRun {
                    id: new_id(),
                    tool_id: tool.id.clone(),
                    task_card_id: claim_plan.task_card.id.clone(),
                    agent_id: lease.owner_agent_id.clone(),
                    state: if tool.risk_level == crate::core::domain::ToolRiskLevel::High {
                        ToolRunState::PendingApproval
                    } else {
                        ToolRunState::Queued
                    },
                    approval_required: tool.risk_level == crate::core::domain::ToolRiskLevel::High,
                    started_at: None,
                    finished_at: None,
                    result_ref: None,
                };
                self.storage.insert_tool_run(&tool_run)?;
                if !tool_run.approval_required {
                    self.spawn_task_execution(
                        app.clone(),
                        claim_plan.task_card.id.clone(),
                        Some(tool_run.id),
                    );
                }
            }
        } else if claim_plan.lease.is_some() {
            self.spawn_task_execution(app.clone(), claim_plan.task_card.id.clone(), None);
        }
        Ok(Some(claim_plan.task_card))
    }

    fn reconcile_parent_task(&self, app: &AppHandle, parent_id: &str) -> Result<()> {
        let parent_task_before = self.storage.get_task_card(parent_id)?;
        let child_tasks = self.storage.list_child_tasks(parent_id)?;
        if !self.reconcile_parent_task_state(parent_id)? {
            return Ok(());
        }
        let parent_task = self.storage.get_task_card(parent_id)?;
        emit(app, "task.status.changed", &parent_task)?;
        let has_issue = matches!(parent_task.status, TaskStatus::NeedsReview);

        let status_message = ConversationMessage {
            id: new_id(),
            conversation_id: parent_task.work_group_id.clone(),
            work_group_id: parent_task.work_group_id.clone(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind: MessageKind::Summary,
            visibility: Visibility::Main,
            content: if has_issue {
                format!(
                    "Parent task '{}' moved to needs review after child task completion.",
                    parent_task.title
                )
            } else {
                format!(
                    "Parent task '{}' completed after all child tasks finished.",
                    parent_task.title
                )
            },
            mentions: vec![],
            task_card_id: Some(parent_task.id.clone()),
            created_at: now(),
        };
        if parent_task_before.status != parent_task.status {
            self.storage.insert_message(&status_message)?;
            emit(app, "chat.message.created", &status_message)?;
        }
        self.record_audit(
            "task.parent_reconciled",
            "task_card",
            &parent_task.id,
            json!({
                "childTaskIds": child_tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
                "status": parent_task.status.clone(),
            }),
        )?;

        if let Some(grand_parent_id) = parent_task.parent_id.clone() {
            self.reconcile_parent_task(app, &grand_parent_id)?;
        }

        Ok(())
    }

    fn reconcile_parent_task_state(&self, parent_id: &str) -> Result<bool> {
        let mut parent_task = self.storage.get_task_card(parent_id)?;
        let child_tasks = self.storage.list_child_tasks(parent_id)?;
        if child_tasks.is_empty() {
            return Ok(false);
        }

        let has_terminal_children = child_tasks.iter().all(|child| {
            matches!(
                child.status,
                TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
            )
        });
        if !has_terminal_children {
            return Ok(false);
        }

        let has_issue = child_tasks.iter().any(|child| {
            matches!(
                child.status,
                TaskStatus::Cancelled | TaskStatus::NeedsReview
            )
        });
        let next_status = if has_issue {
            TaskStatus::NeedsReview
        } else {
            TaskStatus::Completed
        };
        let changed = parent_task.status != next_status;
        if changed {
            parent_task.status = next_status;
            self.storage.update_task_card(&parent_task)?;
        }

        if let Some(mut lease) = self.storage.get_lease_by_task(parent_id)? {
            if lease.state != LeaseState::Released {
                lease.state = LeaseState::Released;
                lease.released_at = Some(now());
                lease.preempt_requested_at = None;
                self.storage.update_lease(&lease)?;
            }
        }

        Ok(changed)
    }

    fn record_audit(
        &self,
        event_type: &str,
        entity_type: &str,
        entity_id: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        self.storage.insert_audit_event(&AuditEvent {
            id: new_id(),
            event_type: event_type.into(),
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            payload_json: payload.to_string(),
            created_at: now(),
        })
    }
}

fn collect_allowed_tools(agents: &[AgentProfile]) -> Vec<String> {
    let mut ids = Vec::new();
    for agent in agents {
        for tool_id in &agent.tool_ids {
            if !ids.contains(tool_id) {
                ids.push(tool_id.clone());
            }
        }
    }
    ids
}

fn emit<T: Serialize>(app: &AppHandle, event: &str, payload: &T) -> Result<()> {
    app.emit(event, payload)
        .map_err(|error| anyhow!("failed to emit {event}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::AppService;
    use crate::core::domain::{
        new_id, now, Lease, LeaseState, TaskCard, TaskStatus, ToolRun, ToolRunState,
    };
    use anyhow::anyhow;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_root(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("nextchat-service-{prefix}-{nanos}"))
    }

    fn setup_service() -> (AppService, PathBuf, PathBuf) {
        let workspace_root = unique_root("workspace");
        let data_root = unique_root("data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let service = AppService::new(workspace_root.clone(), data_root.clone()).expect("service");
        (service, workspace_root, data_root)
    }

    #[test]
    fn startup_recovery_pauses_low_risk_inflight_work() {
        let (service, workspace_root, data_root) = setup_service();
        let work_group = service
            .storage
            .list_work_groups()
            .expect("groups")
            .into_iter()
            .next()
            .expect("seeded group");
        let agent = service
            .storage
            .list_agents()
            .expect("agents")
            .into_iter()
            .next()
            .expect("seeded agent");

        let task = TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: new_id(),
            title: "Recover queued search".into(),
            normalized_goal: "Search the workspace after restart.".into(),
            input_payload: "search the workspace".into(),
            priority: 10,
            status: TaskStatus::InProgress,
            work_group_id: work_group.id.clone(),
            created_by: "human".into(),
            assigned_agent_id: Some(agent.id.clone()),
            created_at: now(),
        };
        service.storage.insert_task_card(&task).expect("task");
        service
            .storage
            .insert_lease(&Lease {
                id: new_id(),
                task_card_id: task.id.clone(),
                owner_agent_id: agent.id.clone(),
                state: LeaseState::Active,
                granted_at: now(),
                expires_at: None,
                preempt_requested_at: None,
                released_at: None,
            })
            .expect("lease");
        service
            .storage
            .insert_tool_run(&ToolRun {
                id: new_id(),
                tool_id: "project.search".into(),
                task_card_id: task.id.clone(),
                agent_id: agent.id.clone(),
                state: ToolRunState::Running,
                approval_required: false,
                started_at: Some(now()),
                finished_at: None,
                result_ref: None,
            })
            .expect("tool run");

        let recovered = AppService::new(workspace_root, data_root).expect("recovered service");

        let recovered_task = recovered.storage.get_task_card(&task.id).expect("task");
        assert_eq!(recovered_task.status, TaskStatus::Paused);

        let recovered_lease = recovered
            .storage
            .get_lease_by_task(&task.id)
            .expect("lease")
            .expect("lease exists");
        assert_eq!(recovered_lease.state, LeaseState::Paused);

        let recovered_run = recovered
            .storage
            .list_tool_runs()
            .expect("runs")
            .into_iter()
            .find(|run| run.task_card_id == task.id)
            .expect("run");
        assert_eq!(recovered_run.state, ToolRunState::Queued);
        assert!(recovered_run.started_at.is_none());
    }

    #[test]
    fn startup_recovery_marks_high_risk_inflight_work_for_review() {
        let (service, workspace_root, data_root) = setup_service();
        let work_group = service
            .storage
            .list_work_groups()
            .expect("groups")
            .into_iter()
            .next()
            .expect("seeded group");
        let agent = service
            .storage
            .list_agents()
            .expect("agents")
            .into_iter()
            .find(|item| item.tool_ids.iter().any(|tool| tool == "shell.exec"))
            .expect("agent with shell");

        let task = TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: new_id(),
            title: "Recover shell execution".into(),
            normalized_goal: "Run a shell command after approval.".into(),
            input_payload: "run shell command".into(),
            priority: 10,
            status: TaskStatus::InProgress,
            work_group_id: work_group.id.clone(),
            created_by: "human".into(),
            assigned_agent_id: Some(agent.id.clone()),
            created_at: now(),
        };
        service.storage.insert_task_card(&task).expect("task");
        service
            .storage
            .insert_lease(&Lease {
                id: new_id(),
                task_card_id: task.id.clone(),
                owner_agent_id: agent.id.clone(),
                state: LeaseState::Active,
                granted_at: now(),
                expires_at: None,
                preempt_requested_at: None,
                released_at: None,
            })
            .expect("lease");
        service
            .storage
            .insert_tool_run(&ToolRun {
                id: new_id(),
                tool_id: "shell.exec".into(),
                task_card_id: task.id.clone(),
                agent_id: agent.id.clone(),
                state: ToolRunState::Running,
                approval_required: true,
                started_at: Some(now()),
                finished_at: None,
                result_ref: None,
            })
            .expect("tool run");

        let recovered = AppService::new(workspace_root, data_root).expect("recovered service");

        let recovered_task = recovered.storage.get_task_card(&task.id).expect("task");
        assert_eq!(recovered_task.status, TaskStatus::NeedsReview);

        let recovered_lease = recovered
            .storage
            .get_lease_by_task(&task.id)
            .expect("lease")
            .expect("lease exists");
        assert_eq!(recovered_lease.state, LeaseState::Released);
        assert!(recovered_lease.released_at.is_some());

        let recovered_run = recovered
            .storage
            .list_tool_runs()
            .expect("runs")
            .into_iter()
            .find(|run| run.task_card_id == task.id)
            .expect("run");
        assert_eq!(recovered_run.state, ToolRunState::Cancelled);
        assert!(recovered_run.finished_at.is_some());
    }

    #[test]
    fn execution_failure_moves_task_to_review_and_releases_lease() {
        let (service, _, _) = setup_service();
        let work_group = service
            .storage
            .list_work_groups()
            .expect("groups")
            .into_iter()
            .next()
            .expect("seeded group");
        let agent = service
            .storage
            .list_agents()
            .expect("agents")
            .into_iter()
            .next()
            .expect("seeded agent");

        let task = TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: new_id(),
            title: "Failing task".into(),
            normalized_goal: "Simulate execution failure.".into(),
            input_payload: "simulate failure".into(),
            priority: 10,
            status: TaskStatus::InProgress,
            work_group_id: work_group.id.clone(),
            created_by: "human".into(),
            assigned_agent_id: Some(agent.id.clone()),
            created_at: now(),
        };
        service.storage.insert_task_card(&task).expect("task");
        service
            .storage
            .insert_lease(&Lease {
                id: new_id(),
                task_card_id: task.id.clone(),
                owner_agent_id: agent.id.clone(),
                state: LeaseState::Active,
                granted_at: now(),
                expires_at: None,
                preempt_requested_at: None,
                released_at: None,
            })
            .expect("lease");
        service
            .storage
            .insert_tool_run(&ToolRun {
                id: new_id(),
                tool_id: "project.search".into(),
                task_card_id: task.id.clone(),
                agent_id: agent.id.clone(),
                state: ToolRunState::Running,
                approval_required: false,
                started_at: Some(now()),
                finished_at: None,
                result_ref: None,
            })
            .expect("tool run");

        let report = service
            .handle_task_execution_failure(&task.id, &anyhow!("boom"))
            .expect("failure report");

        assert_eq!(report.task.expect("task").status, TaskStatus::NeedsReview);
        assert_eq!(report.cancelled_tool_runs.len(), 1);

        let lease = service
            .storage
            .get_lease_by_task(&task.id)
            .expect("lease")
            .expect("lease exists");
        assert_eq!(lease.state, LeaseState::Released);
        assert!(lease.released_at.is_some());

        let tool_run = service
            .storage
            .list_tool_runs()
            .expect("runs")
            .into_iter()
            .find(|run| run.task_card_id == task.id)
            .expect("tool run");
        assert_eq!(tool_run.state, ToolRunState::Cancelled);
        assert!(tool_run.finished_at.is_some());
    }
}
