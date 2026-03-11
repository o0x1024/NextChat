use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::json;

use super::ask_user_question::{AskUserQuestionSignal, AskUserQuestionToolInput};
use crate::core::domain::{ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::{truncate, TodoItem, ToolRuntime};

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
struct ExitPlanModeToolInput {
    plan: String,
}

#[derive(Debug, Deserialize)]
struct TodoWriteToolInput {
    todos: Vec<TodoItem>,
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

    pub(crate) async fn run_exit_plan_mode_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input =
            self.parse_json_input::<ExitPlanModeToolInput>(&request.input, "ExitPlanMode")?;
        let output = json!({
            "status": "ready_to_code",
            "plan": truncate(&input.plan, 6_000),
            "taskCardId": request.task_card_id,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_todo_write_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<TodoWriteToolInput>(&request.input, "TodoWrite")?;
        for item in &input.todos {
            if !matches!(
                item.status.as_str(),
                "pending" | "in_progress" | "completed"
            ) {
                bail!("invalid todo status '{}'", item.status);
            }
            if item.content.trim().is_empty() || item.id.trim().is_empty() {
                bail!("todo id/content cannot be empty");
            }
        }
        {
            let mut state = self.todo_state.lock().await;
            *state = input.todos.clone();
        }
        let output = json!({
            "status": "ok",
            "count": input.todos.len(),
            "todos": input.todos,
            "taskCardId": request.task_card_id,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }
}
