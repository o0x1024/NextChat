use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use crate::core::domain::{
    AgentExecution, AgentExecutor, ExecutionMode, ModelProviderAdapter, TaskExecutionContext,
    ToolExecutionRequest, ToolHandler,
};
use crate::core::llm_rig::complete_task_with_tools;
use crate::core::runtime_environment::runtime_environment_block;

#[derive(Clone)]
pub struct AgentRuntime<TModel, TTool> {
    model_adapter: Arc<TModel>,
    tool_handler: Arc<TTool>,
}

const MAX_SUGGESTED_SUBTASKS: usize = 3;

impl<TModel, TTool> AgentRuntime<TModel, TTool> {
    pub fn new(model_adapter: Arc<TModel>, tool_handler: Arc<TTool>) -> Self {
        Self {
            model_adapter,
            tool_handler,
        }
    }

    async fn execute_approved_tool(
        &self,
        context: &TaskExecutionContext,
        skill_summary: &str,
    ) -> Result<AgentExecution>
    where
        TTool: ToolHandler + 'static,
    {
        let tool = context
            .approved_tool
            .clone()
            .expect("approved tool should exist");
        let input = context
            .approved_tool_input
            .clone()
            .unwrap_or_else(|| context.task_card.input_payload.clone());
        let result = self
            .tool_handler
            .execute(ToolExecutionRequest {
                tool: tool.clone(),
                input,
                task_card_id: context.task_card.id.clone(),
                agent_id: context.agent.id.clone(),
                agent: context.agent.clone(),
                approval_granted: true,
                working_directory: context.work_group.working_directory.clone(),
                tool_stream: context.tool_stream.clone(),
            })
            .await?;
        let suggested_subtasks = build_suggested_subtasks(context);
        let memory_summary = if context.memory_context.is_empty() {
            "None".to_string()
        } else {
            context
                .memory_context
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let collaborator_summary = if suggested_subtasks.is_empty() {
            "None".to_string()
        } else {
            suggested_subtasks.join(" | ")
        };

        Ok(AgentExecution {
            summary: format!(
                "{} used {} to work on '{}'.",
                context.agent.name, tool.name, context.task_card.title
            ),
            backstage_notes: format!(
                "Skills exposed: {}. Tools exposed: {}. Memory injected: {}. Tools called: {}. Suggested collaborators: {}.",
                skill_summary,
                context
                    .available_tools
                    .iter()
                    .map(|available_tool| available_tool.id.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                memory_summary,
                tool.id,
                collaborator_summary
            ),
            suggested_subtasks,
            tool_output: Some(result.output),
            execution_mode: ExecutionMode::Fallback,
        })
    }
}

fn build_execution_preamble(context: &TaskExecutionContext) -> String {
    let skill_summary = if context.available_skills.is_empty() {
        "No enabled skills exposed.".to_string()
    } else {
        context
            .available_skills
            .iter()
            .map(|skill| format!("{} ({})", skill.name, skill.id))
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

    format!(
        "You are {}. Role: {}. Objective: {}. Enabled skills: {}. Available tools: {}.\nRuntime environment:\n{}\nUse tools when they materially improve accuracy. If the Skill tool is available and a skill matches the request, invoking Skill is a BLOCKING REQUIREMENT — do it before generating any other response about the task.",
        context.agent.name,
        context.agent.role,
        context.agent.objective,
        skill_summary,
        available_tool_summary,
        runtime_environment_block(&context.work_group)
    )
}

fn build_execution_prompt(context: &TaskExecutionContext) -> String {
    let skill_catalog = context
        .available_skills
        .iter()
        .map(|skill| format!("- {} [{}]", skill.name, skill.id))
        .collect::<Vec<_>>();
    let memory_summary = if context.memory_context.is_empty() {
        "- None".to_string()
    } else {
        context
            .memory_context
            .iter()
            .map(|item| {
                format!(
                    "- [{}] {}",
                    item.scope_id,
                    item.content.lines().next().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let conversation_items = context
        .conversation_window
        .iter()
        .rev()
        .take(6)
        .map(|message| format!("{}: {}", message.sender_name, message.content))
        .collect::<Vec<_>>()
        .join("\n");

    let upstream_section = if let Some(ref upstream) = context.upstream_context {
        format!("\nUpstream stage deliverables (use these as context for your work):\n{}\n", upstream)
    } else {
        String::new()
    };

    format!(
        "Work group goal: {}\nWorking directory: {}\nTask: {}\n{}\nConversation items:\n{}\nMemory context:\n{}\nEnabled skills catalog:\n{}\nReturn a concise progress summary. If a skill from the catalog matches this task, invoke the Skill tool BEFORE doing anything else — this is a BLOCKING REQUIREMENT. If you need to delegate follow-up work, append one line per child task in the exact format `Delegate @AgentName: task details`. If you used tools, cite the concrete outcome instead of generic statements.",
        context.work_group.goal,
        context.work_group.working_directory,
        context.task_card.normalized_goal,
        upstream_section,
        conversation_items,
        memory_summary,
        if skill_catalog.is_empty() {
            "- None".to_string()
        } else {
            skill_catalog.join("\n")
        }
    )
}

#[async_trait]
impl<TModel, TTool> AgentExecutor for AgentRuntime<TModel, TTool>
where
    TModel: ModelProviderAdapter + 'static,
    TTool: ToolHandler + 'static,
{
    async fn execute_task(&self, context: TaskExecutionContext) -> Result<AgentExecution> {
        let skill_summary = if context.available_skills.is_empty() {
            "No enabled skills exposed.".to_string()
        } else {
            context
                .available_skills
                .iter()
                .map(|skill| format!("{} ({})", skill.name, skill.id))
                .collect::<Vec<_>>()
                .join(", ")
        };

        if context.approved_tool.is_some() {
            return self.execute_approved_tool(&context, &skill_summary).await;
        }

        let preamble = build_execution_preamble(&context);
        let prompt = build_execution_prompt(&context);

        let rig_execution = complete_task_with_tools(
            &context,
            self.tool_handler.clone(),
            &preamble,
            &prompt,
            context.summary_stream.clone(),
        )
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
                            agent: context.agent.clone(),
                            approval_granted: true,
                            working_directory: context.work_group.working_directory.clone(),
                            tool_stream: context.tool_stream.clone(),
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
                .complete(
                    &context.agent.model_policy,
                    &context.settings,
                    &preamble,
                    &prompt,
                )
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
        let draft_summary = model_summary.or(fallback_summary).unwrap_or_default();
        let explicit_subtasks = extract_delegation_directives(&context, &draft_summary);
        let summary = cleaned_summary(&context, &draft_summary, !explicit_subtasks.is_empty());
        let execution_mode = if used_real_model {
            ExecutionMode::RealModel
        } else {
            ExecutionMode::Fallback
        };

        let suggested_subtasks = merge_subtasks(
            explicit_subtasks,
            build_suggested_subtasks(&context),
            MAX_SUGGESTED_SUBTASKS,
        );

        let backstage_notes = format!(
            "Skills exposed: {}. Tools exposed: {}. Memory injected: {}. Tools called: {}. Suggested collaborators: {}.",
            skill_summary,
            context
                .available_tools
                .iter()
                .map(|tool| tool.id.clone())
                .collect::<Vec<_>>()
                .join(", "),
            if context.memory_context.is_empty() {
                "None".to_string()
            } else {
                context
                    .memory_context
                    .iter()
                    .map(|item| item.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
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
        subtasks.extend(
            context
                .work_group_members
                .iter()
                .filter(|candidate| candidate.id != context.agent.id)
                .take(2)
                .map(|agent| {
                    format!(
                        "@{} review '{}' in parallel and report only key findings.",
                        agent.name, context.task_card.title
                    )
                }),
        );
    }

    subtasks.truncate(MAX_SUGGESTED_SUBTASKS);
    subtasks
}

fn extract_delegation_directives(context: &TaskExecutionContext, summary: &str) -> Vec<String> {
    if !context.agent.can_spawn_subtasks || context.task_card.parent_id.is_some() {
        return Vec::new();
    }

    let directive_regex =
        Regex::new(r"(?im)^\s*delegate\s+@([A-Za-z0-9_\-\u4e00-\u9fa5]+)\s*:\s*(.+?)\s*$")
            .expect("delegation regex");

    directive_regex
        .captures_iter(summary)
        .filter_map(|capture| {
            let token = capture.get(1)?.as_str().to_lowercase();
            let task = capture.get(2)?.as_str().trim();
            let agent = context.work_group_members.iter().find(|candidate| {
                candidate.id != context.agent.id
                    && (candidate.name.to_lowercase() == token || candidate.id == token)
            })?;
            Some(format!("@{} {}", agent.name, task))
        })
        .collect()
}

fn cleaned_summary(
    context: &TaskExecutionContext,
    summary: &str,
    stripped_directives: bool,
) -> String {
    let directive_regex =
        Regex::new(r"(?im)^\s*delegate\s+@([A-Za-z0-9_\-\u4e00-\u9fa5]+)\s*:\s*(.+?)\s*$")
            .expect("delegation regex");
    let cleaned = directive_regex
        .replace_all(summary, "")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if !cleaned.is_empty() {
        cleaned
    } else if stripped_directives {
        format!(
            "{} delegated follow-up work for '{}'.",
            context.agent.name, context.task_card.title
        )
    } else {
        summary.trim().to_string()
    }
}

fn merge_subtasks(explicit: Vec<String>, heuristic: Vec<String>, limit: usize) -> Vec<String> {
    let mut merged = Vec::new();
    for item in explicit.into_iter().chain(heuristic.into_iter()) {
        if merged
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&item))
        {
            continue;
        }
        merged.push(item);
        if merged.len() >= limit {
            break;
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use anyhow::Result;
    use async_trait::async_trait;

    use super::{
        build_execution_preamble, build_execution_prompt, build_suggested_subtasks,
        cleaned_summary, extract_delegation_directives, AgentRuntime,
    };
    use crate::core::domain::{
        AgentExecution, AgentExecutor, AgentPermissionPolicy, AgentProfile, ExecutionMode,
        MemoryPolicy, ModelPolicy, ModelProviderAdapter, SystemSettings, TaskCard,
        TaskExecutionContext, TaskStatus, ToolExecutionRequest, ToolExecutionResult, ToolHandler,
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
            permission_policy: AgentPermissionPolicy::default(),
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
                working_directory: ".".into(),
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
                output_summary: None,
                created_at: "now".into(),
            },
            conversation_window: vec![],
            memory_context: vec![],
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
            approved_tool_input: None,
            upstream_context: None,
            settings: crate::core::domain::SystemSettings::default(),
            summary_stream: None,
            tool_stream: None,
            tool_call_stream: None,
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

    #[test]
    fn explicit_delegation_directives_are_extracted() {
        let ctx = context("Coordinate release");
        let directives = extract_delegation_directives(
            &ctx,
            "Status update\nDelegate @Reviewer: audit the release checklist\nDelegate @a2: verify user-facing copy",
        );
        assert_eq!(directives.len(), 2);
        assert!(directives[0].contains("@Reviewer"));
        assert!(directives[1].contains("@Reviewer"));
    }

    #[test]
    fn delegation_directives_are_removed_from_summary() {
        let ctx = context("Coordinate release");
        let summary = cleaned_summary(
            &ctx,
            "Shipped draft.\nDelegate @Reviewer: audit the release checklist",
            true,
        );
        assert_eq!(summary, "Shipped draft.");
    }

    #[test]
    fn execution_prompt_includes_working_directory() {
        let mut ctx = context("Inspect the repository structure");
        ctx.work_group.working_directory = "/Users/a1024/code/NextChat".into();

        let prompt = build_execution_prompt(&ctx);

        assert!(prompt.contains("Working directory: /Users/a1024/code/NextChat"));
    }

    #[test]
    fn execution_preamble_includes_environment_details() {
        let mut ctx = context("Inspect the repository structure");
        ctx.work_group.working_directory = "/Users/a1024/code/NextChat".into();

        let preamble = build_execution_preamble(&ctx);

        assert!(preamble.contains("Runtime environment:"));
        assert!(preamble.contains("Working directory: /Users/a1024/code/NextChat"));
        assert!(preamble.contains("Shell execution:"));
        assert!(preamble.contains("Platform:"));
    }

    struct NeverModel;

    #[async_trait]
    impl ModelProviderAdapter for NeverModel {
        async fn complete(
            &self,
            _policy: &ModelPolicy,
            _settings: &SystemSettings,
            _preamble: &str,
            _prompt: &str,
        ) -> Result<Option<String>> {
            panic!("model should not be used for approved tool execution")
        }
    }

    struct RecordingToolHandler {
        inputs: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ToolHandler for RecordingToolHandler {
        async fn execute(&self, request: ToolExecutionRequest) -> Result<ToolExecutionResult> {
            self.inputs
                .lock()
                .expect("inputs")
                .push(request.input.clone());
            Ok(ToolExecutionResult {
                output: format!("executed {}", request.tool.id),
                result_ref: Some(request.input),
            })
        }
    }

    #[tokio::test]
    async fn approved_tool_uses_saved_input() {
        let handler = std::sync::Arc::new(RecordingToolHandler {
            inputs: Mutex::new(Vec::new()),
        });
        let runtime = AgentRuntime::new(std::sync::Arc::new(NeverModel), handler.clone());
        let mut ctx = context("fallback task payload");
        ctx.approved_tool = Some(ToolManifest {
            id: "Bash".into(),
            name: "Bash".into(),
            category: "system".into(),
            risk_level: crate::core::domain::ToolRiskLevel::High,
            input_schema: "{}".into(),
            output_schema: "{}".into(),
            timeout_ms: 1_000,
            concurrency_limit: 1,
            permissions: vec![],
            description: "Run shell commands".into(),
        });
        ctx.approved_tool_input = Some(r#"{"command":"pwd"}"#.into());

        let execution: AgentExecution = runtime.execute_task(ctx).await.expect("execution");

        assert_eq!(execution.execution_mode, ExecutionMode::Fallback);
        assert_eq!(execution.tool_output.as_deref(), Some("executed Bash"));
        assert!(execution.summary.contains("Bash"));
        assert_eq!(
            handler.inputs.lock().expect("inputs").as_slice(),
            &[r#"{"command":"pwd"}"#.to_string()]
        );
    }
}
