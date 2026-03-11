use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;

use crate::core::domain::{ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::{truncate, BackgroundBashRun, ToolRuntime};

#[derive(Debug, Deserialize)]
struct BashCompatInput {
    command: String,
    timeout: Option<u64>,
    description: Option<String>,
    run_in_background: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BashOutputToolInput {
    bash_id: String,
    filter: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KillBashToolInput {
    shell_id: String,
}

impl ToolRuntime {
    pub(crate) async fn run_bash_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<BashCompatInput>(&request.input, "Bash")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.command.trim().is_empty() {
            bail!("Bash command is empty");
        }
        let timeout_ms = input.timeout.unwrap_or(120_000).clamp(1, 600_000);
        if input.run_in_background.unwrap_or(false) {
            let bash_id = uuid::Uuid::new_v4().to_string();
            {
                let mut runs = self.bash_runs.lock().await;
                runs.insert(bash_id.clone(), BackgroundBashRun::running());
            }

            let runtime = self.clone();
            let command = input.command.clone();
            let bash_id_for_task = bash_id.clone();
            let task = tokio::spawn(async move {
                let result = runtime
                    .run_shell_command_with_worker(command.clone(), timeout_ms, &execution_root)
                    .await;
                let mut runs = runtime.bash_runs.lock().await;
                if let Some(run) = runs.get_mut(&bash_id_for_task) {
                    match result {
                        Ok((_status, stdout, stderr)) => {
                            run.status = "completed".to_string();
                            run.stdout = stdout;
                            run.stderr = stderr;
                        }
                        Err(error) => {
                            run.status = "failed".to_string();
                            run.stderr = error.to_string();
                        }
                    }
                }
                let mut tasks = runtime.bash_tasks.lock().await;
                tasks.remove(&bash_id_for_task);
            });
            {
                let mut tasks = self.bash_tasks.lock().await;
                tasks.insert(bash_id.clone(), task);
            }

            let output = json!({
                "bash_id": bash_id,
                "status": "running",
                "command": input.command,
                "description": input.description,
            })
            .to_string();
            return Ok(ToolExecutionResult {
                output: output.clone(),
                result_ref: Some(output),
            });
        }

        let (status, stdout, stderr) = self
            .run_shell_command_streaming(
                input.command.clone(),
                timeout_ms,
                &execution_root,
                &request.tool.id,
                request.tool_stream.clone(),
            )
            .await?;
        let output = json!({
            "command": input.command,
            "description": input.description,
            "status": status,
            "stdout": truncate(&stdout, 24_000),
            "stderr": truncate(&stderr, 8_000),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_bash_output_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<BashOutputToolInput>(&request.input, "BashOutput")?;
        let filter_regex = if let Some(filter) = input.filter.as_ref() {
            Some(Regex::new(filter).with_context(|| "invalid BashOutput filter regex")?)
        } else {
            None
        };
        let mut runs = self.bash_runs.lock().await;
        let run = runs
            .get_mut(&input.bash_id)
            .ok_or_else(|| anyhow!("unknown bash_id '{}'", input.bash_id))?;
        let new_stdout = run
            .stdout
            .get(run.read_offset_stdout..)
            .unwrap_or("")
            .to_string();
        let new_stderr = run
            .stderr
            .get(run.read_offset_stderr..)
            .unwrap_or("")
            .to_string();
        run.read_offset_stdout = run.stdout.len();
        run.read_offset_stderr = run.stderr.len();

        let stdout = if let Some(regex) = filter_regex.as_ref() {
            new_stdout
                .lines()
                .filter(|line| regex.is_match(line))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            new_stdout
        };
        let stderr = if let Some(regex) = filter_regex.as_ref() {
            new_stderr
                .lines()
                .filter(|line| regex.is_match(line))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            new_stderr
        };

        let output = json!({
            "bash_id": input.bash_id,
            "status": run.status,
            "stdout": truncate(&stdout, 24_000),
            "stderr": truncate(&stderr, 8_000),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_kill_bash_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<KillBashToolInput>(&request.input, "KillBash")?;
        let mut killed = false;
        {
            let mut tasks = self.bash_tasks.lock().await;
            if let Some(handle) = tasks.remove(&input.shell_id) {
                handle.abort();
                killed = true;
            }
        }
        {
            let mut runs = self.bash_runs.lock().await;
            if let Some(run) = runs.get_mut(&input.shell_id) {
                run.status = if killed {
                    "killed".to_string()
                } else {
                    run.status.clone()
                };
                if killed {
                    run.stderr.push_str("\n[killed]");
                }
            }
        }
        let output = json!({
            "shell_id": input.shell_id,
            "killed": killed,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }
}
