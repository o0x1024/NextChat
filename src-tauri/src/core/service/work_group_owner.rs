use anyhow::Result;

use super::AppService;
use crate::core::domain::{
    AIProviderConfig, AgentPermissionPolicy, AgentProfile, ConversationMessage, CreateAgentInput,
    MemoryPolicy, ModelPolicy, SenderKind,
};

const BUILTIN_GROUP_OWNER_NAME: &str = "群主";
const BUILTIN_GROUP_OWNER_ROLE: &str = "Group Owner";

impl AppService {
    pub(super) fn is_builtin_group_owner_profile(&self, agent: &AgentProfile) -> bool {
        agent.name == BUILTIN_GROUP_OWNER_NAME && agent.role == BUILTIN_GROUP_OWNER_ROLE
    }

    pub(super) fn group_owner_for_work_group(
        &self,
        work_group_id: &str,
    ) -> Result<Option<AgentProfile>> {
        let Some(owner_id) = self.storage.get_work_group_owner_id(work_group_id)? else {
            return Ok(None);
        };
        if let Ok(owner) = self.storage.get_agent(&owner_id) {
            return Ok(Some(owner));
        }
        Ok(None)
    }

    pub(super) fn assign_group_owner_sender(
        &self,
        message: &mut ConversationMessage,
    ) -> Result<()> {
        let Some(owner) = self.group_owner_for_work_group(&message.work_group_id)? else {
            return Ok(());
        };
        message.sender_kind = SenderKind::Agent;
        message.sender_id = owner.id;
        message.sender_name = owner.name;
        Ok(())
    }

    pub(super) fn routing_members_for_message(
        &self,
        work_group_id: &str,
        members: &[AgentProfile],
        mentions: &[String],
    ) -> Result<Vec<AgentProfile>> {
        let owner_agent_id = self.storage.get_work_group_owner_id(work_group_id)?;
        let Some(owner_id) = owner_agent_id.as_deref() else {
            return Ok(members.to_vec());
        };
        let _ = mentions;
        Ok(members
            .iter()
            .filter(|agent| agent.id != owner_id)
            .cloned()
            .collect())
    }

    pub(super) fn ensure_builtin_group_owner(&self) -> Result<AgentProfile> {
        let owner_model_policy = self.resolve_builtin_owner_model_policy()?;
        if let Some(existing) = self
            .storage
            .list_agents()?
            .into_iter()
            .find(|agent| self.is_builtin_group_owner_profile(agent))
        {
            if existing.model_policy.provider == "mock"
                || existing.model_policy.model == "simulation"
            {
                let mut upgraded = existing.clone();
                upgraded.model_policy = owner_model_policy;
                self.storage.insert_agent(&upgraded)?;
                return Ok(upgraded);
            }
            return Ok(existing);
        }

        self.create_agent_profile(CreateAgentInput {
            name: BUILTIN_GROUP_OWNER_NAME.into(),
            avatar: "GM".into(),
            role: BUILTIN_GROUP_OWNER_ROLE.into(),
            objective: "你是内置群主，负责拆解需求、组织群员协作、跟踪阻塞并汇总最终结果。".into(),
            provider: owner_model_policy.provider,
            model: owner_model_policy.model,
            temperature: owner_model_policy.temperature,
            skill_ids: vec![],
            tool_ids: vec![
                "Task".into(),
                "AskUserQuestion".into(),
                "TodoWrite".into(),
                "Skills".into(),
                "Read".into(),
                "Grep".into(),
                "LS".into(),
                "WebSearch".into(),
                "WebFetch".into(),
            ],
            max_parallel_runs: 2,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy {
                read_scope: vec!["user".into(), "work_group".into(), "agent".into()],
                write_scope: vec!["work_group".into(), "agent".into()],
                pinned_memory_ids: vec![],
            },
            permission_policy: AgentPermissionPolicy::default(),
        })
    }

    fn resolve_builtin_owner_model_policy(&self) -> Result<ModelPolicy> {
        let settings = self.storage.get_settings()?;
        if let Some(provider) = settings
            .providers
            .iter()
            .find(|provider| provider.id == settings.global_config.default_llm_provider)
            .filter(|provider| provider_available_for_completion(provider))
        {
            let model = if provider
                .models
                .iter()
                .any(|item| item == &settings.global_config.default_llm_model)
            {
                settings.global_config.default_llm_model.clone()
            } else if provider.models.contains(&provider.default_model) {
                provider.default_model.clone()
            } else {
                provider
                    .models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| settings.global_config.default_llm_model.clone())
            };
            return Ok(ModelPolicy {
                provider: provider.id.clone(),
                model,
                temperature: provider.temperature,
            });
        }

        Ok(ModelPolicy::default())
    }
}

fn provider_available_for_completion(provider: &AIProviderConfig) -> bool {
    provider.enabled
        && !provider.models.is_empty()
        && (provider.rig_provider_type == "Ollama" || !provider.api_key.trim().is_empty())
}
