mod agent_generation;
mod agent_narrative_orchestration;
mod blockers;
#[cfg(test)]
mod clear_history_tests;
mod collaboration;
mod direct_routing;
mod execution_payloads;
#[cfg(test)]
mod group_owner_memory_tests;
mod memory;
mod narrative_messages;
mod owner_blocker_orchestration;
mod owner_orchestration;
mod routing;
mod runtime;
mod runtime_recovery;
mod summary_stream;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod todo_fallback_tests;
mod tool_stream;
mod work_group_owner;
use crate::core::{
    agent_runtime::AgentRuntime,
    coordinator::Coordinator,
    domain::{
        new_id, now, AIProviderConfig, AgentProfile, AuditEvent, ConversationMessage,
        CreateAgentInput, CreateWorkGroupInput, DashboardState, Lease, LeaseState, MessageKind,
        ModelPolicy, SenderKind, SystemSettings, TaskCard, TaskStatus, ToolRun, ToolRunState,
        UpdateAgentInput, UpdateWorkGroupInput, Visibility, WorkGroup,
    },
    llm_rig::{refresh_models, RigModelAdapter},
    storage::Storage,
    tool_runtime::ToolRuntime,
};
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::json;
use std::{collections::HashSet, future::Future, sync::Arc};
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Clone)]
pub struct AppService {
    storage: Storage,
    coordinator: Coordinator,
    tool_runtime: Arc<ToolRuntime>,
    agent_runtime: Arc<AgentRuntime<RigModelAdapter, ToolRuntime>>,
}

fn block_on_service_future<F>(future: F) -> F::Output
where
    F: Future,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| tauri::async_runtime::block_on(future))
    } else {
        tauri::async_runtime::block_on(future)
    }
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
        state.skills = self.tool_runtime.all_skills();
        Ok(state)
    }

    pub fn install_skill_from_local_path(
        &self,
        source_path: &str,
    ) -> Result<Vec<crate::core::domain::SkillPack>> {
        let skills = self
            .tool_runtime
            .install_skill_from_local_path(source_path)?;
        for skill in &skills {
            self.record_audit(
                "skill.installed.local",
                "skill",
                &skill.id,
                json!({ "sourcePath": source_path, "name": skill.name }),
            )?;
        }
        Ok(skills)
    }

    pub async fn install_skill_from_github(
        &self,
        source: &str,
        skill_path: Option<&str>,
    ) -> Result<Vec<crate::core::domain::SkillPack>> {
        let skills = self
            .tool_runtime
            .install_skill_from_github(source, skill_path)
            .await?;
        for skill in &skills {
            self.record_audit(
                "skill.installed.github",
                "skill",
                &skill.id,
                json!({ "source": source, "path": skill_path, "name": skill.name }),
            )?;
        }
        Ok(skills)
    }

    pub fn update_installed_skill(
        &self,
        skill_id: &str,
        name: Option<String>,
        prompt_template: Option<String>,
    ) -> Result<crate::core::domain::SkillPack> {
        let skill = self
            .tool_runtime
            .update_installed_skill(skill_id, name, prompt_template)?;
        self.record_audit(
            "skill.updated",
            "skill",
            &skill.id,
            json!({ "name": skill.name, "enabled": skill.enabled }),
        )?;
        Ok(skill)
    }

    pub fn set_installed_skill_enabled(
        &self,
        skill_id: &str,
        enabled: bool,
    ) -> Result<crate::core::domain::SkillPack> {
        let skill = self
            .tool_runtime
            .set_installed_skill_enabled(skill_id, enabled)?;
        self.record_audit(
            "skill.toggled",
            "skill",
            &skill.id,
            json!({ "enabled": enabled }),
        )?;
        Ok(skill)
    }

    pub fn delete_installed_skill(&self, skill_id: &str) -> Result<()> {
        self.tool_runtime.delete_installed_skill(skill_id)?;
        self.record_audit("skill.deleted", "skill", skill_id, json!({}))?;
        Ok(())
    }

    pub fn get_installed_skill_detail(
        &self,
        skill_id: &str,
    ) -> Result<crate::core::domain::SkillDetail> {
        self.tool_runtime.get_installed_skill_detail(skill_id)
    }

    pub fn update_skill_detail(
        &self,
        input: crate::core::domain::UpdateSkillDetailInput,
    ) -> Result<crate::core::domain::SkillDetail> {
        let detail = self.tool_runtime.update_skill_detail(input)?;
        self.record_audit(
            "skill.detail.updated",
            "skill",
            &detail.skill_id,
            json!({ "name": detail.name, "enabled": detail.enabled }),
        )?;
        Ok(detail)
    }

    pub fn read_installed_skill_file(&self, skill_id: &str, relative_path: &str) -> Result<String> {
        self.tool_runtime
            .read_installed_skill_file(skill_id, relative_path)
    }

    pub fn upsert_installed_skill_file(
        &self,
        skill_id: &str,
        relative_path: &str,
        content: &str,
    ) -> Result<()> {
        self.tool_runtime
            .upsert_installed_skill_file(skill_id, relative_path, content)?;
        self.record_audit(
            "skill.file.upserted",
            "skill",
            skill_id,
            json!({ "path": relative_path }),
        )?;
        Ok(())
    }

    pub fn delete_installed_skill_file(&self, skill_id: &str, relative_path: &str) -> Result<()> {
        self.tool_runtime
            .delete_installed_skill_file(skill_id, relative_path)?;
        self.record_audit(
            "skill.file.deleted",
            "skill",
            skill_id,
            json!({ "path": relative_path }),
        )?;
        Ok(())
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
        let agent = self
            .storage
            .get_agent(agent_id)
            .with_context(|| format!("agent not found: {}", agent_id))?;
        if self.is_builtin_group_owner_profile(&agent) {
            return Err(anyhow!("cannot delete builtin group owner agent"));
        }

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
        let CreateWorkGroupInput {
            name,
            goal,
            working_directory,
            kind,
            default_visibility,
            auto_archive,
            member_agent_ids,
        } = input;
        let working_directory = self
            .tool_runtime
            .normalize_working_directory(&working_directory)?;
        let known_agent_ids = self
            .storage
            .list_agents()?
            .into_iter()
            .map(|agent| agent.id)
            .collect::<HashSet<_>>();
        let mut member_agent_ids_normalized = Vec::new();
        let mut seen = HashSet::new();
        for agent_id in member_agent_ids.unwrap_or_default() {
            if !known_agent_ids.contains(&agent_id) {
                return Err(anyhow!("agent not found: {}", agent_id));
            }
            if seen.insert(agent_id.clone()) {
                member_agent_ids_normalized.push(agent_id);
            }
        }
        let owner_agent = self.ensure_builtin_group_owner()?;
        member_agent_ids_normalized.insert(0, owner_agent.id.clone());
        let group = WorkGroup {
            id: new_id(),
            kind,
            name,
            goal,
            working_directory,
            member_agent_ids: member_agent_ids_normalized,
            default_visibility,
            auto_archive,
            created_at: now(),
            archived_at: None,
        };
        self.storage.insert_work_group(&group)?;
        self.storage
            .set_work_group_owner(&group.id, &owner_agent.id)?;
        self.seed_work_group_memory(&group, &owner_agent)?;
        self.record_audit(
            "work_group.created",
            "work_group",
            &group.id,
            json!({
                "name": group.name,
                "workingDirectory": group.working_directory,
                "ownerAgentId": owner_agent.id,
            }),
        )?;
        Ok(group)
    }

    pub fn delete_work_group(&self, work_group_id: &str) -> Result<()> {
        if self.storage.list_work_groups()?.len() <= 1 {
            return Err(anyhow!("cannot delete the last work group"));
        }
        self.storage.delete_work_group(work_group_id)?;
        self.record_audit("work_group.deleted", "work_group", work_group_id, json!({}))?;
        Ok(())
    }

    pub fn clear_work_group_history(&self, work_group_id: &str) -> Result<()> {
        let active_leases = self.storage.list_active_leases_for_group(work_group_id)?;
        if !active_leases.is_empty() {
            return Err(anyhow!(
                "cannot clear history while active tasks are running"
            ));
        }
        self.storage.clear_work_group_history(work_group_id)?;
        self.record_audit(
            "work_group.history_cleared",
            "work_group",
            work_group_id,
            json!({}),
        )?;
        Ok(())
    }

    pub fn update_work_group(&self, input: UpdateWorkGroupInput) -> Result<WorkGroup> {
        let working_directory = self
            .tool_runtime
            .normalize_working_directory(&input.working_directory)?;
        let mut group = self.storage.get_work_group(&input.id)?;
        group.kind = input.kind;
        group.name = input.name;
        group.goal = input.goal;
        group.working_directory = working_directory;
        group.default_visibility = input.default_visibility;
        group.auto_archive = input.auto_archive;
        self.storage.insert_work_group(&group)?;
        self.record_audit(
            "work_group.updated",
            "work_group",
            &group.id,
            json!({ "name": group.name, "workingDirectory": group.working_directory }),
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
    ) -> Result<ConversationMessage> {
        let mut message = ConversationMessage {
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
        };
        self.assign_group_owner_sender(&mut message)?;
        Ok(message)
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
            self.build_permission_denied_message(work_group_id, &task.id, agent, tool, reason)?;
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

fn should_skip_implicit_todowrite_fallback(
    content: &str,
    tool: &crate::core::domain::ToolManifest,
) -> bool {
    if tool.id != "TodoWrite" {
        return false;
    }
    let lowered = content.to_lowercase();
    let explicit_todo_request = ["todo", "task list", "待办"]
        .iter()
        .any(|keyword| lowered.contains(keyword));
    if explicit_todo_request {
        return false;
    }
    true
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
