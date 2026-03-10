use anyhow::Result;

use super::AppService;
use crate::core::domain::{
    AgentPermissionPolicy, AgentProfile, ConversationMessage, CreateAgentInput, MemoryPolicy,
    ModelPolicy, SenderKind,
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
        if let Some(existing) = self
            .storage
            .list_agents()?
            .into_iter()
            .find(|agent| self.is_builtin_group_owner_profile(agent))
        {
            return Ok(existing);
        }

        let model_policy = ModelPolicy::default();

        self.create_agent_profile(CreateAgentInput {
            name: BUILTIN_GROUP_OWNER_NAME.into(),
            avatar: "GM".into(),
            role: BUILTIN_GROUP_OWNER_ROLE.into(),
            objective: "你是内置群主，负责拆解需求、组织群员协作、跟踪阻塞并汇总最终结果。".into(),
            provider: model_policy.provider,
            model: model_policy.model,
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec![
                "Task".into(),
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
}
