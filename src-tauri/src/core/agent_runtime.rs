use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use crate::core::domain::{
    AgentExecution, AgentExecutor, ExecutionMode, ModelProviderAdapter, TaskExecutionContext,
    ToolExecutionRequest, ToolHandler,
};
use crate::core::llm_rig::complete_task_with_tools;

#[derive(Clone)]
pub struct AgentRuntime<TModel, TTool> {
    model_adapter: Arc<TModel>,
    tool_handler: Arc<TTool>,
}

impl<TModel, TTool> AgentRuntime<TModel, TTool> {
    pub fn new(model_adapter: Arc<TModel>, tool_handler: Arc<TTool>) -> Self {
        Self {
            model_adapter,
            tool_handler,
        }
    }
}

#[async_trait]
impl<TModel, TTool> AgentExecutor for AgentRuntime<TModel, TTool>
where
    TModel: ModelProviderAdapter + 'static,
    TTool: ToolHandler + 'static,
{
    async fn execute_task(&self, context: TaskExecutionContext) -> Result<AgentExecution> {
        let skill_summary = if context.available_skills.is_empty() {
            "No extra skills enabled.".to_string()
        } else {
            context
                .available_skills
                .iter()
                .map(|skill| skill.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        };

        let available_tool_summary = if context.available_tools.is_empty() {
            "No tools enabled.".to_string()
        } else {
            context
                .available_tools
                .iter()
                .map(|tool| format!("{} ({})", tool.id, tool.description))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let planning_rules = context
            .available_skills
            .iter()
            .flat_map(|skill| skill.planning_rules.iter().cloned())
            .collect::<Vec<_>>();
        let done_criteria = context
            .available_skills
            .iter()
            .flat_map(|skill| skill.done_criteria.iter().cloned())
            .collect::<Vec<_>>();

        let preamble = format!(
            "You are {}. Role: {}. Objective: {}. Skills: {}. Available tools: {}. Use tools when they materially improve accuracy. If a high-risk tool is not exposed, do not imply that it was used.",
            context.agent.name,
            context.agent.role,
            context.agent.objective,
            skill_summary,
            available_tool_summary
        );
        let prompt = format!(
            "Work group goal: {}\nTask: {}\nConversation items:\n{}\nPlanning rules:\n{}\nDone criteria:\n{}\nReturn a concise progress summary. If you used tools, cite the concrete outcome instead of generic statements.",
            context.work_group.goal,
            context.task_card.normalized_goal,
            context
                .conversation_window
                .iter()
                .rev()
                .take(6)
                .map(|message| format!("{}: {}", message.sender_name, message.content))
                .collect::<Vec<_>>()
                .join("\n"),
            if planning_rules.is_empty() {
                "- None".to_string()
            } else {
                planning_rules
                    .iter()
                    .map(|rule| format!("- {rule}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            if done_criteria.is_empty() {
                "- Provide the best available answer".to_string()
            } else {
                done_criteria
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        );

        let rig_execution =
            complete_task_with_tools(&context, self.tool_handler.clone(), &preamble, &prompt)
                .await?;

        let legacy_tool_output = if rig_execution.is_none() {
            if let Some(tool) = context.approved_tool.clone() {
                Some(
                    self.tool_handler
                        .execute(ToolExecutionRequest {
                            tool,
                            input: context.task_card.input_payload.clone(),
                            task_card_id: context.task_card.id.clone(),
                            agent_id: context.agent.id.clone(),
                        })
                        .await?
                        .output,
                )
            } else {
                None
            }
        } else {
            None
        };

        let model_summary = if let Some(result) = rig_execution.as_ref() {
            Some(result.summary.clone())
        } else {
            self.model_adapter
                .complete(&context.agent.model_policy, &context.settings, &preamble, &prompt)
                .await?
        };

        let tool_output = if let Some(result) = rig_execution.as_ref() {
            if result.tool_events.is_empty() {
                None
            } else {
                Some(
                    serde_json::json!({
                        "toolCalls": result.tool_events,
                    })
                    .to_string(),
                )
            }
        } else {
            legacy_tool_output
        };

        let tool_event_summary = rig_execution
            .as_ref()
            .map(|result| {
                result
                    .tool_events
                    .iter()
                    .map(|event| event.tool_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let fallback_summary = if !tool_event_summary.is_empty() {
            Some(format!(
                "{} used {} while working on '{}'.",
                context.agent.name,
                tool_event_summary.join(", "),
                context.task_card.title
            ))
        } else if let Some(tool) = context.approved_tool.as_ref() {
            Some(format!(
                "{} used {} to work on '{}'.",
                context.agent.name, tool.name, context.task_card.title
            ))
        } else {
            Some(format!(
                "{} accepted the task and focused on: {}.",
                context.agent.name, context.task_card.title
            ))
        };
        let used_real_model = model_summary.is_some();
        let summary = model_summary.or(fallback_summary).unwrap_or_default();
        let execution_mode = if used_real_model {
            ExecutionMode::RealModel
        } else {
            ExecutionMode::Fallback
        };

        let suggested_subtasks = build_suggested_subtasks(&context);

        let backstage_notes = format!(
            "Skills used: {}. Tools exposed: {}. Tools called: {}. Suggested collaborators: {}.",
            skill_summary,
            context
                .available_tools
                .iter()
                .map(|tool| tool.id.clone())
                .collect::<Vec<_>>()
                .join(", "),
            if tool_event_summary.is_empty() {
                context
                    .approved_tool
                    .as_ref()
                    .map(|tool| tool.id.clone())
                    .unwrap_or_else(|| "None".to_string())
            } else {
                tool_event_summary.join(", ")
            },
            if suggested_subtasks.is_empty() {
                "None".to_string()
            } else {
                suggested_subtasks.join(" | ")
            }
        );

        Ok(AgentExecution {
            summary,
            backstage_notes,
            suggested_subtasks,
            tool_output,
            execution_mode,
        })
    }
}

fn build_suggested_subtasks(context: &TaskExecutionContext) -> Vec<String> {
    if !context.agent.can_spawn_subtasks || context.task_card.parent_id.is_some() {
        return Vec::new();
    }

    let lowered = context.task_card.input_payload.to_lowercase();
    let mention_regex = Regex::new(r"@([A-Za-z0-9_\-\u4e00-\u9fa5]+)").expect("mention regex");
    let mentioned_agents = mention_regex
        .captures_iter(&context.task_card.input_payload)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_lowercase()))
        .filter_map(|token| {
            context.work_group_members.iter().find(|agent| {
                agent.id != context.agent.id
                    && (agent.name.to_lowercase() == token || agent.id == token)
            })
        })
        .map(|agent| agent.name.clone())
        .collect::<Vec<_>>();

    let mut subtasks = mentioned_agents
        .into_iter()
        .map(|name| {
            format!(
                "@{name} collaborate on '{}' and return a concise update for the parent task.",
                context.task_card.title
            )
        })
        .collect::<Vec<_>>();

    if subtasks.is_empty()
        && (lowered.contains("parallel") || lowered.contains("并行") || lowered.contains("并且"))
    {
        if let Some(agent) = context
            .work_group_members
            .iter()
            .find(|candidate| candidate.id != context.agent.id)
        {
            subtasks.push(format!(
                "@{} review '{}' in parallel and report only key findings.",
                agent.name, context.task_card.title
            ));
        }
    }

    subtasks.truncate(2);
    subtasks
}

#[cfg(test)]
mod tests {
    use super::build_suggested_subtasks;
    use crate::core::domain::{
        AgentProfile, MemoryPolicy, ModelPolicy, TaskCard, TaskExecutionContext, TaskStatus,
        ToolManifest, WorkGroup, WorkGroupKind,
    };

    fn agent(id: &str, name: &str) -> AgentProfile {
        AgentProfile {
            id: id.into(),
            name: name.into(),
            avatar: name.chars().take(2).collect(),
            role: "Engineer".into(),
            objective: "Ship".into(),
            model_policy: ModelPolicy::default(),
            skill_ids: vec![],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
        }
    }

    fn context(input_payload: &str) -> TaskExecutionContext {
        let scout = agent("a1", "Scout");
        let reviewer = agent("a2", "Reviewer");
        TaskExecutionContext {
            agent: scout.clone(),
            work_group: WorkGroup {
                id: "wg-1".into(),
                kind: WorkGroupKind::Persistent,
                name: "WG".into(),
                goal: "Goal".into(),
                member_agent_ids: vec![scout.id.clone(), reviewer.id.clone()],
                default_visibility: "summary".into(),
                auto_archive: false,
                created_at: "now".into(),
                archived_at: None,
            },
            work_group_members: vec![scout, reviewer],
            task_card: TaskCard {
                id: "task-1".into(),
                parent_id: None,
                source_message_id: "msg-1".into(),
                title: "Draft release notes".into(),
                normalized_goal: input_payload.into(),
                input_payload: input_payload.into(),
                priority: 100,
                status: TaskStatus::InProgress,
                work_group_id: "wg-1".into(),
                created_by: "human".into(),
                assigned_agent_id: Some("a1".into()),
                created_at: "now".into(),
            },
            conversation_window: vec![],
            available_tools: vec![ToolManifest {
                id: "plan.summarize".into(),
                name: "Plan".into(),
                category: "coordination".into(),
                risk_level: crate::core::domain::ToolRiskLevel::Low,
                input_schema: "{}".into(),
                output_schema: "{}".into(),
                timeout_ms: 1000,
                concurrency_limit: 1,
                permissions: vec![],
                description: "Plan".into(),
            }],
            available_skills: vec![],
            approved_tool: None,
            settings: crate::core::domain::SystemSettings::default(),
        }
    }

    #[test]
    fn mentioned_agent_becomes_subtask_target() {
        let subtasks = build_suggested_subtasks(&context("@Reviewer please check this draft"));
        assert_eq!(subtasks.len(), 1);
        assert!(subtasks[0].contains("@Reviewer"));
    }

    #[test]
    fn parallel_request_creates_fallback_subtask() {
        let subtasks = build_suggested_subtasks(&context("Please handle this in parallel"));
        assert_eq!(subtasks.len(), 1);
        assert!(subtasks[0].contains("@Reviewer"));
    }
}
