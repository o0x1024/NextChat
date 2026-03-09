mod collaboration;
mod memory;
mod runtime;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Runtime};

use crate::core::{
    agent_runtime::AgentRuntime,
    coordinator::Coordinator,
    domain::{
        new_id, now, AIProviderConfig, AgentProfile, AuditEvent, ClaimContext, ClaimScorer,
        ConversationMessage, CreateAgentInput, CreateWorkGroupInput, DashboardState, Lease,
        LeaseState, MessageKind, ModelPolicy, SendHumanMessageInput, SenderKind, SystemSettings,
        TaskCard, TaskStatus, ToolRun, ToolRunState, UpdateAgentInput, Visibility, WorkGroup,
    },
    llm_rig::{refresh_models, RigModelAdapter},
    storage::Storage,
    tool_runtime::ToolRuntime,
};

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
        let expired_memory = service.storage.cleanup_expired_memory_items()?;
        service.recover_runtime_state()?;
        if expired_memory > 0 {
            service.record_audit(
                "memory.expired_cleanup",
                "system",
                "startup",
                json!({ "deletedCount": expired_memory }),
            )?;
        }
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
            memory_policy: input.memory_policy,
            permission_policy: input.permission_policy,
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
            memory_policy: input.memory_policy,
            permission_policy: input.permission_policy,
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
            return Err(anyhow!(
                "cannot delete agent with unfinished assigned tasks"
            ));
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

    pub async fn refresh_provider_models(
        &self,
        config: AIProviderConfig,
    ) -> Result<AIProviderConfig> {
        let mut settings = self.storage.get_settings()?;
        let provider_index = settings
            .providers
            .iter()
            .position(|provider| provider.id == config.id)
            .ok_or_else(|| anyhow!("provider not found: {}", config.id))?;

        let models = refresh_models(&config).await?;
        let mut updated_provider = config;
        updated_provider.models = models;
        if !updated_provider
            .models
            .contains(&updated_provider.default_model)
        {
            updated_provider.default_model = updated_provider
                .models
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("provider returned no models"))?;
        }

        settings.providers[provider_index] = updated_provider.clone();

        if settings.global_config.default_llm_provider == updated_provider.id
            && !updated_provider
                .models
                .contains(&settings.global_config.default_llm_model)
        {
            settings.global_config.default_llm_model = updated_provider.default_model.clone();
        }

        if settings.global_config.default_vlm_provider == updated_provider.id
            && !updated_provider
                .models
                .contains(&settings.global_config.default_vlm_model)
        {
            settings.global_config.default_vlm_model = updated_provider.default_model.clone();
        }

        self.storage.update_settings(&settings)?;
        self.record_audit(
            "settings.provider_models_refreshed",
            "provider",
            &updated_provider.id,
            json!({
                "providerId": updated_provider.id.clone(),
                "modelsCount": updated_provider.models.len(),
                "defaultModel": updated_provider.default_model.clone(),
            }),
        )?;

        Ok(updated_provider)
    }

    fn build_permission_denied_message(
        &self,
        work_group_id: &str,
        task_id: &str,
        agent: &AgentProfile,
        tool: &crate::core::domain::ToolManifest,
        reason: &str,
    ) -> ConversationMessage {
        ConversationMessage {
            id: new_id(),
            conversation_id: work_group_id.to_string(),
            work_group_id: work_group_id.to_string(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content: format!(
                "Permission denied: {} cannot use {}. {}",
                agent.name, tool.name, reason
            ),
            mentions: vec![agent.id.clone()],
            task_card_id: Some(task_id.to_string()),
            execution_mode: None,
            created_at: now(),
        }
    }

    fn handle_permission_denial<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        work_group_id: &str,
        task: &mut TaskCard,
        lease: Option<&mut Lease>,
        agent: &AgentProfile,
        tool: &crate::core::domain::ToolManifest,
        reason: &str,
    ) -> Result<()> {
        task.status = TaskStatus::NeedsReview;
        self.storage.update_task_card(task)?;
        emit(app, "task:status-changed", task)?;

        if let Some(lease) = lease {
            lease.state = LeaseState::Released;
            lease.released_at = Some(now());
            lease.preempt_requested_at = None;
            self.storage.update_lease(lease)?;
        }

        let message =
            self.build_permission_denied_message(work_group_id, &task.id, agent, tool, reason);
        self.storage.insert_message(&message)?;
        emit(app, "chat:message-created", &message)?;
        self.record_audit(
            "tool_run.permission_denied",
            "task_card",
            &task.id,
            json!({
                "agentId": agent.id,
                "toolId": tool.id,
                "reason": reason,
            }),
        )?;
        self.emit_collaboration_result(
            app,
            task,
            agent,
            TaskStatus::NeedsReview,
            &format!("Permission denied before execution. {reason}"),
            None,
        )?;
        if let Some(parent_id) = task.parent_id.clone() {
            self.reconcile_parent_task(app, &parent_id)?;
        }
        Ok(())
    }

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
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_message(&human_message)?;
        emit(&app, "chat:message-created", &human_message)?;

        self.preempt_active_leases(&app, &work_group.id)?;

        let scored_members = scored_candidates(&self.tool_runtime, &members);
        let requested_tool = self.tool_runtime.select_tool_for_text(
            &input.content,
            &collect_allowed_tools(&self.tool_runtime, &members),
        );
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
            candidates: scored_members,
            content: input.content.clone(),
            mentioned_agent_ids: human_message.mentions.clone(),
            active_loads,
            requested_tool: requested_tool.clone(),
        })?;

        let mut denied_request: Option<(AgentProfile, crate::core::domain::ToolManifest, String)> =
            None;
        let mut requested_tool_requires_approval = false;
        if let (Some(tool), Some(lease)) = (requested_tool.clone(), claim_plan.lease.as_ref()) {
            if let Some(agent) = members
                .iter()
                .find(|candidate| candidate.id == lease.owner_agent_id)
                .cloned()
            {
                let decision = self.tool_runtime.authorize_tool_call(
                    &agent,
                    &tool,
                    &claim_plan.task_card.input_payload,
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

        self.storage.insert_task_card(&claim_plan.task_card)?;
        emit(&app, "task:card-created", &claim_plan.task_card)?;
        self.record_audit(
            "task.created",
            "task_card",
            &claim_plan.task_card.id,
            json!({ "sourceMessageId": human_message.id, "title": claim_plan.task_card.title }),
        )?;

        for bid in &claim_plan.bids {
            self.storage.insert_claim_bid(bid)?;
            emit(&app, "claim:bid-submitted", bid)?;
        }

        for message in &claim_plan.coordinator_messages {
            self.storage.insert_message(message)?;
            emit(&app, "chat:message-created", message)?;
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
                let tool_run = ToolRun {
                    id: new_id(),
                    tool_id: tool.id.clone(),
                    task_card_id: claim_plan.task_card.id.clone(),
                    agent_id: lease.owner_agent_id.clone(),
                    state: if requested_tool_requires_approval {
                        ToolRunState::PendingApproval
                    } else {
                        ToolRunState::Queued
                    },
                    approval_required: requested_tool_requires_approval,
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
                        execution_mode: None,
                        created_at: now(),
                    };
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

        Ok(human_message)
    }

    pub fn approve_tool_run<R: Runtime>(
        &self,
        app: AppHandle<R>,
        tool_run_id: &str,
        approved: bool,
    ) -> Result<ToolRun> {
        let mut tool_run = self.storage.get_tool_run(tool_run_id)?;
        if approved {
            tool_run.state = ToolRunState::Queued;
            self.storage.insert_tool_run(&tool_run)?;
            emit(&app, "tool:run-started", &tool_run)?;
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
            emit(&app, "task:status-changed", &task_card)?;
            self.record_audit(
                "tool_run.rejected",
                "tool_run",
                &tool_run.id,
                json!({ "taskCardId": tool_run.task_card_id }),
            )?;
            let agent = self.storage.get_agent(&tool_run.agent_id)?;
            self.emit_collaboration_result(
                &app,
                &task_card,
                &agent,
                TaskStatus::NeedsReview,
                "Approval rejected before tool execution.",
                None,
            )?;
            if let Some(parent_id) = task_card.parent_id.clone() {
                self.reconcile_parent_task(&app, &parent_id)?;
            }
        }
        Ok(tool_run)
    }

    pub fn cancel_task_card<R: Runtime>(
        &self,
        app: AppHandle<R>,
        task_card_id: &str,
    ) -> Result<TaskCard> {
        let mut task = self.storage.get_task_card(task_card_id)?;
        task.status = TaskStatus::Cancelled;
        self.storage.update_task_card(&task)?;
        if let Some(mut lease) = self.storage.get_lease_by_task(task_card_id)? {
            lease.state = LeaseState::Released;
            lease.released_at = Some(now());
            self.storage.update_lease(&lease)?;
        }
        self.record_audit("task.cancelled", "task_card", &task.id, json!({}))?;
        emit(&app, "task:status-changed", &task)?;
        if let Some(agent_id) = task.assigned_agent_id.as_deref() {
            let agent = self.storage.get_agent(agent_id)?;
            self.emit_collaboration_result(
                &app,
                &task,
                &agent,
                TaskStatus::Cancelled,
                "Task cancelled before completion.",
                None,
            )?;
        }
        if let Some(parent_id) = task.parent_id.clone() {
            self.reconcile_parent_task(&app, &parent_id)?;
        }
        Ok(task)
    }

    pub fn pause_lease<R: Runtime>(&self, app: AppHandle<R>, lease_id: &str) -> Result<Lease> {
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
            emit(&app, "task:status-changed", &task)?;
        }
        Ok(lease)
    }

    pub fn resume_task_card<R: Runtime>(
        &self,
        app: AppHandle<R>,
        task_card_id: &str,
    ) -> Result<TaskCard> {
        let mut task = self.storage.get_task_card(task_card_id)?;
        task.status = TaskStatus::Leased;
        self.storage.update_task_card(&task)?;
        if let Some(mut lease) = self.storage.get_lease_by_task(task_card_id)? {
            lease.state = LeaseState::Active;
            lease.preempt_requested_at = None;
            self.storage.update_lease(&lease)?;
            emit(&app, "lease:granted", &lease)?;
        }
        emit(&app, "task:status-changed", &task)?;
        self.spawn_task_execution(app, task.id.clone(), None);
        Ok(task)
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

fn collect_allowed_tools(tool_runtime: &ToolRuntime, agents: &[AgentProfile]) -> Vec<String> {
    let mut ids = Vec::new();
    for agent in agents {
        for tool in tool_runtime.available_tools_for_agent(agent) {
            if !ids.contains(&tool.id) {
                ids.push(tool.id);
            }
        }
    }
    ids
}

fn scored_candidates(tool_runtime: &ToolRuntime, agents: &[AgentProfile]) -> Vec<AgentProfile> {
    agents
        .iter()
        .cloned()
        .map(|mut agent| {
            agent.tool_ids = tool_runtime
                .available_tools_for_agent(&agent)
                .into_iter()
                .map(|tool| tool.id)
                .collect();
            agent
        })
        .collect()
}

fn emit<R: Runtime, T: Serialize>(app: &AppHandle<R>, event: &str, payload: &T) -> Result<()> {
    app.emit(event, payload)
        .map_err(|error| anyhow!("failed to emit {event}: {error}"))
}
