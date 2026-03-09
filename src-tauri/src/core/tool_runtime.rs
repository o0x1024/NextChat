use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::{
    fs as tokio_fs,
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};
use walkdir::WalkDir;

use crate::core::domain::{
    SkillPack, ToolExecutionRequest, ToolExecutionResult, ToolHandler, ToolManifest, ToolRiskLevel,
};
use crate::core::tool_worker::{resolve_worker_binary, ShellWorkerRequest, ShellWorkerResponse};

pub static BUILTIN_TOOLS: Lazy<Vec<ToolManifest>> = Lazy::new(|| {
    vec![
        ToolManifest {
            id: "file.readwrite".into(),
            name: "File Read/Write".into(),
            category: "filesystem".into(),
            risk_level: ToolRiskLevel::High,
            input_schema: r#"{"path":"string","mode":"read|write","content":"string?"}"#.into(),
            output_schema: r#"{"content":"string","saved":"boolean","path":"string"}"#.into(),
            timeout_ms: 30_000,
            concurrency_limit: 2,
            permissions: vec!["fs:read".into(), "fs:write".into()],
            description: "Inspect or modify local workspace files.".into(),
        },
        ToolManifest {
            id: "project.search".into(),
            name: "Project Search".into(),
            category: "workspace".into(),
            risk_level: ToolRiskLevel::Low,
            input_schema: r#"{"query":"string"}"#.into(),
            output_schema: r#"{"matches":["string"]}"#.into(),
            timeout_ms: 15_000,
            concurrency_limit: 4,
            permissions: vec!["workspace:index".into()],
            description: "Search code and notes inside the active workspace.".into(),
        },
        ToolManifest {
            id: "shell.exec".into(),
            name: "Shell Command".into(),
            category: "system".into(),
            risk_level: ToolRiskLevel::High,
            input_schema: r#"{"command":"string"}"#.into(),
            output_schema: r#"{"stdout":"string","stderr":"string","status":"number"}"#.into(),
            timeout_ms: 45_000,
            concurrency_limit: 1,
            permissions: vec!["system:shell".into()],
            description: "Run shell commands in an isolated worker.".into(),
        },
        ToolManifest {
            id: "http.request".into(),
            name: "HTTP Request".into(),
            category: "network".into(),
            risk_level: ToolRiskLevel::Medium,
            input_schema: r#"{"url":"string","method":"string","body":"string?"}"#.into(),
            output_schema: r#"{"status":"number","body":"string"}"#.into(),
            timeout_ms: 20_000,
            concurrency_limit: 3,
            permissions: vec!["network:http".into()],
            description: "Fetch remote APIs and websites with audit logging.".into(),
        },
        ToolManifest {
            id: "browser.automation".into(),
            name: "Browser Automation".into(),
            category: "browser".into(),
            risk_level: ToolRiskLevel::High,
            input_schema: r#"{"url":"string","actions":["string"]}"#.into(),
            output_schema: r#"{"snapshot":"string","result":"string"}"#.into(),
            timeout_ms: 60_000,
            concurrency_limit: 1,
            permissions: vec!["browser:automation".into()],
            description: "Drive a browser session for UI workflows.".into(),
        },
        ToolManifest {
            id: "markdown.compose".into(),
            name: "Markdown Compose".into(),
            category: "content".into(),
            risk_level: ToolRiskLevel::Low,
            input_schema: r#"{"topic":"string","format":"string"}"#.into(),
            output_schema: r#"{"markdown":"string"}"#.into(),
            timeout_ms: 8_000,
            concurrency_limit: 4,
            permissions: vec!["content:markdown".into()],
            description: "Draft markdown reports, specs, and release notes.".into(),
        },
        ToolManifest {
            id: "plan.summarize".into(),
            name: "Plan & Summarize".into(),
            category: "coordination".into(),
            risk_level: ToolRiskLevel::Low,
            input_schema: r#"{"input":"string"}"#.into(),
            output_schema: r#"{"summary":"string"}"#.into(),
            timeout_ms: 5_000,
            concurrency_limit: 8,
            permissions: vec!["coordination:summary".into()],
            description: "Generate execution summaries and concise plans.".into(),
        },
    ]
});

pub static BUILTIN_SKILLS: Lazy<Vec<SkillPack>> = Lazy::new(|| {
    vec![
        SkillPack {
            id: "skill.research".into(),
            name: "Research Sweep".into(),
            prompt_template: "Break the task into evidence collection, synthesis, and gaps.".into(),
            planning_rules: vec![
                "Collect facts before proposing action.".into(),
                "Surface uncertainty explicitly.".into(),
            ],
            allowed_tool_tags: vec!["workspace".into(), "network".into(), "coordination".into()],
            done_criteria: vec!["Summarize findings".into(), "Call out blockers".into()],
        },
        SkillPack {
            id: "skill.builder".into(),
            name: "Builder Loop".into(),
            prompt_template: "Prefer implementation-ready artifacts and concrete next steps."
                .into(),
            planning_rules: vec![
                "Decompose by dependency order.".into(),
                "Prefer actionable outputs.".into(),
            ],
            allowed_tool_tags: vec![
                "filesystem".into(),
                "workspace".into(),
                "coordination".into(),
            ],
            done_criteria: vec!["Produce a change set".into(), "Note verification".into()],
        },
        SkillPack {
            id: "skill.reviewer".into(),
            name: "Review Lens".into(),
            prompt_template: "Prioritize regressions, security, and missing tests.".into(),
            planning_rules: vec![
                "List findings by severity.".into(),
                "Separate findings from summary.".into(),
            ],
            allowed_tool_tags: vec!["workspace".into(), "coordination".into()],
            done_criteria: vec!["Identify risks".into(), "Recommend fixes".into()],
        },
    ]
});

#[derive(Debug, Clone)]
pub struct ToolRuntime {
    workspace_root: PathBuf,
    app_data_dir: PathBuf,
    http_client: Client,
}

#[derive(Debug, Deserialize)]
struct FileToolInput {
    path: String,
    mode: Option<String>,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ShellToolInput {
    command: String,
}

#[derive(Debug, Deserialize)]
struct HttpToolInput {
    url: String,
    method: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchToolInput {
    query: String,
}

#[derive(Debug, Deserialize)]
struct BrowserToolInput {
    url: String,
    actions: Option<Vec<String>>,
    headed: Option<bool>,
    screenshot_name: Option<String>,
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
        })
    }

    pub fn builtin_tools(&self) -> Vec<ToolManifest> {
        BUILTIN_TOOLS.clone()
    }

    pub fn builtin_skills(&self) -> Vec<SkillPack> {
        BUILTIN_SKILLS.clone()
    }

    pub fn tool_by_id(&self, tool_id: &str) -> Option<ToolManifest> {
        BUILTIN_TOOLS
            .iter()
            .find(|tool| tool.id == tool_id)
            .cloned()
    }

    pub fn select_tool_for_text(
        &self,
        text: &str,
        allowed_tool_ids: &[String],
    ) -> Option<ToolManifest> {
        let lowered = text.to_lowercase();
        let keywords: HashMap<&str, &[&str]> = HashMap::from([
            (
                "shell.exec",
                &[
                    "shell",
                    "bash",
                    "terminal",
                    "command",
                    "rm ",
                    "delete",
                    "执行命令",
                ][..],
            ),
            (
                "browser.automation",
                &["browser", "website", "web page", "网页", "site"][..],
            ),
            (
                "http.request",
                &[
                    "http", "api", "request", "fetch", "接口", "https://", "http://",
                ][..],
            ),
            (
                "file.readwrite",
                &[
                    "file", "write", "document", "保存", "文件", ".md", ".rs", ".ts",
                ][..],
            ),
            (
                "project.search",
                &["search", "find", "grep", "scan", "搜索", "查找"][..],
            ),
            (
                "markdown.compose",
                &["markdown", "report", "doc", "文档", "总结"][..],
            ),
            (
                "plan.summarize",
                &["summary", "summarize", "plan", "拆解", "计划"][..],
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
                .find(|tool| tool.id == "plan.summarize")
                .cloned()
        })
    }

    fn is_allowed_path(&self, path: &Path) -> bool {
        path.starts_with(&self.workspace_root) || path.starts_with(&self.app_data_dir)
    }

    fn resolve_path(&self, raw: &str, create_parent: bool) -> Result<PathBuf> {
        let candidate = PathBuf::from(raw.trim());
        let absolute = if candidate.is_absolute() {
            candidate
        } else {
            self.workspace_root.join(candidate)
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

        if !self.is_allowed_path(&normalized) {
            bail!(
                "path '{}' is outside allowed roots '{}', '{}'",
                normalized.display(),
                self.workspace_root.display(),
                self.app_data_dir.display()
            );
        }
        Ok(normalized)
    }

    fn parse_file_input(&self, input: &str) -> Result<FileToolInput> {
        if let Ok(parsed) = serde_json::from_str::<FileToolInput>(input) {
            return Ok(parsed);
        }

        let path_regex = Regex::new(
            r#"([./~A-Za-z0-9_\-]+(?:/[A-Za-z0-9._\-]+)+|[A-Za-z0-9._\-]+\.[A-Za-z0-9]+)"#,
        )
        .expect("file regex");
        let path = path_regex
            .captures(input)
            .and_then(|capture| capture.get(1).map(|m| m.as_str().to_string()))
            .ok_or_else(|| anyhow!("could not infer a file path from input"))?;
        let lowered = input.to_lowercase();
        let mode = if lowered.contains("write")
            || lowered.contains("save")
            || lowered.contains("保存")
            || lowered.contains("写入")
        {
            "write".to_string()
        } else {
            "read".to_string()
        };
        let content = if mode == "write" {
            input
                .split_once("content:")
                .map(|(_, value)| value.trim().to_string())
        } else {
            None
        };
        Ok(FileToolInput {
            path,
            mode: Some(mode),
            content,
        })
    }

    fn parse_shell_input(&self, input: &str) -> Result<ShellToolInput> {
        if let Ok(parsed) = serde_json::from_str::<ShellToolInput>(input) {
            return Ok(parsed);
        }
        let command = if let Some((_, body)) = input.split_once("command:") {
            body.trim().to_string()
        } else if let Some(capture) = Regex::new(r"`([^`]+)`")
            .expect("command regex")
            .captures(input)
            .and_then(|capture| capture.get(1).map(|m| m.as_str().to_string()))
        {
            capture
        } else {
            input.trim().to_string()
        };
        if command.is_empty() {
            bail!("shell command is empty");
        }
        Ok(ShellToolInput { command })
    }

    fn parse_http_input(&self, input: &str) -> Result<HttpToolInput> {
        if let Ok(parsed) = serde_json::from_str::<HttpToolInput>(input) {
            return Ok(parsed);
        }
        let url = Regex::new(r#"https?://[^\s"']+"#)
            .expect("url regex")
            .find(input)
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow!("could not infer a URL from input"))?;
        let method = if input.to_lowercase().contains("post") {
            Some("POST".to_string())
        } else {
            Some("GET".to_string())
        };
        Ok(HttpToolInput {
            url,
            method,
            body: None,
        })
    }

    fn parse_search_input(&self, input: &str) -> SearchToolInput {
        if let Ok(parsed) = serde_json::from_str::<SearchToolInput>(input) {
            return parsed;
        }
        let query = input
            .replace("search", "")
            .replace("find", "")
            .replace("grep", "")
            .replace("搜索", "")
            .trim()
            .to_string();
        SearchToolInput {
            query: if query.is_empty() {
                input.trim().to_string()
            } else {
                query
            },
        }
    }

    fn parse_browser_input(&self, input: &str) -> Result<BrowserToolInput> {
        if let Ok(parsed) = serde_json::from_str::<BrowserToolInput>(input) {
            return Ok(parsed);
        }
        let url = Regex::new(r#"https?://[^\s"']+"#)
            .expect("browser url regex")
            .find(input)
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow!("could not infer a browser URL from input"))?;
        Ok(BrowserToolInput {
            url,
            actions: None,
            headed: Some(false),
            screenshot_name: None,
        })
    }

    async fn run_file_tool(&self, request: &ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let input = self.parse_file_input(&request.input)?;
        let mode = input
            .mode
            .unwrap_or_else(|| "read".to_string())
            .to_lowercase();
        match mode.as_str() {
            "read" => {
                let path = self.resolve_path(&input.path, false)?;
                let mut file = tokio_fs::File::open(&path).await?;
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).await?;
                let content = String::from_utf8_lossy(&buffer).to_string();
                let output = json!({
                    "path": path.display().to_string(),
                    "saved": false,
                    "content": truncate(&content, 16_000),
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(path.display().to_string()),
                })
            }
            "write" => {
                let path = self.resolve_path(&input.path, true)?;
                let content = input
                    .content
                    .ok_or_else(|| anyhow!("file write requires content"))?;
                tokio_fs::write(&path, content.as_bytes()).await?;
                let output = json!({
                    "path": path.display().to_string(),
                    "saved": true,
                    "content": truncate(&content, 8_000),
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(path.display().to_string()),
                })
            }
            other => bail!("unsupported file mode '{other}'"),
        }
    }

    async fn run_search_tool(&self, request: &ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let input = self.parse_search_input(&request.input);
        if input.query.trim().is_empty() {
            bail!("search query is empty");
        }
        let query_lower = input.query.to_lowercase();
        let matches = tokio::task::spawn_blocking({
            let root = self.workspace_root.clone();
            move || -> Result<Vec<String>> {
                let mut results = Vec::new();
                let iter = WalkDir::new(&root)
                    .into_iter()
                    .filter_entry(|entry| !should_skip_dir(entry.path()));
                for entry in iter.filter_map(Result::ok) {
                    if results.len() >= 40 {
                        break;
                    }
                    let path = entry.path();
                    if !entry.file_type().is_file() || should_skip_file(path) {
                        continue;
                    }
                    let bytes = fs::read(path).unwrap_or_default();
                    if bytes.is_empty() {
                        continue;
                    }
                    let haystack = String::from_utf8_lossy(&bytes);
                    for (index, line) in haystack.lines().enumerate() {
                        if line.to_lowercase().contains(&query_lower) {
                            let relative = path
                                .strip_prefix(&root)
                                .unwrap_or(path)
                                .display()
                                .to_string();
                            results.push(format!(
                                "{}:{}:{}",
                                relative,
                                index + 1,
                                truncate(line.trim(), 180)
                            ));
                            if results.len() >= 40 {
                                break;
                            }
                        }
                    }
                }
                Ok(results)
            }
        })
        .await??;

        let output = json!({
            "query": input.query,
            "matchCount": matches.len(),
            "matches": matches,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_shell_tool(&self, request: &ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let input = self.parse_shell_input(&request.input)?;
        let worker_binary = resolve_worker_binary()?;
        let payload = serde_json::to_vec(&ShellWorkerRequest {
            workspace_root: self.workspace_root.display().to_string(),
            command: input.command.clone(),
        })?;

        let mut command = Command::new(worker_binary);
        command.arg("--tool-worker").arg("shell-exec");
        command
            .current_dir(&self.workspace_root)
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

        let status = match timeout(Duration::from_secs(45), child.wait()).await {
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
        let stdout = truncate(&response.stdout, 16_000);
        let stderr = truncate(&response.stderr, 8_000);
        let result = json!({
            "command": response.command,
            "status": response.status,
            "stdout": stdout,
            "stderr": stderr,
            "worker": "external_process",
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: result.clone(),
            result_ref: Some(result),
        })
    }

    async fn run_http_tool(&self, request: &ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let input = self.parse_http_input(&request.input)?;
        let method = input
            .method
            .unwrap_or_else(|| "GET".to_string())
            .to_uppercase();
        let response = match method.as_str() {
            "POST" => {
                let body = input.body.unwrap_or_default();
                self.http_client.post(&input.url).body(body).send().await?
            }
            "PUT" => {
                let body = input.body.unwrap_or_default();
                self.http_client.put(&input.url).body(body).send().await?
            }
            "DELETE" => self.http_client.delete(&input.url).send().await?,
            _ => self.http_client.get(&input.url).send().await?,
        };
        let status = response.status().as_u16();
        let body = truncate(&response.text().await.unwrap_or_default(), 16_000);
        let output = json!({
            "url": input.url,
            "method": method,
            "status": status,
            "body": body,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_browser_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_browser_input(&request.input)?;
        let wrapper =
            PathBuf::from("/Users/a1024/.codex/skills/playwright/scripts/playwright_cli.sh");
        if !wrapper.exists() {
            bail!("playwright wrapper not found at {}", wrapper.display());
        }

        let session =
            sanitize_session_name(&format!("{}-{}", request.task_card_id, request.agent_id));
        let output_dir = self.workspace_root.join("output/playwright").join(&session);
        fs::create_dir_all(&output_dir)?;
        let mut logs = Vec::new();

        let mut open_args = vec!["open".to_string(), input.url.clone()];
        if input.headed.unwrap_or(false) {
            open_args.push("--headed".to_string());
        }
        logs.push(
            self.run_playwright_command(&wrapper, &output_dir, &session, &open_args, 60)
                .await
                .context("failed to open browser page")?,
        );
        logs.push(
            self.run_playwright_command(
                &wrapper,
                &output_dir,
                &session,
                &[String::from("snapshot")],
                60,
            )
            .await
            .context("failed to snapshot browser page")?,
        );

        if let Some(actions) = input.actions.clone() {
            for action in actions {
                let args = shell_words(&action);
                if args.is_empty() {
                    continue;
                }
                let is_navigation_like = matches!(
                    args.first().map(String::as_str),
                    Some("click" | "fill" | "type" | "press" | "tab-new" | "tab-select" | "open")
                );
                logs.push(
                    self.run_playwright_command(&wrapper, &output_dir, &session, &args, 90)
                        .await
                        .with_context(|| format!("playwright action failed: {action}"))?,
                );
                if is_navigation_like {
                    logs.push(
                        self.run_playwright_command(
                            &wrapper,
                            &output_dir,
                            &session,
                            &[String::from("snapshot")],
                            60,
                        )
                        .await
                        .context("failed to refresh snapshot after browser action")?,
                    );
                }
            }
        }

        let screenshot_name = input
            .screenshot_name
            .unwrap_or_else(|| "screenshot.png".to_string());
        let screenshot_path = output_dir.join(&screenshot_name);
        logs.push(
            self.run_playwright_command(
                &wrapper,
                &output_dir,
                &session,
                &[String::from("screenshot")],
                90,
            )
            .await
            .context("failed to capture Playwright screenshot")?,
        );

        let output = json!({
            "session": session,
            "url": input.url,
            "screenshot": screenshot_path.display().to_string(),
            "logs": logs,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(screenshot_path.display().to_string()),
        })
    }

    async fn run_playwright_command(
        &self,
        wrapper: &Path,
        command_dir: &Path,
        session: &str,
        args: &[String],
        timeout_secs: u64,
    ) -> Result<String> {
        let mut command = Command::new(wrapper);
        command
            .env("PLAYWRIGHT_CLI_SESSION", session)
            .current_dir(command_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for arg in args {
            command.arg(arg);
        }

        let output = timeout(Duration::from_secs(timeout_secs), command.output())
            .await
            .map_err(|_| anyhow!("playwright command timed out"))??;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Executable doesn't exist") || stderr.contains("install") {
                bail!(
                    "Playwright browser is not installed. Run `PLAYWRIGHT_BROWSERS_PATH=0 npx playwright install chromium` first."
                );
            }
            bail!("playwright command failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stdout.is_empty() && stderr.is_empty() {
            Ok(format!("{} ok", args.join(" ")))
        } else if stderr.is_empty() {
            Ok(truncate(&stdout, 12_000))
        } else {
            Ok(truncate(&format!("{stdout}\n{stderr}"), 12_000))
        }
    }

    fn run_markdown_tool(&self, request: &ToolExecutionRequest) -> ToolExecutionResult {
        let output = format!("# Draft\n\n{}", request.input.trim());
        ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        }
    }

    fn run_plan_tool(&self, request: &ToolExecutionRequest) -> ToolExecutionResult {
        let cleaned = request.input.trim();
        let output = json!({
            "summary": format!("Execution plan ready: {}", truncate(cleaned, 600)),
            "taskCardId": request.task_card_id,
            "agentId": request.agent_id,
        })
        .to_string();
        ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        }
    }
}

#[async_trait]
impl ToolHandler for ToolRuntime {
    async fn execute(&self, request: ToolExecutionRequest) -> Result<ToolExecutionResult> {
        match request.tool.id.as_str() {
            "file.readwrite" => self.run_file_tool(&request).await,
            "project.search" => self.run_search_tool(&request).await,
            "shell.exec" => self.run_shell_tool(&request).await,
            "http.request" => self.run_http_tool(&request).await,
            "browser.automation" => self.run_browser_tool(&request).await,
            "markdown.compose" => Ok(self.run_markdown_tool(&request)),
            "plan.summarize" => Ok(self.run_plan_tool(&request)),
            _ => Err(anyhow!("unsupported tool '{}'", request.tool.id)),
        }
    }
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("node_modules" | ".git" | "target" | "dist" | ".next")
    )
}

fn should_skip_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "sqlite3" | "db" | "woff" | "woff2")
    )
}

fn truncate(input: &str, max_chars: usize) -> String {
    let mut value = input.trim().to_string();
    if value.chars().count() <= max_chars {
        return value;
    }
    value = value.chars().take(max_chars).collect::<String>();
    format!("{value}\n...[truncated]")
}

fn shell_words(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(|part| part.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn sanitize_session_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ToolRuntime;
    use crate::core::domain::{ToolExecutionRequest, ToolHandler};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_root(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("nextchat-{prefix}-{nanos}"))
    }

    #[tokio::test]
    async fn file_tool_reads_and_writes() {
        let workspace_root = unique_root("workspace");
        let data_root = unique_root("data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root.clone(), data_root).expect("runtime");
        let tool = runtime.tool_by_id("file.readwrite").expect("tool");

        let write_result = runtime
            .execute(ToolExecutionRequest {
                tool: tool.clone(),
                input: r#"{"path":"notes/spec.md","mode":"write","content":"hello"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
            })
            .await
            .expect("write");
        assert!(write_result.output.contains("\"saved\":true"));

        let read_result = runtime
            .execute(ToolExecutionRequest {
                tool,
                input: r#"{"path":"notes/spec.md","mode":"read"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
            })
            .await
            .expect("read");
        assert!(read_result.output.contains("hello"));
    }

    #[tokio::test]
    async fn project_search_returns_matches() {
        let workspace_root = unique_root("search-workspace");
        let data_root = unique_root("search-data");
        fs::create_dir_all(workspace_root.join("src")).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        fs::write(
            workspace_root.join("src/main.ts"),
            "const agentName = 'Scout';\nconsole.log(agentName);\n",
        )
        .expect("seed file");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
        let result = runtime
            .execute(ToolExecutionRequest {
                tool: runtime.tool_by_id("project.search").expect("tool"),
                input: r#"{"query":"Scout"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
            })
            .await
            .expect("search");
        assert!(result.output.contains("src/main.ts"));
        assert!(result.output.contains("Scout"));
    }
}
