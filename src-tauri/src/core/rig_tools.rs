use std::sync::{Arc, Mutex};

use rig::{
    completion::ToolDefinition,
    tool::{ToolDyn, ToolError},
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::core::domain::{
    TaskExecutionContext, ToolExecutionRequest, ToolHandler, ToolManifest, ToolRiskLevel,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigToolEvent {
    pub tool_id: String,
    pub tool_name: String,
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

    pub fn push(&self, event: RigToolEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
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
    call_log: RigToolCallLog,
}

impl<TTool> NextChatRigTool<TTool> {
    pub fn new(
        manifest: ToolManifest,
        tool_handler: Arc<TTool>,
        task_card_id: String,
        agent_id: String,
        call_log: RigToolCallLog,
    ) -> Self {
        Self {
            manifest,
            tool_handler,
            task_card_id,
            agent_id,
            call_log,
        }
    }
}

impl<TTool> ToolDyn for NextChatRigTool<TTool>
where
    TTool: ToolHandler + 'static,
{
    fn name(&self) -> String {
        self.manifest.id.clone()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: self.manifest.id.clone(),
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
                })
                .await
                .map_err(|error| {
                    ToolError::ToolCallError(Box::new(RigToolCallError(error.to_string())))
                })?;

            self.call_log.push(RigToolEvent {
                tool_id: self.manifest.id.clone(),
                tool_name: self.manifest.name.clone(),
                input: normalized_input,
                output: result.output.clone(),
            });

            Ok(result.output)
        })
    }
}

pub fn build_rig_tools<TTool>(
    context: &TaskExecutionContext,
    tool_handler: Arc<TTool>,
    call_log: RigToolCallLog,
) -> Vec<Box<dyn ToolDyn>>
where
    TTool: ToolHandler + 'static,
{
    allowed_rig_tools(context)
        .into_iter()
        .map(|manifest| {
            Box::new(NextChatRigTool::new(
                manifest,
                tool_handler.clone(),
                context.task_card.id.clone(),
                context.agent.id.clone(),
                call_log.clone(),
            )) as Box<dyn ToolDyn>
        })
        .collect()
}

pub fn allowed_rig_tools(context: &TaskExecutionContext) -> Vec<ToolManifest> {
    let mut tools = context
        .available_tools
        .iter()
        .filter(|tool| tool.risk_level != ToolRiskLevel::High)
        .cloned()
        .collect::<Vec<_>>();

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
    match manifest.id.as_str() {
        "project.search" => json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search term or code fragment to find in the workspace."
                }
            },
            "required": ["query"]
        }),
        "file.readwrite" => json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or workspace-relative file path."
                },
                "mode": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "Use read to inspect files, write to save content."
                },
                "content": {
                    "type": "string",
                    "description": "Required when mode is write."
                }
            },
            "required": ["path", "mode"]
        }),
        "shell.exec" => json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to run in the workspace."
                }
            },
            "required": ["command"]
        }),
        "http.request" => json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Target URL."
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE"],
                    "description": "HTTP method."
                },
                "body": {
                    "type": "string",
                    "description": "Optional request body."
                }
            },
            "required": ["url"]
        }),
        "browser.automation" => json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Page URL to open."
                },
                "actions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional Playwright CLI actions."
                }
            },
            "required": ["url"]
        }),
        "markdown.compose" => json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Topic to draft."
                },
                "format": {
                    "type": "string",
                    "description": "Desired markdown format, for example report, notes, or checklist."
                }
            },
            "required": ["topic"]
        }),
        "plan.summarize" => json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Text to summarize into a concise plan or status update."
                }
            },
            "required": ["input"]
        }),
        _ => json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": format!("Input for {}", manifest.name)
                }
            },
            "required": ["input"]
        }),
    }
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
    use super::{allowed_rig_tools, normalize_tool_input};
    use crate::core::domain::{
        AgentProfile, MemoryPolicy, ModelPolicy, SystemSettings, TaskCard, TaskExecutionContext,
        TaskStatus, ToolManifest, ToolRiskLevel, WorkGroup, WorkGroupKind,
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
            },
            work_group: WorkGroup {
                id: "wg-1".into(),
                kind: WorkGroupKind::Persistent,
                name: "WG".into(),
                goal: "Goal".into(),
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
            available_tools: vec![
                manifest("project.search", ToolRiskLevel::Low),
                manifest("file.readwrite", ToolRiskLevel::High),
            ],
            available_skills: vec![],
            approved_tool,
            settings: SystemSettings::default(),
        }
    }

    #[test]
    fn high_risk_tool_requires_approval_to_be_exposed() {
        let tools = allowed_rig_tools(&context(None));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "project.search");

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
}
