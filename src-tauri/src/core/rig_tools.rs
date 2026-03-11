use std::sync::{Arc, Mutex};

use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::core::domain::{
    TaskExecutionContext, ToolExecutionRequest, ToolHandler, ToolManifest, ToolStreamChunk,
};
use tokio::sync::mpsc::UnboundedSender;

pub(crate) fn sanitize_rig_tool_name(tool_id: &str) -> String {
    let mut sanitized = String::with_capacity(tool_id.len());

    for ch in tool_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        "tool".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigToolEvent {
    pub tool_id: String,
    pub tool_name: String,
    pub call_id: String,
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct RigToolCallLog {
    events: Arc<Mutex<Vec<RigToolEvent>>>,
}

impl RigToolCallLog {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record_call(&self, tool_id: &str, tool_name: &str, call_id: &str, input: &str) {
        if let Ok(mut events) = self.events.lock() {
            if let Some(existing) = events.iter_mut().find(|event| event.call_id == call_id) {
                existing.tool_id = tool_id.to_string();
                existing.tool_name = tool_name.to_string();
                existing.input = input.to_string();
                return;
            }
            events.push(RigToolEvent {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                call_id: call_id.to_string(),
                input: input.to_string(),
                output: String::new(),
            });
        }
    }

    pub fn record_result(
        &self,
        tool_id: &str,
        tool_name: &str,
        call_id: &str,
        input: &str,
        output: &str,
    ) {
        if let Ok(mut events) = self.events.lock() {
            if let Some(existing) = events.iter_mut().find(|event| event.call_id == call_id) {
                existing.tool_id = tool_id.to_string();
                existing.tool_name = tool_name.to_string();
                existing.input = input.to_string();
                existing.output = output.to_string();
                return;
            }
            events.push(RigToolEvent {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                call_id: call_id.to_string(),
                input: input.to_string(),
                output: output.to_string(),
            });
        }
    }

    pub fn snapshot(&self) -> Vec<RigToolEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug)]
struct RigToolCallError(String);

impl std::fmt::Display for RigToolCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RigToolCallError {}

#[derive(Clone)]
pub struct NextChatRigTool<TTool> {
    manifest: ToolManifest,
    tool_handler: Arc<TTool>,
    task_card_id: String,
    agent_id: String,
    agent: crate::core::domain::AgentProfile,
    working_directory: String,
    approval_granted: bool,
    tool_stream: Option<UnboundedSender<ToolStreamChunk>>,
}

impl<TTool> NextChatRigTool<TTool> {
    pub fn new(
        manifest: ToolManifest,
        tool_handler: Arc<TTool>,
        task_card_id: String,
        agent_id: String,
        agent: crate::core::domain::AgentProfile,
        working_directory: String,
        approval_granted: bool,
        tool_stream: Option<UnboundedSender<ToolStreamChunk>>,
    ) -> Self {
        Self {
            manifest,
            tool_handler,
            task_card_id,
            agent_id,
            agent,
            working_directory,
            approval_granted,
            tool_stream,
        }
    }
}

impl<TTool> ToolDyn for NextChatRigTool<TTool>
where
    TTool: ToolHandler + 'static,
{
    fn name(&self) -> String {
        sanitize_rig_tool_name(&self.manifest.id)
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: sanitize_rig_tool_name(&self.manifest.id),
                description: format!(
                    "{}. {} Input must be a JSON object matching this schema: {}",
                    self.manifest.name, self.manifest.description, self.manifest.input_schema
                ),
                parameters: tool_parameters_schema(&self.manifest),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let normalized_input = normalize_tool_input(&args);
            let result = self
                .tool_handler
                .execute(ToolExecutionRequest {
                    tool: self.manifest.clone(),
                    input: normalized_input.clone(),
                    task_card_id: self.task_card_id.clone(),
                    agent_id: self.agent_id.clone(),
                    agent: self.agent.clone(),
                    approval_granted: self.approval_granted,
                    working_directory: self.working_directory.clone(),
                    tool_stream: self.tool_stream.clone(),
                })
                .await
                .map_err(|error| {
                    ToolError::ToolCallError(Box::new(RigToolCallError(error.to_string())))
                })?;

            Ok(result.output)
        })
    }
}

pub fn build_rig_tools<TTool>(
    context: &TaskExecutionContext,
    tool_handler: Arc<TTool>,
) -> Vec<Box<dyn ToolDyn>>
where
    TTool: ToolHandler + 'static,
{
    allowed_rig_tools(context)
        .into_iter()
        .map(|manifest| {
            let approval_granted = context
                .approved_tool
                .as_ref()
                .map(|tool| tool.id == manifest.id)
                .unwrap_or(false);
            Box::new(NextChatRigTool::new(
                manifest,
                tool_handler.clone(),
                context.task_card.id.clone(),
                context.agent.id.clone(),
                context.agent.clone(),
                context.work_group.working_directory.clone(),
                approval_granted,
                context.tool_stream.clone(),
            )) as Box<dyn ToolDyn>
        })
        .collect()
}

pub fn allowed_rig_tools(context: &TaskExecutionContext) -> Vec<ToolManifest> {
    let mut tools = context.available_tools.clone();

    if let Some(tool) = context.approved_tool.clone() {
        if !tools.iter().any(|candidate| candidate.id == tool.id) {
            tools.push(tool);
        }
    }

    tools.sort_by_key(|tool| {
        if context
            .approved_tool
            .as_ref()
            .map(|approved| approved.id == tool.id)
            .unwrap_or(false)
        {
            0
        } else {
            1
        }
    });
    tools
}

fn tool_parameters_schema(manifest: &ToolManifest) -> Value {
    if let Ok(parsed) = serde_json::from_str::<Value>(&manifest.input_schema) {
        if parsed.get("type").is_some() && parsed.get("properties").is_some() {
            return parsed;
        }
    }

    json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "string",
                "description": format!("Input for {}", manifest.name)
            }
        },
        "required": ["input"]
    })
}

fn normalize_tool_input(args: &str) -> String {
    match serde_json::from_str::<Value>(args) {
        Ok(Value::Object(map)) => map
            .get("input")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| Value::Object(map).to_string()),
        Ok(Value::String(value)) => value,
        Ok(value) => value.to_string(),
        Err(_) => args.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{allowed_rig_tools, normalize_tool_input, sanitize_rig_tool_name};
    use crate::core::domain::{
        AgentPermissionPolicy, AgentProfile, MemoryPolicy, ModelPolicy, SystemSettings, TaskCard,
        TaskExecutionContext, TaskStatus, ToolManifest, ToolRiskLevel, WorkGroup, WorkGroupKind,
    };

    fn manifest(id: &str, risk_level: ToolRiskLevel) -> ToolManifest {
        ToolManifest {
            id: id.into(),
            name: id.into(),
            category: "test".into(),
            risk_level,
            input_schema: "{}".into(),
            output_schema: "{}".into(),
            timeout_ms: 1000,
            concurrency_limit: 1,
            permissions: vec![],
            description: "test tool".into(),
        }
    }

    fn context(approved_tool: Option<ToolManifest>) -> TaskExecutionContext {
        TaskExecutionContext {
            agent: AgentProfile {
                id: "agent-1".into(),
                name: "Scout".into(),
                avatar: "SC".into(),
                role: "Engineer".into(),
                objective: "Ship".into(),
                model_policy: ModelPolicy::default(),
                skill_ids: vec![],
                tool_ids: vec![],
                max_parallel_runs: 1,
                can_spawn_subtasks: true,
                memory_policy: MemoryPolicy::default(),
                permission_policy: AgentPermissionPolicy::default(),
            },
            work_group: WorkGroup {
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
            },
            work_group_members: vec![],
            task_card: TaskCard {
                id: "task-1".into(),
                parent_id: None,
                source_message_id: "msg-1".into(),
                title: "Task".into(),
                normalized_goal: "Goal".into(),
                input_payload: "Goal".into(),
                priority: 1,
                status: TaskStatus::Pending,
                work_group_id: "wg-1".into(),
                created_by: "human".into(),
                assigned_agent_id: None,
                created_at: "now".into(),
            },
            conversation_window: vec![],
            memory_context: vec![],
            available_tools: vec![
                manifest("project.search", ToolRiskLevel::Low),
                manifest("file.readwrite", ToolRiskLevel::High),
            ],
            available_skills: vec![],
            approved_tool,
            approved_tool_input: None,
            settings: SystemSettings::default(),
            summary_stream: None,
            tool_stream: None,
            tool_call_stream: None,
        }
    }

    #[test]
    fn approved_tool_is_prioritized_when_exposed() {
        let tools = allowed_rig_tools(&context(None));
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].id, "project.search");
        assert_eq!(tools[1].id, "file.readwrite");

        let tools = allowed_rig_tools(&context(Some(manifest(
            "file.readwrite",
            ToolRiskLevel::High,
        ))));
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].id, "file.readwrite");
    }

    #[test]
    fn normalize_tool_input_prefers_input_field() {
        assert_eq!(
            normalize_tool_input(r#"{"input":"find auth code"}"#),
            "find auth code"
        );
        assert_eq!(
            normalize_tool_input(r#"{"query":"task"}"#),
            r#"{"query":"task"}"#
        );
    }

    #[test]
    fn rig_tool_name_is_sanitized_for_function_calling() {
        assert_eq!(sanitize_rig_tool_name("plan.summarize"), "plan_summarize");
        assert_eq!(
            sanitize_rig_tool_name("browser/automation"),
            "browser_automation"
        );
    }
}
