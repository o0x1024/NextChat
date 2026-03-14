mod legacy_inputs;
mod policy;
#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Mutex as AsyncMutex,
    time::timeout,
};

use crate::core::buildin_tools::catalog::{BUILTIN_SKILLS, BUILTIN_TOOLS};
use crate::core::domain::{
    AgentProfile, SkillPack, ToolExecutionRequest, ToolExecutionResult, ToolHandler, ToolManifest,
    ToolStreamChunk,
};
use crate::core::permissions::{APPROVAL_REQUIRED_PREFIX, PERMISSION_DENIED_PREFIX};
use crate::core::skill_policy::{effective_tools_for_agent, selected_skills_for_agent};
use crate::core::tool_worker::{resolve_worker_binary, ShellWorkerRequest, ShellWorkerResponse};

#[derive(Debug, Clone)]
pub struct ToolRuntime {
    pub(crate) workspace_root: PathBuf,
    pub(crate) app_data_dir: PathBuf,
    pub(crate) http_client: Client,
    pub(crate) bash_runs: Arc<AsyncMutex<HashMap<String, BackgroundBashRun>>>,
    pub(crate) bash_tasks: Arc<AsyncMutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    pub(crate) task_state: Arc<AsyncMutex<HashMap<String, TaskItem>>>,
    pub(crate) task_counter: Arc<AsyncMutex<u64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskItem {
    pub(crate) id: String,
    pub(crate) subject: String,
    pub(crate) description: String,
    pub(crate) active_form: Option<String>,
    pub(crate) status: String,
    pub(crate) owner: Option<String>,
    pub(crate) blocks: Vec<String>,
    pub(crate) blocked_by: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct BackgroundBashRun {
    pub(crate) status: String,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) read_offset_stdout: usize,
    pub(crate) read_offset_stderr: usize,
}

impl BackgroundBashRun {
    pub(crate) fn running() -> Self {
        Self {
            status: "running".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            read_offset_stdout: 0,
            read_offset_stderr: 0,
        }
    }
}

impl ToolRuntime {
    pub fn new(workspace_root: PathBuf, app_data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&app_data_dir)?;
        let workspace_root = workspace_root.canonicalize().unwrap_or(workspace_root);
        let app_data_dir = app_data_dir.canonicalize().unwrap_or(app_data_dir);
        let http_client = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            workspace_root,
            app_data_dir,
            http_client,
            bash_runs: Arc::new(AsyncMutex::new(HashMap::new())),
            bash_tasks: Arc::new(AsyncMutex::new(HashMap::new())),
            task_state: Arc::new(AsyncMutex::new(HashMap::new())),
            task_counter: Arc::new(AsyncMutex::new(0)),
        })
    }

    pub fn builtin_tools(&self) -> Vec<ToolManifest> {
        BUILTIN_TOOLS.clone()
    }

    pub fn builtin_skills(&self) -> Vec<SkillPack> {
        BUILTIN_SKILLS.clone()
    }

    pub fn all_skills(&self) -> Vec<SkillPack> {
        let mut skills = self.builtin_skills();
        skills.extend(self.installed_skills());
        skills
    }

    pub fn tool_by_id(&self, tool_id: &str) -> Option<ToolManifest> {
        BUILTIN_TOOLS
            .iter()
            .find(|tool| tool.id == tool_id)
            .cloned()
    }

    pub fn available_tools_for_agent(&self, agent: &AgentProfile) -> Vec<ToolManifest> {
        let selected_skills = selected_skills_for_agent(agent, &self.all_skills());
        effective_tools_for_agent(agent, &self.builtin_tools(), &selected_skills)
    }

    pub fn select_tool_for_text(
        &self,
        text: &str,
        allowed_tool_ids: &[String],
    ) -> Option<ToolManifest> {
        let lowered = text.to_lowercase();
        let keywords: HashMap<&str, &[&str]> = HashMap::from([
            (
                "Task",
                &["agent", "delegate", "subtask", "子任务", "委派"][..],
            ),
            ("Bash", &["bash", "shell", "command", "终端", "命令"][..]),
            ("Glob", &["glob", "pattern", "文件名", "匹配"][..]),
            ("Grep", &["grep", "regex", "search", "查找", "搜索"][..]),
            (
                "Skill",
                &["skill", "skills", "skill.md", "技能", "提示词包"][..],
            ),
            ("LS", &["ls", "list", "directory", "目录"][..]),
            ("Read", &["read", "查看", "读取", "file"][..]),
            ("Edit", &["edit", "replace", "修改", "替换"][..]),
            ("Write", &["write", "save", "写入", "保存"][..]),
            ("WebFetch", &["fetch", "url", "网页抓取"][..]),
            ("TaskCreate", &["task create", "new task", "创建任务"][..]),
            ("TaskList", &["task list", "todo", "待办", "list tasks"][..]),
            ("TaskGet", &["task get", "get task"][..]),
            (
                "TaskUpdate",
                &["task update", "update task", "mark task"][..],
            ),
            ("WebSearch", &["web search", "search web", "联网搜索"][..]),
            (
                "TaskOutput",
                &["task output", "bash output", "shell output", "后台输出"][..],
            ),
            (
                "TaskStop",
                &["task stop", "kill bash", "terminate shell", "停止命令"][..],
            ),
        ]);

        let mut selected = None;
        for tool in BUILTIN_TOOLS.iter() {
            let allowed = allowed_tool_ids.is_empty() || allowed_tool_ids.contains(&tool.id);
            if !allowed {
                continue;
            }
            if let Some(words) = keywords.get(tool.id.as_str()) {
                if words.iter().any(|word| lowered.contains(word)) {
                    selected = Some(tool.clone());
                    break;
                }
            }
        }

        selected.or_else(|| {
            BUILTIN_TOOLS
                .iter()
                .find(|tool| {
                    tool.id == "TaskCreate"
                        && (allowed_tool_ids.is_empty() || allowed_tool_ids.contains(&tool.id))
                })
                .cloned()
        })
    }

    fn is_path_within_execution_scope(&self, path: &Path, execution_root: &Path) -> bool {
        path.starts_with(execution_root) || path.starts_with(&self.app_data_dir)
    }

    pub fn normalize_working_directory(&self, raw: &str) -> Result<String> {
        let execution_root = self.resolve_execution_root(raw)?;
        Ok(execution_root.display().to_string())
    }

    pub(crate) fn resolve_execution_root(&self, raw: &str) -> Result<PathBuf> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "." {
            return Ok(self.workspace_root.clone());
        }

        let candidate = PathBuf::from(trimmed);
        let absolute = if candidate.is_absolute() {
            candidate
        } else {
            self.workspace_root.join(candidate)
        };
        let normalized = absolute
            .canonicalize()
            .with_context(|| format!("working directory not found: {}", absolute.display()))?;
        if !normalized.is_dir() {
            bail!(
                "working directory is not a directory: {}",
                normalized.display()
            );
        }
        Ok(normalized)
    }

    pub(crate) fn resolve_permission_root(&self, raw: &str, execution_root: &Path) -> PathBuf {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("app_data") || trimmed.eq_ignore_ascii_case("$APP_DATA") {
            return self.app_data_dir.clone();
        }
        if trimmed.is_empty() || trimmed == "." {
            return execution_root.to_path_buf();
        }

        let candidate = PathBuf::from(trimmed);
        let absolute = if candidate.is_absolute() {
            candidate
        } else {
            execution_root.join(candidate)
        };
        absolute.canonicalize().unwrap_or(absolute)
    }

    pub(crate) fn resolve_path(
        &self,
        raw: &str,
        create_parent: bool,
        execution_root: &Path,
    ) -> Result<PathBuf> {
        let candidate = PathBuf::from(raw.trim());
        let absolute = if candidate.is_absolute() {
            candidate
        } else {
            execution_root.join(candidate)
        };

        let normalized = if create_parent {
            let parent = absolute
                .parent()
                .ok_or_else(|| anyhow!("path has no parent: {}", absolute.display()))?;
            fs::create_dir_all(parent)?;
            let parent_canonical = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            parent_canonical.join(
                absolute
                    .file_name()
                    .ok_or_else(|| anyhow!("invalid file name for path {}", absolute.display()))?,
            )
        } else {
            absolute
                .canonicalize()
                .with_context(|| format!("file not found: {}", absolute.display()))?
        };

        if !self.is_path_within_execution_scope(&normalized, execution_root) {
            bail!(
                "path '{}' is outside working directory '{}' and app data '{}'",
                normalized.display(),
                execution_root.display(),
                self.app_data_dir.display()
            );
        }
        Ok(normalized)
    }

    pub(crate) fn parse_json_input<T>(&self, input: &str, tool_name: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_str::<T>(input)
            .with_context(|| format!("{tool_name} expects JSON object input"))
    }

    pub(crate) async fn run_shell_command_with_worker(
        &self,
        command_text: String,
        timeout_ms: u64,
        execution_root: &Path,
    ) -> Result<(i32, String, String)> {
        let worker_binary = resolve_worker_binary()?;
        let payload = serde_json::to_vec(&ShellWorkerRequest {
            workspace_root: execution_root.display().to_string(),
            command: command_text,
        })?;

        let mut command = Command::new(worker_binary);
        command.arg("--tool-worker").arg("shell-exec");
        command
            .current_dir(execution_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().context("failed to spawn shell worker")?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("shell worker stdin unavailable"))?;
        stdin
            .write_all(&payload)
            .await
            .context("failed to send shell worker request")?;
        drop(stdin);

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("shell worker stdout unavailable"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("shell worker stderr unavailable"))?;

        let stdout_task = tokio::spawn(async move {
            let mut buffer = Vec::new();
            stdout.read_to_end(&mut buffer).await?;
            Result::<Vec<u8>>::Ok(buffer)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buffer = Vec::new();
            stderr.read_to_end(&mut buffer).await?;
            Result::<Vec<u8>>::Ok(buffer)
        });

        let status = match timeout(Duration::from_millis(timeout_ms), child.wait()).await {
            Ok(result) => result.context("shell worker wait failed")?,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                bail!("shell worker timed out");
            }
        };

        let stdout = stdout_task
            .await
            .context("shell worker stdout join failed")??;
        let stderr = stderr_task
            .await
            .context("shell worker stderr join failed")??;

        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            bail!(
                "shell worker failed{}",
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            );
        }

        let response: ShellWorkerResponse =
            serde_json::from_slice(&stdout).context("shell worker returned invalid JSON")?;
        Ok((response.status, response.stdout, response.stderr))
    }

    pub(crate) async fn run_shell_command_streaming(
        &self,
        command_text: String,
        timeout_ms: u64,
        execution_root: &Path,
        tool_id: &str,
        tool_stream: Option<tokio::sync::mpsc::UnboundedSender<ToolStreamChunk>>,
    ) -> Result<(i32, String, String)> {
        let mut command = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(&command_text);
            cmd
        } else {
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-lc").arg(&command_text);
            cmd
        };
        command
            .current_dir(execution_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().context("failed to spawn shell command")?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("shell command stdout unavailable"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("shell command stderr unavailable"))?;

        let stdout_stream = tool_stream.clone();
        let stderr_stream = tool_stream;
        let stdout_tool_id = tool_id.to_string();
        let stderr_tool_id = tool_id.to_string();

        let stdout_task = tokio::spawn(async move {
            let mut output = String::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = stdout.read(&mut buffer).await?;
                if read == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
                output.push_str(&chunk);
                if let Some(stream) = stdout_stream.as_ref() {
                    let _ = stream.send(ToolStreamChunk {
                        tool_id: stdout_tool_id.clone(),
                        channel: "stdout".to_string(),
                        delta: chunk,
                    });
                }
            }
            Result::<String>::Ok(output)
        });

        let stderr_task = tokio::spawn(async move {
            let mut output = String::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = stderr.read(&mut buffer).await?;
                if read == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
                output.push_str(&chunk);
                if let Some(stream) = stderr_stream.as_ref() {
                    let _ = stream.send(ToolStreamChunk {
                        tool_id: stderr_tool_id.clone(),
                        channel: "stderr".to_string(),
                        delta: chunk,
                    });
                }
            }
            Result::<String>::Ok(output)
        });

        let status = match timeout(Duration::from_millis(timeout_ms), child.wait()).await {
            Ok(result) => result.context("shell command wait failed")?,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                bail!("shell command timed out");
            }
        };

        let stdout = stdout_task
            .await
            .context("shell command stdout join failed")??;
        let stderr = stderr_task
            .await
            .context("shell command stderr join failed")??;
        Ok((status.code().unwrap_or_default(), stdout, stderr))
    }
}

#[async_trait]
impl ToolHandler for ToolRuntime {
    async fn execute(&self, request: ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let normalized_input = self.normalize_compat_input(&request.tool, &request.input);
        let normalized_request = ToolExecutionRequest {
            tool: request.tool.clone(),
            input: normalized_input,
            task_card_id: request.task_card_id.clone(),
            agent_id: request.agent_id.clone(),
            agent: request.agent.clone(),
            approval_granted: request.approval_granted,
            working_directory: request.working_directory.clone(),
            tool_stream: request.tool_stream.clone(),
        };
        let decision = self.authorize_tool_call(
            &normalized_request.agent,
            &normalized_request.tool,
            &normalized_request.input,
            &normalized_request.working_directory,
        )?;
        if !decision.allowed {
            bail!(
                "{PERMISSION_DENIED_PREFIX} {}",
                decision
                    .reason
                    .unwrap_or_else(|| "tool access rejected".to_string())
            );
        }
        if decision.approval_required && !request.approval_granted {
            bail!(
                "{APPROVAL_REQUIRED_PREFIX} tool '{}' needs explicit approval for agent '{}'",
                request.tool.name,
                request.agent.name
            );
        }

        match normalized_request.tool.id.as_str() {
            "Task" => self.run_task_compat_tool(&normalized_request).await,
            "AskUserQuestion" => self.run_ask_user_question_tool(&normalized_request).await,
            "RequestPeerInput" => self.run_request_peer_input_tool(&normalized_request).await,
            "Bash" => self.run_bash_compat_tool(&normalized_request).await,
            "Glob" => self.run_glob_compat_tool(&normalized_request).await,
            "Grep" => self.run_grep_compat_tool(&normalized_request).await,
            "Skill" => self.run_skill_tool(&normalized_request).await,
            "LS" => self.run_ls_compat_tool(&normalized_request).await,
            "Read" => self.run_read_compat_tool(&normalized_request).await,
            "Edit" => self.run_edit_compat_tool(&normalized_request).await,
            "Write" => self.run_write_compat_tool(&normalized_request).await,
            "WebFetch" => self.run_web_fetch_compat_tool(&normalized_request).await,
            "TaskCreate" => self.run_task_create_tool(&normalized_request).await,
            "TaskGet" => self.run_task_get_tool(&normalized_request).await,
            "TaskUpdate" => self.run_task_update_tool(&normalized_request).await,
            "TaskList" => self.run_task_list_tool(&normalized_request).await,
            "WebSearch" => self.run_web_search_compat_tool(&normalized_request).await,
            "TaskOutput" => self.run_task_output_compat_tool(&normalized_request).await,
            "TaskStop" => self.run_task_stop_compat_tool(&normalized_request).await,
            _ => Err(anyhow!("unsupported tool '{}'", request.tool.id)),
        }
    }
}

pub(crate) fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("node_modules" | ".git" | "target" | "dist" | ".next")
    )
}

pub(crate) fn should_skip_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "sqlite3" | "db" | "woff" | "woff2")
    )
}

pub(crate) fn truncate(input: &str, max_chars: usize) -> String {
    let mut value = input.trim().to_string();
    if value.chars().count() <= max_chars {
        return value;
    }
    value = value.chars().take(max_chars).collect::<String>();
    format!("{value}\n...[truncated]")
}

pub(crate) fn glob_match(pattern: &str, candidate: &str) -> bool {
    let regex = glob_to_regex(pattern);
    Regex::new(&regex)
        .map(|compiled| compiled.is_match(candidate))
        .unwrap_or(false)
}

fn glob_to_regex(pattern: &str) -> String {
    let mut regex = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' => {
                if matches!(chars.peek(), Some('*')) {
                    chars.next();
                    regex.push_str(".*");
                } else {
                    regex.push_str("[^/]*");
                }
            }
            '?' => regex.push('.'),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }
    regex.push('$');
    regex
}

pub(crate) fn domain_matches(host: &str, rule: &str) -> bool {
    let host = host.to_lowercase();
    let normalized = rule
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches('*')
        .trim_start_matches('.')
        .trim_end_matches('/')
        .to_lowercase();
    !normalized.is_empty() && (host == normalized || host.ends_with(&format!(".{normalized}")))
}
