use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::json;

use super::ask_user_question::{AskUserQuestionSignal, AskUserQuestionToolInput};
use super::request_peer_input::{RequestPeerInputSignal, RequestPeerInputToolInput};
use crate::core::domain::{ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::{truncate, TaskItem, ToolRuntime};

#[derive(Debug, Deserialize)]
struct TaskToolInput {
    description: String,
    prompt: String,
    subagent_type: String,
}

#[derive(Debug, Deserialize)]
struct AskUserQuestionCompatInput {
    question: String,
    options: Option<Vec<String>>,
    context: Option<String>,
    allow_free_form: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestPeerInputCompatInput {
    target_agent_id: String,
    question: String,
    context: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskCreateInput {
    subject: String,
    description: String,
    #[serde(rename = "activeForm")]
    active_form: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskGetInput {
    #[serde(rename = "taskId")]
    task_id: String,
}

#[derive(Debug, Deserialize)]
struct TaskUpdateInput {
    #[serde(rename = "taskId")]
    task_id: String,
    subject: Option<String>,
    description: Option<String>,
    #[serde(rename = "activeForm")]
    active_form: Option<String>,
    status: Option<String>,
    owner: Option<String>,
    #[serde(rename = "addBlocks")]
    add_blocks: Option<Vec<String>>,
    #[serde(rename = "addBlockedBy")]
    add_blocked_by: Option<Vec<String>>,
}

impl ToolRuntime {
    pub(crate) async fn run_task_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<TaskToolInput>(&request.input, "Task")?;
        let output = json!({
            "status": "accepted",
            "task": {
                "description": input.description,
                "prompt": truncate(&input.prompt, 4_000),
                "subagent_type": input.subagent_type,
                "requested_by": request.agent_id,
                "task_card_id": request.task_card_id,
            },
            "note": "Task tool is registered and input validated by runtime.",
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_ask_user_question_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input =
            self.parse_json_input::<AskUserQuestionCompatInput>(&request.input, "AskUserQuestion")?;
        let signal = AskUserQuestionSignal::from_input(AskUserQuestionToolInput {
            question: input.question,
            options: input.options.unwrap_or_default(),
            context: input.context,
            allow_free_form: input.allow_free_form,
        })?;
        bail!(signal.to_error_message()?);
    }

    pub(crate) async fn run_request_peer_input_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self
            .parse_json_input::<RequestPeerInputCompatInput>(&request.input, "RequestPeerInput")?;
        let signal = RequestPeerInputSignal::from_input(RequestPeerInputToolInput {
            target_agent_id: input.target_agent_id,
            question: input.question,
            context: input.context,
        })?;
        bail!(signal.to_error_message()?);
    }

    pub(crate) async fn run_task_create_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<TaskCreateInput>(&request.input, "TaskCreate")?;
        if input.subject.trim().is_empty() {
            bail!("TaskCreate: subject cannot be empty");
        }
        let id = {
            let mut counter = self.task_counter.lock().await;
            *counter += 1;
            format!("{:04}", *counter)
        };
        let task = TaskItem {
            id: id.clone(),
            subject: input.subject.clone(),
            description: input.description.clone(),
            active_form: input.active_form,
            status: "pending".to_string(),
            owner: None,
            blocks: vec![],
            blocked_by: vec![],
        };
        {
            let mut state = self.task_state.lock().await;
            state.insert(id.clone(), task.clone());
        }
        let output = json!({
            "id": task.id,
            "subject": task.subject,
            "description": task.description,
            "status": task.status,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_task_get_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<TaskGetInput>(&request.input, "TaskGet")?;
        let state = self.task_state.lock().await;
        let task = state
            .get(&input.task_id)
            .ok_or_else(|| anyhow!("TaskGet: task '{}' not found", input.task_id))?;
        let output = json!({
            "id": task.id,
            "subject": task.subject,
            "description": task.description,
            "activeForm": task.active_form,
            "status": task.status,
            "owner": task.owner,
            "blocks": task.blocks,
            "blockedBy": task.blocked_by,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_task_update_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<TaskUpdateInput>(&request.input, "TaskUpdate")?;
        if let Some(ref status) = input.status {
            if !matches!(
                status.as_str(),
                "pending" | "in_progress" | "completed" | "deleted"
            ) {
                bail!("TaskUpdate: invalid status '{}'", status);
            }
        }
        let output = {
            let mut state = self.task_state.lock().await;
            let task = state
                .get_mut(&input.task_id)
                .ok_or_else(|| anyhow!("TaskUpdate: task '{}' not found", input.task_id))?;
            if let Some(subject) = input.subject {
                task.subject = subject;
            }
            if let Some(description) = input.description {
                task.description = description;
            }
            if let Some(active_form) = input.active_form {
                task.active_form = Some(active_form);
            }
            if let Some(status) = input.status {
                task.status = status;
            }
            if let Some(owner) = input.owner {
                task.owner = Some(owner);
            }
            if let Some(add_blocks) = input.add_blocks {
                for b in add_blocks {
                    if !task.blocks.contains(&b) {
                        task.blocks.push(b);
                    }
                }
            }
            if let Some(add_blocked_by) = input.add_blocked_by {
                for b in add_blocked_by {
                    if !task.blocked_by.contains(&b) {
                        task.blocked_by.push(b);
                    }
                }
            }
            json!({
                "id": task.id,
                "subject": task.subject,
                "status": task.status,
                "owner": task.owner,
                "blocks": task.blocks,
                "blockedBy": task.blocked_by,
            })
            .to_string()
        };
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_task_list_tool(
        &self,
        _request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let state = self.task_state.lock().await;
        let mut tasks: Vec<_> = state
            .values()
            .filter(|t| t.status != "deleted")
            .map(|t| {
                json!({
                    "id": t.id,
                    "subject": t.subject,
                    "status": t.status,
                    "owner": t.owner,
                    "blockedBy": t.blocked_by,
                })
            })
            .collect();
        tasks.sort_by_key(|t| t["id"].as_str().unwrap_or("").to_string());
        let output = json!({ "tasks": tasks }).to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }
}
