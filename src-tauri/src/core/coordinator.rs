use std::collections::HashMap;

use anyhow::Result;
use regex::Regex;
use serde_json::json;

use crate::core::domain::{
    new_id, now, AgentProfile, ClaimBid, ClaimContext, ClaimPlan, ClaimScorer, ConversationMessage,
    Lease, LeaseState, MessageKind, SenderKind, TaskStatus, ToolManifest, Visibility,
};

#[derive(Debug, Clone)]
pub struct Coordinator;

impl Coordinator {
    pub fn extract_mentions(content: &str, agents: &[AgentProfile]) -> Vec<String> {
        let regex = Regex::new(r"@([A-Za-z0-9_\-\u4e00-\u9fa5]+)").expect("mention regex");
        let mut mentions = Vec::new();
        for capture in regex.captures_iter(content) {
            if let Some(found) = capture.get(1) {
                let token = found.as_str().to_lowercase();
                if let Some(agent) = agents
                    .iter()
                    .find(|agent| agent.name.to_lowercase() == token || agent.id == token)
                {
                    mentions.push(agent.id.clone());
                }
            }
        }
        mentions
    }

    pub fn build_task_title(content: &str) -> String {
        let clean = content.trim().replace('\n', " ");
        clean.chars().take(72).collect()
    }

    fn score_candidate(
        &self,
        agent: &AgentProfile,
        content: &str,
        mentioned_agent_ids: &[String],
        active_load: i64,
        requested_tool: &Option<ToolManifest>,
    ) -> (f64, Vec<String>, String) {
        let lowered = content.to_lowercase();
        let role_lower = agent.role.to_lowercase();
        let objective_lower = agent.objective.to_lowercase();
        let mut score = 10.0;

        if mentioned_agent_ids.contains(&agent.id) {
            score += 35.0;
        }

        if active_load >= agent.max_parallel_runs {
            score -= 40.0;
        } else {
            score += ((agent.max_parallel_runs - active_load).max(0) as f64) * 2.5;
        }

        for keyword in lowered.split_whitespace() {
            if role_lower.contains(keyword) || objective_lower.contains(keyword) {
                score += 3.0;
            }
        }

        let mut expected_tools = Vec::new();
        if let Some(tool) = requested_tool {
            if agent.tool_ids.contains(&tool.id) {
                score += 20.0;
                expected_tools.push(tool.id.clone());
            } else {
                score -= 8.0;
            }
        }

        for skill in &agent.skill_ids {
            if lowered.contains("research") && skill.contains("research") {
                score += 8.0;
            }
            if lowered.contains("build") && skill.contains("builder") {
                score += 8.0;
            }
            if lowered.contains("review") && skill.contains("reviewer") {
                score += 8.0;
            }
        }

        score -= (active_load as f64) * 12.0;
        let rationale = format!(
            "{} matched with load {} / parallel limit {} and tools {:?}",
            agent.role, active_load, agent.max_parallel_runs, expected_tools
        );
        (score, expected_tools, rationale)
    }
}

#[cfg(test)]
mod tests {
    use super::Coordinator;
    use crate::core::domain::{
        AgentProfile, ClaimContext, ClaimScorer, MemoryPolicy, ModelPolicy, TaskCard, TaskStatus,
        ToolManifest, WorkGroup, WorkGroupKind,
    };

    fn agent(id: &str, name: &str, role: &str) -> AgentProfile {
        AgentProfile {
            id: id.into(),
            name: name.into(),
            avatar: name.chars().take(2).collect(),
            role: role.into(),
            objective: role.into(),
            model_policy: ModelPolicy::default(),
            skill_ids: vec![],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
        }
    }

    fn work_group() -> WorkGroup {
        WorkGroup {
            id: "wg-1".into(),
            kind: WorkGroupKind::Persistent,
            name: "WG".into(),
            goal: "Goal".into(),
            member_agent_ids: vec!["a1".into(), "a2".into()],
            default_visibility: "summary".into(),
            auto_archive: false,
            created_at: "now".into(),
            archived_at: None,
        }
    }

    fn task_card() -> TaskCard {
        TaskCard {
            id: "task-1".into(),
            parent_id: None,
            source_message_id: "msg-1".into(),
            title: "Review a plan".into(),
            normalized_goal: "Review a plan".into(),
            input_payload: "Review a plan".into(),
            priority: 100,
            status: TaskStatus::Bidding,
            work_group_id: "wg-1".into(),
            created_by: "human".into(),
            assigned_agent_id: None,
            created_at: "now".into(),
        }
    }

    #[test]
    fn mentioned_agent_wins_bid() {
        let scorer = Coordinator;
        let scout = agent("a1", "Scout", "Research Lead");
        let reviewer = agent("a2", "Reviewer", "Quality Reviewer");

        let plan = scorer
            .score(ClaimContext {
                task_card: task_card(),
                work_group: work_group(),
                candidates: vec![scout, reviewer.clone()],
                content: "@Reviewer please review the rollout".into(),
                mentioned_agent_ids: vec![reviewer.id.clone()],
                active_loads: vec![("a1".into(), 0), ("a2".into(), 0)],
                requested_tool: None,
            })
            .expect("score plan");

        assert_eq!(
            plan.lease.expect("lease").owner_agent_id,
            reviewer.id,
            "mentioned agent should win the lease"
        );
    }

    #[test]
    fn no_candidates_leaves_task_pending() {
        let scorer = Coordinator;
        let plan = scorer
            .score(ClaimContext {
                task_card: task_card(),
                work_group: work_group(),
                candidates: vec![],
                content: "Do work".into(),
                mentioned_agent_ids: vec![],
                active_loads: vec![],
                requested_tool: None,
            })
            .expect("score plan");

        assert!(plan.lease.is_none());
        assert_eq!(plan.task_card.status, TaskStatus::Pending);
    }

    #[test]
    fn tool_coverage_changes_winner() {
        let scorer = Coordinator;
        let scout = agent("a1", "Scout", "Research Lead");
        let mut builder = agent("a2", "Builder", "Systems Engineer");
        builder.tool_ids = vec!["shell.exec".into(), "plan.summarize".into()];

        let plan = scorer
            .score(ClaimContext {
                task_card: task_card(),
                work_group: work_group(),
                candidates: vec![scout, builder.clone()],
                content: "Run shell command to inspect the project".into(),
                mentioned_agent_ids: vec![],
                active_loads: vec![("a1".into(), 0), ("a2".into(), 0)],
                requested_tool: Some(ToolManifest {
                    id: "shell.exec".into(),
                    name: "Shell Command".into(),
                    category: "system".into(),
                    risk_level: crate::core::domain::ToolRiskLevel::High,
                    input_schema: "{}".into(),
                    output_schema: "{}".into(),
                    timeout_ms: 1000,
                    concurrency_limit: 1,
                    permissions: vec!["system:shell".into()],
                    description: "Run shell".into(),
                }),
            })
            .expect("score plan");

        assert_eq!(plan.lease.expect("lease").owner_agent_id, builder.id);
    }
}

impl ClaimScorer for Coordinator {
    fn score(&self, context: ClaimContext) -> Result<ClaimPlan> {
        let ClaimContext {
            mut task_card,
            work_group,
            candidates,
            content,
            mentioned_agent_ids,
            active_loads,
            requested_tool,
        } = context;

        let created_at = now();
        let load_map: HashMap<String, i64> = active_loads.into_iter().collect();

        let mut bids = Vec::new();
        for candidate in &candidates {
            let (capability_score, expected_tools, rationale) = self.score_candidate(
                candidate,
                &content,
                &mentioned_agent_ids,
                *load_map.get(&candidate.id).unwrap_or(&0),
                &requested_tool,
            );
            bids.push(ClaimBid {
                id: new_id(),
                task_card_id: task_card.id.clone(),
                agent_id: candidate.id.clone(),
                rationale,
                capability_score,
                expected_tools,
                estimated_cost: (task_card.priority as f64) * 0.4 + 1.0,
                created_at: created_at.clone(),
            });
        }

        bids.sort_by(|left, right| right.capability_score.total_cmp(&left.capability_score));

        let lease = bids.first().map(|top_bid| Lease {
            id: new_id(),
            task_card_id: task_card.id.clone(),
            owner_agent_id: top_bid.agent_id.clone(),
            state: LeaseState::Active,
            granted_at: created_at.clone(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        });

        task_card.assigned_agent_id = lease.as_ref().map(|item| item.owner_agent_id.clone());
        task_card.status = if lease.is_some() {
            TaskStatus::Leased
        } else {
            TaskStatus::Pending
        };

        let mut coordinator_messages = vec![ConversationMessage {
            id: new_id(),
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content: if let Some(ref active_lease) = lease {
                let winner = candidates
                    .iter()
                    .find(|agent| agent.id == active_lease.owner_agent_id)
                    .map(|agent| agent.name.as_str())
                    .unwrap_or("Unknown");
                if let Some(tool) = requested_tool.as_ref() {
                    format!(
                        "Task card created and leased to {winner} using {}.",
                        tool.name
                    )
                } else {
                    format!("Task card created and leased to {winner}.")
                }
            } else {
                "Task card created, but no eligible agent claimed it.".into()
            },
            mentions: vec![],
            task_card_id: Some(task_card.id.clone()),
            created_at: created_at.clone(),
        }];

        for bid in &bids {
            let bidder_name = candidates
                .iter()
                .find(|agent| agent.id == bid.agent_id)
                .map(|agent| agent.name.as_str())
                .unwrap_or(bid.agent_id.as_str());
            coordinator_messages.push(ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::System,
                sender_id: "coordinator".into(),
                sender_name: "Coordinator".into(),
                kind: MessageKind::Status,
                visibility: Visibility::Backstage,
                content: format!(
                    "Bid from {bidder_name}: {:.1} points. {}",
                    bid.capability_score, bid.rationale
                ),
                mentions: vec![bid.agent_id.clone()],
                task_card_id: Some(task_card.id.clone()),
                created_at: created_at.clone(),
            });
        }

        let payload = json!({
            "taskCardId": task_card.id,
            "candidateCount": candidates.len(),
            "mentionedAgentIds": mentioned_agent_ids,
        });
        coordinator_messages.push(ConversationMessage {
            id: new_id(),
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind: MessageKind::Summary,
            visibility: Visibility::Backstage,
            content: payload.to_string(),
            mentions: vec![],
            task_card_id: Some(task_card.id.clone()),
            created_at,
        });

        Ok(ClaimPlan {
            task_card,
            bids,
            lease,
            coordinator_messages,
            requested_tool,
        })
    }
}
