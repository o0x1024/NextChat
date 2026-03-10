use anyhow::Result;
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::domain::{
    new_id, now, AgentProfile, ConversationMessage, MemoryItem, MemoryScope, TaskCard, WorkGroup,
};
use crate::core::memory::{
    build_memory_snapshot, memory_context_for_task, scope_key, writable_scope_enabled,
    HUMAN_MEMORY_SCOPE_ID,
};

impl AppService {
    pub(super) fn seed_work_group_memory(
        &self,
        work_group: &WorkGroup,
        owner: &AgentProfile,
    ) -> Result<()> {
        let content = format!(
            "群聊协作章程\n- 群主: {}\n- 群目标: {}\n- 机制: 群主先拆解任务，再组织成员并行执行，最后汇总交付。",
            owner.name,
            if work_group.goal.trim().is_empty() {
                "待补充"
            } else {
                work_group.goal.trim()
            }
        );
        self.storage.insert_memory_item(&MemoryItem {
            id: new_id(),
            scope: MemoryScope::WorkGroup,
            scope_id: work_group.id.clone(),
            content,
            tags: vec![
                "group_charter".into(),
                "owner".into(),
                "coordination".into(),
            ],
            embedding_ref: None,
            pinned: true,
            ttl: None,
            created_at: now(),
        })?;
        Ok(())
    }

    pub(super) fn remember_human_directive<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        message: &ConversationMessage,
    ) -> Result<()> {
        let memory = MemoryItem {
            id: new_id(),
            scope: MemoryScope::WorkGroup,
            scope_id: message.work_group_id.clone(),
            content: summarize_human_directive(&message.content),
            tags: vec!["human".into(), "directive".into(), "chat_memory".into()],
            embedding_ref: None,
            pinned: false,
            ttl: Some(14 * 24 * 60 * 60),
            created_at: now(),
        };
        self.storage.insert_memory_item(&memory)?;
        emit(
            app,
            "memory:updated",
            &json!({
                "workGroupId": message.work_group_id,
                "memoryId": memory.id,
                "source": "human_directive",
            }),
        )?;
        self.record_audit(
            "memory.human_directive_recorded",
            "work_group",
            &message.work_group_id,
            json!({ "messageId": message.id }),
        )?;
        Ok(())
    }

    pub(super) fn load_memory_context(
        &self,
        agent: &AgentProfile,
        work_group: &WorkGroup,
    ) -> Result<Vec<MemoryItem>> {
        self.storage.cleanup_expired_memory_items()?;
        let items = self.storage.list_memory_items()?;
        Ok(memory_context_for_task(agent, work_group, &items))
    }

    pub(super) fn record_memory_context<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &TaskCard,
        memory_context: &[MemoryItem],
    ) -> Result<()> {
        let snapshot = MemoryItem {
            id: new_id(),
            scope: MemoryScope::Task,
            scope_id: task.id.clone(),
            content: build_memory_snapshot(memory_context),
            tags: vec!["context".into(), "injected".into()],
            embedding_ref: None,
            pinned: false,
            ttl: None,
            created_at: now(),
        };
        self.storage.insert_memory_item(&snapshot)?;
        emit(
            app,
            "memory:updated",
            &json!({ "taskCardId": task.id, "snapshotId": snapshot.id }),
        )?;
        self.record_audit(
            "memory.injected",
            "task_card",
            &task.id,
            json!({
                "memoryIds": memory_context.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
                "count": memory_context.len(),
            }),
        )?;
        Ok(())
    }

    pub(super) fn persist_execution_memory<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &TaskCard,
        agent: &AgentProfile,
        work_group: &WorkGroup,
        summary: &str,
    ) -> Result<()> {
        let writable_targets = [
            (MemoryScope::User, HUMAN_MEMORY_SCOPE_ID.to_string()),
            (MemoryScope::WorkGroup, work_group.id.clone()),
            (MemoryScope::Agent, agent.id.clone()),
        ];
        let mut persisted_scopes = Vec::new();

        for (scope, scope_id) in writable_targets {
            if !writable_scope_enabled(&agent.memory_policy, &scope) {
                continue;
            }

            self.storage.insert_memory_item(&MemoryItem {
                id: new_id(),
                scope: scope.clone(),
                scope_id,
                content: summary.to_string(),
                tags: vec![
                    "summary".into(),
                    "execution".into(),
                    scope_key(&scope).into(),
                    format!("task:{}", task.id),
                ],
                embedding_ref: None,
                pinned: false,
                ttl: None,
                created_at: now(),
            })?;
            persisted_scopes.push(scope_key(&scope).to_string());
        }

        if !persisted_scopes.is_empty() {
            emit(
                app,
                "memory:updated",
                &json!({
                    "agentId": agent.id,
                    "taskCardId": task.id,
                    "scopes": persisted_scopes,
                }),
            )?;
            self.record_audit(
                "memory.persisted",
                "task_card",
                &task.id,
                json!({ "scopes": persisted_scopes }),
            )?;
        }

        Ok(())
    }
}

fn summarize_human_directive(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return "Human directive: (empty)".to_string();
    }
    let truncated = trimmed.chars().take(280).collect::<String>();
    format!("Human directive: {truncated}")
}
