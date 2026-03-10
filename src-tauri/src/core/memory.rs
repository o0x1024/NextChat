use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};

use crate::core::domain::{AgentProfile, MemoryItem, MemoryPolicy, MemoryScope, WorkGroup};

pub const HUMAN_MEMORY_SCOPE_ID: &str = "human";
const MAX_CONTEXT_MEMORY_ITEMS: usize = 8;

pub fn scope_key(scope: &MemoryScope) -> &'static str {
    match scope {
        MemoryScope::User => "user",
        MemoryScope::WorkGroup => "work_group",
        MemoryScope::Agent => "agent",
        MemoryScope::Task => "task",
    }
}

pub fn scope_enabled(field: &[String], scope: &MemoryScope) -> bool {
    field.iter().any(|candidate| candidate == scope_key(scope))
}

pub fn memory_is_expired(item: &MemoryItem) -> bool {
    memory_is_expired_at(item, Utc::now())
}

pub fn memory_is_expired_at(item: &MemoryItem, reference_time: DateTime<Utc>) -> bool {
    let Some(ttl_seconds) = item.ttl else {
        return false;
    };
    if ttl_seconds <= 0 {
        return true;
    }
    let Ok(created_at) = DateTime::parse_from_rfc3339(&item.created_at) else {
        return false;
    };
    created_at
        .with_timezone(&Utc)
        .checked_add_signed(Duration::seconds(ttl_seconds))
        .map(|expires_at| expires_at <= reference_time)
        .unwrap_or(false)
}

pub fn filter_active_memory(items: Vec<MemoryItem>) -> Vec<MemoryItem> {
    items
        .into_iter()
        .filter(|item| !memory_is_expired(item))
        .collect()
}

pub fn memory_context_for_task(
    agent: &AgentProfile,
    work_group: &WorkGroup,
    items: &[MemoryItem],
) -> Vec<MemoryItem> {
    let pinned_ids = agent
        .memory_policy
        .pinned_memory_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let mut candidates = items
        .iter()
        .filter(|item| !memory_is_expired(item))
        .filter(|item| pinned_ids.contains(&item.id) || readable_for_agent(agent, work_group, item))
        .cloned()
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        pinned_ids
            .contains(&right.id)
            .cmp(&pinned_ids.contains(&left.id))
            .then_with(|| right.pinned.cmp(&left.pinned))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });

    let pinned_count = candidates
        .iter()
        .filter(|item| pinned_ids.contains(&item.id))
        .count();
    candidates.truncate(MAX_CONTEXT_MEMORY_ITEMS.max(pinned_count));
    candidates
}

pub fn writable_scope_enabled(policy: &MemoryPolicy, scope: &MemoryScope) -> bool {
    scope_enabled(&policy.write_scope, scope)
}

pub fn build_memory_snapshot(items: &[MemoryItem]) -> String {
    if items.is_empty() {
        return "No memory injected.".to_string();
    }

    items
        .iter()
        .map(|item| {
            let mut line = format!(
                "[{}] {}",
                scope_key(&item.scope),
                truncate(item.content.trim(), 160)
            );
            if !item.tags.is_empty() {
                line.push_str(&format!(" | tags: {}", item.tags.join(", ")));
            }
            if item.pinned {
                line.push_str(" | pinned");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn readable_for_agent(agent: &AgentProfile, work_group: &WorkGroup, item: &MemoryItem) -> bool {
    match item.scope {
        MemoryScope::User => {
            scope_enabled(&agent.memory_policy.read_scope, &MemoryScope::User)
                && item.scope_id == HUMAN_MEMORY_SCOPE_ID
        }
        MemoryScope::WorkGroup => {
            scope_enabled(&agent.memory_policy.read_scope, &MemoryScope::WorkGroup)
                && item.scope_id == work_group.id
        }
        MemoryScope::Agent => {
            scope_enabled(&agent.memory_policy.read_scope, &MemoryScope::Agent)
                && item.scope_id == agent.id
        }
        MemoryScope::Task => false,
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::{memory_context_for_task, memory_is_expired_at, writable_scope_enabled};
    use crate::core::domain::{
        AgentPermissionPolicy, AgentProfile, MemoryItem, MemoryPolicy, MemoryScope, ModelPolicy,
        WorkGroup, WorkGroupKind,
    };
    use chrono::{Duration, Utc};

    fn agent() -> AgentProfile {
        AgentProfile {
            id: "agent-1".into(),
            name: "Scout".into(),
            avatar: "SC".into(),
            role: "Research".into(),
            objective: "Find facts".into(),
            model_policy: ModelPolicy::default(),
            skill_ids: vec![],
            tool_ids: vec![],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy {
                read_scope: vec!["agent".into()],
                write_scope: vec!["work_group".into()],
                pinned_memory_ids: vec!["wg-pin".into()],
            },
            permission_policy: AgentPermissionPolicy::default(),
        }
    }

    fn work_group() -> WorkGroup {
        WorkGroup {
            id: "wg-1".into(),
            kind: WorkGroupKind::Persistent,
            name: "WG".into(),
            goal: "Goal".into(),
            working_directory: ".".into(),
            member_agent_ids: vec!["agent-1".into()],
            default_visibility: "summary".into(),
            auto_archive: false,
            created_at: "now".into(),
            archived_at: None,
        }
    }

    #[test]
    fn pinned_memory_can_override_read_scope() {
        let context = memory_context_for_task(
            &agent(),
            &work_group(),
            &[
                MemoryItem {
                    id: "wg-pin".into(),
                    scope: MemoryScope::WorkGroup,
                    scope_id: "wg-1".into(),
                    content: "Shared launch constraints".into(),
                    tags: vec!["policy".into()],
                    embedding_ref: None,
                    pinned: true,
                    ttl: None,
                    created_at: Utc::now().to_rfc3339(),
                },
                MemoryItem {
                    id: "agent-note".into(),
                    scope: MemoryScope::Agent,
                    scope_id: "agent-1".into(),
                    content: "Agent note".into(),
                    tags: vec![],
                    embedding_ref: None,
                    pinned: false,
                    ttl: None,
                    created_at: Utc::now().to_rfc3339(),
                },
            ],
        );

        assert_eq!(context.len(), 2);
        assert_eq!(context[0].id, "wg-pin");
    }

    #[test]
    fn ttl_marks_expired_memory() {
        let created_at = (Utc::now() - Duration::minutes(10)).to_rfc3339();
        let item = MemoryItem {
            id: "temp".into(),
            scope: MemoryScope::Agent,
            scope_id: "agent-1".into(),
            content: "Temp".into(),
            tags: vec![],
            embedding_ref: None,
            pinned: false,
            ttl: Some(60),
            created_at,
        };

        assert!(memory_is_expired_at(&item, Utc::now()));
    }

    #[test]
    fn write_scope_respects_policy() {
        assert!(writable_scope_enabled(
            &agent().memory_policy,
            &MemoryScope::WorkGroup
        ));
        assert!(!writable_scope_enabled(
            &agent().memory_policy,
            &MemoryScope::Agent
        ));
    }
}
