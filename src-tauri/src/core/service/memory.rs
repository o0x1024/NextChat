use anyhow::Result;
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::domain::{
    new_id, now, AgentProfile, MemoryItem, MemoryScope, TaskCard, WorkGroup,
};
use crate::core::memory::{
    build_memory_snapshot, memory_context_for_task, scope_key, writable_scope_enabled,
    HUMAN_MEMORY_SCOPE_ID,
};

impl AppService {
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
