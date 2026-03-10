mod policy;

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
use serde_json::{json, Value};
use tokio::{
    fs as tokio_fs,
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Mutex as AsyncMutex,
    time::timeout,
};
use walkdir::WalkDir;

use crate::core::domain::{
    AgentProfile, SkillPack, ToolExecutionRequest, ToolExecutionResult, ToolHandler, ToolManifest,
};
use crate::core::permissions::{APPROVAL_REQUIRED_PREFIX, PERMISSION_DENIED_PREFIX};
use crate::core::skill_policy::{effective_tools_for_agent, selected_skills_for_agent};
use crate::core::tool_catalog::{BUILTIN_SKILLS, BUILTIN_TOOLS};
use crate::core::tool_worker::{resolve_worker_binary, ShellWorkerRequest, ShellWorkerResponse};

#[derive(Debug, Clone)]
pub struct ToolRuntime {
    workspace_root: PathBuf,
    app_data_dir: PathBuf,
    http_client: Client,
    bash_runs: Arc<AsyncMutex<HashMap<String, BackgroundBashRun>>>,
    bash_tasks: Arc<AsyncMutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    todo_state: Arc<AsyncMutex<Vec<TodoItem>>>,
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

#[derive(Debug, Deserialize)]
struct SkillsToolInput {
    action: String,
    source: Option<String>,
    path: Option<String>,
    skill_id: Option<String>,
    name: Option<String>,
    prompt_template: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoItem {
    content: String,
    status: String,
    id: String,
}

#[derive(Debug, Clone)]
struct BackgroundBashRun {
    status: String,
    stdout: String,
    stderr: String,
    read_offset_stdout: usize,
    read_offset_stderr: usize,
}

impl BackgroundBashRun {
    fn running() -> Self {
        Self {
            status: "running".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            read_offset_stdout: 0,
            read_offset_stderr: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct BashCompatInput {
    command: String,
    timeout: Option<u64>,
    description: Option<String>,
    run_in_background: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GlobToolInput {
    pattern: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GrepToolInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    output_mode: Option<String>,
    #[serde(rename = "-B")]
    before: Option<usize>,
    #[serde(rename = "-A")]
    after: Option<usize>,
    #[serde(rename = "-C")]
    context: Option<usize>,
    #[serde(rename = "-n")]
    line_number: Option<bool>,
    #[serde(rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(rename = "type")]
    file_type: Option<String>,
    head_limit: Option<usize>,
    multiline: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LsToolInput {
    path: String,
    ignore: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ReadToolInput {
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EditToolInput {
    file_path: String,
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultiEditToolInput {
    file_path: String,
    edits: Vec<EditOperationInput>,
}

#[derive(Debug, Deserialize)]
struct EditOperationInput {
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WriteToolInput {
    file_path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct NotebookEditToolInput {
    notebook_path: String,
    cell_id: Option<String>,
    new_source: String,
    cell_type: Option<String>,
    edit_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebFetchToolInput {
    url: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct TodoWriteToolInput {
    todos: Vec<TodoItem>,
}

#[derive(Debug, Deserialize)]
struct WebSearchToolInput {
    query: String,
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
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

#[derive(Debug, Deserialize)]
struct TaskToolInput {
    description: String,
    prompt: String,
    subagent_type: String,
}

#[derive(Debug, Deserialize)]
struct ExitPlanModeToolInput {
    plan: String,
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
            todo_state: Arc::new(AsyncMutex::new(Vec::new())),
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
            (
                "skills.manage",
                &["skill", "skills", "安装", "github", "拖拽", "local"][..],
            ),
            (
                "Task",
                &["agent", "delegate", "subtask", "子任务", "委派"][..],
            ),
            ("Bash", &["bash", "shell", "command", "终端", "命令"][..]),
            ("Glob", &["glob", "pattern", "文件名", "匹配"][..]),
            ("Grep", &["grep", "regex", "search", "查找", "搜索"][..]),
            ("LS", &["ls", "list", "directory", "目录"][..]),
            ("ExitPlanMode", &["plan", "ready to code", "退出规划"][..]),
            ("Read", &["read", "查看", "读取", "file"][..]),
            ("Edit", &["edit", "replace", "修改", "替换"][..]),
            ("MultiEdit", &["multi edit", "batch edit", "批量修改"][..]),
            ("Write", &["write", "save", "写入", "保存"][..]),
            ("NotebookEdit", &["notebook", "ipynb", "jupyter"][..]),
            ("WebFetch", &["fetch", "url", "网页抓取"][..]),
            ("TodoWrite", &["todo", "task list", "待办"][..]),
            ("WebSearch", &["web search", "search web", "联网搜索"][..]),
            (
                "BashOutput",
                &["bash output", "shell output", "后台输出"][..],
            ),
            (
                "KillBash",
                &["kill bash", "terminate shell", "停止命令"][..],
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

    fn skills_root(&self) -> PathBuf {
        self.app_data_dir.join("skills")
    }

    fn installed_skill_meta_path(skill_dir: &Path) -> PathBuf {
        skill_dir.join(".nextchat-skill.json")
    }

    fn installed_skills(&self) -> Vec<SkillPack> {
        let root = self.skills_root();
        let entries = fs::read_dir(&root);
        let mut skills = Vec::new();
        let Ok(entries) = entries else {
            return skills;
        };

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(skill) = self.skill_pack_from_dir(&path) {
                skills.push(skill);
            }
        }
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    pub fn update_installed_skill(
        &self,
        skill_id: &str,
        name: Option<String>,
        prompt_template: Option<String>,
    ) -> Result<SkillPack> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let mut meta = self.load_skill_meta(&skill_dir)?;
        if let Some(next_name) = name {
            let trimmed = next_name.trim();
            if !trimmed.is_empty() {
                meta.name = Some(trimmed.to_string());
            }
        }
        if let Some(next_prompt) = prompt_template {
            let trimmed = next_prompt.trim();
            if !trimmed.is_empty() {
                meta.prompt_template = Some(trimmed.to_string());
            }
        }
        self.save_skill_meta(&skill_dir, &meta)?;
        self.skill_pack_from_dir(&skill_dir)
    }

    pub fn set_installed_skill_enabled(&self, skill_id: &str, enabled: bool) -> Result<SkillPack> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let mut meta = self.load_skill_meta(&skill_dir)?;
        meta.enabled = enabled;
        self.save_skill_meta(&skill_dir, &meta)?;
        self.skill_pack_from_dir(&skill_dir)
    }

    pub fn delete_installed_skill(&self, skill_id: &str) -> Result<()> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        fs::remove_dir_all(&skill_dir)
            .with_context(|| format!("failed to delete skill {}", skill_dir.display()))?;
        Ok(())
    }

    pub fn install_skill_from_local_path(&self, source_path: &str) -> Result<SkillPack> {
        let source = PathBuf::from(source_path.trim());
        if source.as_os_str().is_empty() {
            bail!("local skill path is empty");
        }
        let source = source
            .canonicalize()
            .with_context(|| format!("skill path not found: {}", source.display()))?;
        let source_dir = if source.is_file() {
            let file_name = source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !file_name.eq_ignore_ascii_case("SKILL.md") {
                bail!("local install expects a directory or SKILL.md file");
            }
            source
                .parent()
                .ok_or_else(|| anyhow!("invalid skill path"))?
                .to_path_buf()
        } else {
            source
        };
        self.install_skill_from_dir(&source_dir, InstalledSkillMeta::default())
    }

    pub async fn install_skill_from_github(
        &self,
        source: &str,
        skill_path: Option<&str>,
    ) -> Result<SkillPack> {
        let (repo_url, embedded_path) = parse_github_source(source)?;
        let relative_path = skill_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(embedded_path);
        let temp_root = self.app_data_dir.join("tmp");
        fs::create_dir_all(&temp_root)?;
        let clone_dir = temp_root.join(format!("skill-clone-{}", uuid::Uuid::new_v4()));
        let output = Command::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg(&repo_url)
            .arg(&clone_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to execute git clone")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let _ = fs::remove_dir_all(&clone_dir);
            bail!(
                "failed to clone github repository: {}",
                if stderr.is_empty() {
                    "git clone returned non-zero".to_string()
                } else {
                    stderr
                }
            );
        }

        let candidate = if let Some(path) = relative_path {
            clone_dir.join(path.trim_start_matches('/'))
        } else {
            clone_dir.clone()
        };
        let install_source = resolve_skill_source_dir(&candidate)?;
        let result = self.install_skill_from_dir(
            &install_source,
            InstalledSkillMeta {
                source: "github".into(),
                source_ref: Some(source.to_string()),
                enabled: true,
                name: None,
                prompt_template: None,
            },
        );
        let _ = fs::remove_dir_all(&clone_dir);
        result
    }

    fn install_skill_from_dir(
        &self,
        source_dir: &Path,
        meta: InstalledSkillMeta,
    ) -> Result<SkillPack> {
        let source_dir = source_dir
            .canonicalize()
            .with_context(|| format!("invalid source dir: {}", source_dir.display()))?;
        let skill_file = source_dir.join("SKILL.md");
        if !skill_file.exists() {
            bail!("SKILL.md not found in {}", source_dir.display());
        }

        let skills_root = self.skills_root();
        fs::create_dir_all(&skills_root)?;

        let slug = skill_slug_from_path(&source_dir)?;
        let destination = skills_root.join(slug);
        if destination.exists() {
            fs::remove_dir_all(&destination).with_context(|| {
                format!(
                    "failed to replace existing skill at {}",
                    destination.display()
                )
            })?;
        }
        copy_dir_recursively(&source_dir, &destination)?;
        self.save_skill_meta(&destination, &meta)?;
        self.skill_pack_from_dir(&destination)
    }

    fn skill_pack_from_dir(&self, skill_dir: &Path) -> Result<SkillPack> {
        let skill_file = skill_dir.join("SKILL.md");
        let raw = fs::read_to_string(&skill_file)
            .with_context(|| format!("failed reading {}", skill_file.display()))?;
        let metadata = parse_skill_markdown(&raw);
        let folder = skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid skill folder name"))?;
        let id = format!("skill.local.{}", sanitize_skill_id(folder));
        let name = metadata
            .name
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| folder.to_string());
        let description = metadata.description.unwrap_or_else(|| {
            "Use this installed skill as a reusable workflow and instruction set.".to_string()
        });
        let meta = self.load_skill_meta(skill_dir).unwrap_or_default();

        Ok(SkillPack {
            id,
            name: meta.name.unwrap_or(name),
            prompt_template: meta.prompt_template.unwrap_or(description),
            planning_rules: metadata
                .tags
                .iter()
                .map(|tag| format!("Tag: {tag}"))
                .collect(),
            allowed_tool_tags: vec![],
            done_criteria: vec![format!("Installed at {}", skill_dir.display())],
            enabled: meta.enabled,
            editable: true,
            source: meta.source,
            install_path: Some(skill_dir.display().to_string()),
        })
    }

    fn save_skill_meta(&self, skill_dir: &Path, meta: &InstalledSkillMeta) -> Result<()> {
        let path = Self::installed_skill_meta_path(skill_dir);
        let serialized = serde_json::to_string_pretty(meta)?;
        fs::write(&path, serialized)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    fn load_skill_meta(&self, skill_dir: &Path) -> Result<InstalledSkillMeta> {
        let path = Self::installed_skill_meta_path(skill_dir);
        if !path.exists() {
            return Ok(InstalledSkillMeta::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str::<InstalledSkillMeta>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    fn resolve_installed_skill_dir(&self, skill_id: &str) -> Result<PathBuf> {
        let prefix = "skill.local.";
        let slug = skill_id
            .strip_prefix(prefix)
            .ok_or_else(|| anyhow!("only local installed skills can be managed"))?;
        let dir = self.skills_root().join(sanitize_skill_id(slug));
        if !dir.exists() || !dir.is_dir() {
            bail!("skill not found: {skill_id}");
        }
        Ok(dir)
    }

    fn is_allowed_path(&self, path: &Path) -> bool {
        path.starts_with(&self.workspace_root) || path.starts_with(&self.app_data_dir)
    }

    fn resolve_permission_root(&self, raw: &str) -> PathBuf {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("app_data") || trimmed.eq_ignore_ascii_case("$APP_DATA") {
            return self.app_data_dir.clone();
        }
        if trimmed.is_empty() || trimmed == "." {
            return self.workspace_root.clone();
        }

        let candidate = PathBuf::from(trimmed);
        let absolute = if candidate.is_absolute() {
            candidate
        } else {
            self.workspace_root.join(candidate)
        };
        absolute.canonicalize().unwrap_or(absolute)
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

    fn parse_skills_input(&self, input: &str) -> Result<SkillsToolInput> {
        if let Ok(parsed) = serde_json::from_str::<SkillsToolInput>(input) {
            if parsed.action.trim().is_empty() {
                bail!("skills.manage action is required");
            }
            return Ok(parsed);
        }
        Ok(SkillsToolInput {
            action: input.trim().to_string(),
            source: None,
            path: None,
            skill_id: None,
            name: None,
            prompt_template: None,
            enabled: None,
        })
    }

    fn parse_json_input<T>(&self, input: &str, tool_name: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_str::<T>(input)
            .with_context(|| format!("{tool_name} expects JSON object input"))
    }

    async fn run_shell_command_with_worker(
        &self,
        command_text: String,
        timeout_ms: u64,
    ) -> Result<(i32, String, String)> {
        let worker_binary = resolve_worker_binary()?;
        let payload = serde_json::to_vec(&ShellWorkerRequest {
            workspace_root: self.workspace_root.display().to_string(),
            command: command_text,
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
        let (status, stdout_raw, stderr_raw) = self
            .run_shell_command_with_worker(input.command.clone(), 45_000)
            .await?;
        let stdout = truncate(&stdout_raw, 16_000);
        let stderr = truncate(&stderr_raw, 8_000);
        let result = json!({
            "command": input.command,
            "status": status,
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

    async fn run_skills_tool(&self, request: &ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let input = self.parse_skills_input(&request.input)?;
        match input.action.trim() {
            "list" => {
                let skills = self.all_skills();
                let output = json!({
                    "action": "list",
                    "status": "ok",
                    "count": skills.len(),
                    "skills": skills,
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(output),
                })
            }
            "install_local" => {
                let source = input
                    .source
                    .ok_or_else(|| anyhow!("install_local requires source path"))?;
                let skill = self.install_skill_from_local_path(&source)?;
                let output = json!({
                    "action": "install_local",
                    "status": "ok",
                    "skill": skill,
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(output),
                })
            }
            "install_github" => {
                let source = input
                    .source
                    .ok_or_else(|| anyhow!("install_github requires source URL"))?;
                let skill = self
                    .install_skill_from_github(&source, input.path.as_deref())
                    .await?;
                let output = json!({
                    "action": "install_github",
                    "status": "ok",
                    "skill": skill,
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(output),
                })
            }
            "update" => {
                let skill_id = input
                    .skill_id
                    .ok_or_else(|| anyhow!("update requires skill_id"))?;
                let skill = self.update_installed_skill(
                    &skill_id,
                    input.name.clone(),
                    input.prompt_template.clone(),
                )?;
                let output = json!({
                    "action": "update",
                    "status": "ok",
                    "skill": skill,
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(output),
                })
            }
            "toggle" => {
                let skill_id = input
                    .skill_id
                    .ok_or_else(|| anyhow!("toggle requires skill_id"))?;
                let enabled = input
                    .enabled
                    .ok_or_else(|| anyhow!("toggle requires enabled"))?;
                let skill = self.set_installed_skill_enabled(&skill_id, enabled)?;
                let output = json!({
                    "action": "toggle",
                    "status": "ok",
                    "skill": skill,
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(output),
                })
            }
            "delete" => {
                let skill_id = input
                    .skill_id
                    .ok_or_else(|| anyhow!("delete requires skill_id"))?;
                self.delete_installed_skill(&skill_id)?;
                let output = json!({
                    "action": "delete",
                    "status": "ok",
                    "skillId": skill_id,
                })
                .to_string();
                Ok(ToolExecutionResult {
                    output: output.clone(),
                    result_ref: Some(output),
                })
            }
            other => bail!("unsupported skills.manage action '{other}'"),
        }
    }

    async fn run_task_compat_tool(
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

    async fn run_bash_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<BashCompatInput>(&request.input, "Bash")?;
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
                    .run_shell_command_with_worker(command.clone(), timeout_ms)
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
            .run_shell_command_with_worker(input.command.clone(), timeout_ms)
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

    async fn run_glob_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<GlobToolInput>(&request.input, "Glob")?;
        if input.pattern.trim().is_empty() {
            bail!("glob pattern is required");
        }
        let root = if let Some(path) = input.path {
            self.resolve_path(path.trim(), false)?
        } else {
            self.workspace_root.clone()
        };
        if !root.exists() || !root.is_dir() {
            bail!("glob path does not exist or is not a directory");
        }

        let mut matches = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|entry| !should_skip_dir(entry.path()))
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = path.strip_prefix(&root).unwrap_or(path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if glob_match(&input.pattern, &rel_str) {
                matches.push(path.display().to_string());
            }
        }
        matches.sort();

        let output = json!({
            "pattern": input.pattern,
            "path": root.display().to_string(),
            "matches": matches,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_grep_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<GrepToolInput>(&request.input, "Grep")?;
        if input.pattern.trim().is_empty() {
            bail!("grep pattern is required");
        }
        let root = if let Some(path) = input.path.clone() {
            self.resolve_path(path.trim(), false)?
        } else {
            self.workspace_root.clone()
        };
        let mode = input
            .output_mode
            .clone()
            .unwrap_or_else(|| "files_with_matches".to_string());
        let regex = if input.case_insensitive.unwrap_or(false) {
            Regex::new(&format!("(?i){}", input.pattern))
                .with_context(|| format!("invalid regex pattern '{}'", input.pattern))?
        } else {
            Regex::new(&input.pattern)
                .with_context(|| format!("invalid regex pattern '{}'", input.pattern))?
        };
        let mut output_rows = Vec::new();
        let mut files_with_matches = Vec::new();

        let mut candidates = Vec::new();
        if root.is_file() {
            candidates.push(root);
        } else {
            for entry in WalkDir::new(&root)
                .into_iter()
                .filter_entry(|entry| !should_skip_dir(entry.path()))
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if entry.file_type().is_file() && !should_skip_file(path) {
                    candidates.push(path.to_path_buf());
                }
            }
        }

        for path in candidates {
            if let Some(file_type) = input.file_type.as_ref() {
                let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
                if ext != file_type {
                    continue;
                }
            }
            let rel = path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if let Some(glob) = input.glob.as_ref() {
                if !glob_match(glob, &rel) {
                    continue;
                }
            }

            let content = tokio_fs::read_to_string(&path).await.unwrap_or_default();
            if input.multiline.unwrap_or(false) {
                if let Some(found) = regex.find(&content) {
                    files_with_matches.push(rel.clone());
                    if mode == "content" {
                        output_rows.push(format!("{}:{}", rel, truncate(found.as_str(), 240)));
                    }
                }
                continue;
            }

            let mut matched_lines = Vec::new();
            for (index, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matched_lines.push((index + 1, line.to_string()));
                }
            }
            if matched_lines.is_empty() {
                continue;
            }
            files_with_matches.push(rel.clone());
            if mode == "files_with_matches" {
                continue;
            }
            if mode == "count" {
                output_rows.push(format!("{}:{}", rel, matched_lines.len()));
                continue;
            }
            for (line_no, line_text) in matched_lines {
                let rendered_line = if input.line_number.unwrap_or(false) {
                    format!("{}:{}:{}", rel, line_no, line_text)
                } else {
                    format!("{}:{}", rel, line_text)
                };
                output_rows.push(rendered_line);
                let before = input.before.or(input.context).unwrap_or(0);
                let after = input.after.or(input.context).unwrap_or(0);
                if before > 0 || after > 0 {
                    let lines = content.lines().collect::<Vec<_>>();
                    let start = line_no.saturating_sub(before + 1);
                    let end = (line_no + after).min(lines.len());
                    for (i, ctx_line) in lines[start..end].iter().enumerate() {
                        let ctx_no = start + i + 1;
                        if ctx_no != line_no {
                            output_rows.push(format!("{}:{}-{}", rel, ctx_no, ctx_line));
                        }
                    }
                }
            }
        }

        if let Some(limit) = input.head_limit {
            let limit = limit.max(1);
            if mode == "files_with_matches" {
                files_with_matches.truncate(limit);
            } else {
                output_rows.truncate(limit);
            }
        }

        let output = match mode.as_str() {
            "count" => json!({
                "pattern": input.pattern,
                "output_mode": mode,
                "matches": output_rows,
            }),
            "content" => json!({
                "pattern": input.pattern,
                "output_mode": mode,
                "matches": output_rows,
            }),
            _ => json!({
                "pattern": input.pattern,
                "output_mode": "files_with_matches",
                "matches": files_with_matches,
            }),
        }
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_ls_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<LsToolInput>(&request.input, "LS")?;
        let path = PathBuf::from(input.path.trim());
        if !path.is_absolute() {
            bail!("LS path must be absolute");
        }
        if !path.exists() || !path.is_dir() {
            bail!("LS path does not exist or is not a directory");
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let candidate = entry.path();
            let display = candidate.display().to_string();
            if input.ignore.as_ref().is_some_and(|ignores| {
                let rel = candidate
                    .strip_prefix(&path)
                    .unwrap_or(candidate.as_path())
                    .to_string_lossy()
                    .replace('\\', "/");
                ignores.iter().any(|pattern| glob_match(pattern, &rel))
            }) {
                continue;
            }
            entries.push(display);
        }
        entries.sort();
        let output = json!({
            "path": path.display().to_string(),
            "entries": entries,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_exit_plan_mode_tool(
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

    async fn run_read_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<ReadToolInput>(&request.input, "Read")?;
        let path = self.resolve_path(&input.file_path, false)?;
        let content = tokio_fs::read_to_string(&path).await?;
        let all_lines = content.lines().collect::<Vec<_>>();
        let start_line = input.offset.unwrap_or(1).max(1);
        let start_idx = start_line.saturating_sub(1);
        let limit = input.limit.unwrap_or(2000).max(1);
        let end_idx = (start_idx + limit).min(all_lines.len());
        let width = (end_idx.max(1)).to_string().len().max(1);
        let mut rendered = String::new();
        for (idx, line) in all_lines[start_idx..end_idx].iter().enumerate() {
            let number = start_idx + idx + 1;
            rendered.push_str(&format!("{:>width$}\t{}\n", number, line, width = width));
        }
        let output = json!({
            "file_path": path.display().to_string(),
            "offset": start_line,
            "limit": limit,
            "content": truncate(&rendered, 30_000),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    async fn run_edit_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<EditToolInput>(&request.input, "Edit")?;
        if input.old_string == input.new_string {
            bail!("old_string and new_string cannot be the same");
        }
        let path = self.resolve_path(&input.file_path, false)?;
        let content = tokio_fs::read_to_string(&path).await?;
        let occurrences = content.matches(&input.old_string).count();
        if occurrences == 0 {
            bail!("old_string not found in file");
        }
        let replace_all = input.replace_all.unwrap_or(false);
        if !replace_all && occurrences != 1 {
            bail!("old_string is not unique; use replace_all or provide more context");
        }
        let updated = if replace_all {
            content.replace(&input.old_string, &input.new_string)
        } else {
            content.replacen(&input.old_string, &input.new_string, 1)
        };
        tokio_fs::write(&path, updated.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "file_path": path.display().to_string(),
            "replacements": if replace_all { occurrences } else { 1 },
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    async fn run_multiedit_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<MultiEditToolInput>(&request.input, "MultiEdit")?;
        if input.edits.is_empty() {
            bail!("MultiEdit requires at least one edit");
        }
        let path = self.resolve_path(&input.file_path, false)?;
        let mut content = tokio_fs::read_to_string(&path).await?;
        let mut replacements = 0usize;
        for edit in &input.edits {
            if edit.old_string == edit.new_string {
                bail!("MultiEdit old_string and new_string cannot be identical");
            }
            let occurrences = content.matches(&edit.old_string).count();
            if occurrences == 0 {
                bail!("MultiEdit old_string not found");
            }
            let replace_all = edit.replace_all.unwrap_or(false);
            if !replace_all && occurrences != 1 {
                bail!("MultiEdit old_string is not unique");
            }
            content = if replace_all {
                replacements += occurrences;
                content.replace(&edit.old_string, &edit.new_string)
            } else {
                replacements += 1;
                content.replacen(&edit.old_string, &edit.new_string, 1)
            };
        }
        tokio_fs::write(&path, content.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "file_path": path.display().to_string(),
            "replacements": replacements,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    async fn run_write_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<WriteToolInput>(&request.input, "Write")?;
        let path = self.resolve_path(&input.file_path, true)?;
        tokio_fs::write(&path, input.content.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "file_path": path.display().to_string(),
            "bytes": input.content.len(),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    async fn run_notebook_edit_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input =
            self.parse_json_input::<NotebookEditToolInput>(&request.input, "NotebookEdit")?;
        let path = self.resolve_path(&input.notebook_path, false)?;
        let raw = tokio_fs::read_to_string(&path).await?;
        let mut notebook: Value =
            serde_json::from_str(&raw).context("invalid notebook JSON content")?;
        let cells = notebook
            .get_mut("cells")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("notebook does not contain a cells array"))?;

        let mode = input
            .edit_mode
            .clone()
            .unwrap_or_else(|| "replace".to_string());
        let target_idx = input
            .cell_id
            .as_ref()
            .and_then(|cell_id| {
                cells.iter().position(|cell| {
                    cell.get("id")
                        .and_then(Value::as_str)
                        .map(|id| id == cell_id)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(0);

        match mode.as_str() {
            "delete" => {
                if cells.is_empty() {
                    bail!("cannot delete from an empty notebook");
                }
                let idx = target_idx.min(cells.len() - 1);
                cells.remove(idx);
            }
            "insert" => {
                let cell_type = input
                    .cell_type
                    .clone()
                    .unwrap_or_else(|| "code".to_string());
                let new_cell = json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "cell_type": cell_type,
                    "metadata": {},
                    "source": input.new_source,
                    "outputs": [],
                    "execution_count": Value::Null,
                });
                if cells.is_empty() {
                    cells.push(new_cell);
                } else {
                    let idx = (target_idx + 1).min(cells.len());
                    cells.insert(idx, new_cell);
                }
            }
            _ => {
                if cells.is_empty() {
                    bail!("cannot replace cell in an empty notebook");
                }
                let idx = target_idx.min(cells.len() - 1);
                let existing_type = cells[idx]
                    .get("cell_type")
                    .and_then(Value::as_str)
                    .unwrap_or("code")
                    .to_string();
                let next_type = input.cell_type.clone().unwrap_or(existing_type);
                cells[idx]["cell_type"] = Value::String(next_type);
                cells[idx]["source"] = Value::String(input.new_source);
                if cells[idx].get("id").is_none() {
                    cells[idx]["id"] = Value::String(uuid::Uuid::new_v4().to_string());
                }
            }
        }

        let serialized = serde_json::to_string_pretty(&notebook)?;
        tokio_fs::write(&path, serialized.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "notebook_path": path.display().to_string(),
            "edit_mode": mode,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    async fn run_web_fetch_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<WebFetchToolInput>(&request.input, "WebFetch")?;
        let normalized_url = if input.url.starts_with("http://") {
            format!("https://{}", input.url.trim_start_matches("http://"))
        } else {
            input.url.clone()
        };
        let response = self.http_client.get(&normalized_url).send().await?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let output = json!({
            "url": normalized_url,
            "status": status,
            "prompt": input.prompt,
            "analysis": truncate(&body, 16_000),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_todo_write_compat_tool(
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

    async fn run_web_search_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<WebSearchToolInput>(&request.input, "WebSearch")?;
        if input.query.trim().len() < 2 {
            bail!("query must be at least 2 characters");
        }
        let url = reqwest::Url::parse_with_params(
            "https://duckduckgo.com/html/",
            &[("q", input.query.as_str())],
        )?;
        let response = self.http_client.get(url).send().await?;
        let html = response.text().await.unwrap_or_default();
        let result_re = Regex::new(r#"<a[^>]*class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
            .expect("valid websearch regex");
        let tag_re = Regex::new(r"<[^>]+>").expect("valid strip-tags regex");
        let mut results = Vec::new();
        for capture in result_re.captures_iter(&html) {
            if results.len() >= 8 {
                break;
            }
            let url = capture.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let title_html = capture.get(2).map(|m| m.as_str()).unwrap_or("");
            let title = tag_re.replace_all(title_html, "").trim().to_string();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let host = reqwest::Url::parse(&url)
                .ok()
                .and_then(|value| value.host_str().map(str::to_string))
                .unwrap_or_default();
            if input.blocked_domains.as_ref().is_some_and(|domains| {
                domains
                    .iter()
                    .any(|domain| domain_matches(&host, domain.as_str()))
            }) {
                continue;
            }
            if input.allowed_domains.as_ref().is_some_and(|domains| {
                !domains
                    .iter()
                    .any(|domain| domain_matches(&host, domain.as_str()))
            }) {
                continue;
            }
            results.push(json!({
                "title": title,
                "url": url,
                "host": host,
            }));
        }
        if results.is_empty() {
            results.push(json!({
                "title": "No parsed search results; returning snippet",
                "url": "https://duckduckgo.com/html/",
                "host": "duckduckgo.com",
                "snippet": truncate(&html, 500),
            }));
        }
        let output = json!({
            "query": input.query,
            "results": results,
            "taskCardId": request.task_card_id,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    async fn run_bash_output_compat_tool(
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

    async fn run_kill_bash_compat_tool(
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

#[async_trait]
impl ToolHandler for ToolRuntime {
    async fn execute(&self, request: ToolExecutionRequest) -> Result<ToolExecutionResult> {
        let decision = self.authorize_tool_call(&request.agent, &request.tool, &request.input)?;
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

        match request.tool.id.as_str() {
            "file.readwrite" => self.run_file_tool(&request).await,
            "project.search" => self.run_search_tool(&request).await,
            "shell.exec" => self.run_shell_tool(&request).await,
            "http.request" => self.run_http_tool(&request).await,
            "browser.automation" => self.run_browser_tool(&request).await,
            "markdown.compose" => Ok(self.run_markdown_tool(&request)),
            "plan.summarize" => Ok(self.run_plan_tool(&request)),
            "skills.manage" => self.run_skills_tool(&request).await,
            "Task" => self.run_task_compat_tool(&request).await,
            "Bash" => self.run_bash_compat_tool(&request).await,
            "Glob" => self.run_glob_compat_tool(&request).await,
            "Grep" => self.run_grep_compat_tool(&request).await,
            "LS" => self.run_ls_compat_tool(&request).await,
            "ExitPlanMode" => self.run_exit_plan_mode_tool(&request).await,
            "Read" => self.run_read_compat_tool(&request).await,
            "Edit" => self.run_edit_compat_tool(&request).await,
            "MultiEdit" => self.run_multiedit_compat_tool(&request).await,
            "Write" => self.run_write_compat_tool(&request).await,
            "NotebookEdit" => self.run_notebook_edit_compat_tool(&request).await,
            "WebFetch" => self.run_web_fetch_compat_tool(&request).await,
            "TodoWrite" => self.run_todo_write_compat_tool(&request).await,
            "WebSearch" => self.run_web_search_compat_tool(&request).await,
            "BashOutput" => self.run_bash_output_compat_tool(&request).await,
            "KillBash" => self.run_kill_bash_compat_tool(&request).await,
            _ => Err(anyhow!("unsupported tool '{}'", request.tool.id)),
        }
    }
}

#[derive(Debug, Default)]
struct SkillMarkdownMeta {
    name: Option<String>,
    description: Option<String>,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledSkillMeta {
    source: String,
    source_ref: Option<String>,
    enabled: bool,
    name: Option<String>,
    prompt_template: Option<String>,
}

impl Default for InstalledSkillMeta {
    fn default() -> Self {
        Self {
            source: "local".to_string(),
            source_ref: None,
            enabled: true,
            name: None,
            prompt_template: None,
        }
    }
}

fn parse_github_source(source: &str) -> Result<(String, Option<String>)> {
    let trimmed = source.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("github source cannot be empty");
    }

    if !trimmed.contains("://") && trimmed.matches('/').count() == 1 {
        let mut parts = trimmed.split('/');
        let owner = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default();
        if owner.is_empty() || repo.is_empty() {
            bail!("invalid github repository format");
        }
        return Ok((format!("https://github.com/{owner}/{repo}.git"), None));
    }

    let tree_regex = Regex::new(
        r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/tree/(?P<branch>[^/]+)/(?P<path>.+)$",
    )
    .expect("valid github tree regex");
    if let Some(captures) = tree_regex.captures(trimmed) {
        let owner = captures
            .name("owner")
            .map(|value| value.as_str())
            .unwrap_or("");
        let repo = captures
            .name("repo")
            .map(|value| value.as_str())
            .unwrap_or("");
        let path = captures
            .name("path")
            .map(|value| value.as_str().trim_matches('/').to_string());
        if owner.is_empty() || repo.is_empty() {
            bail!("invalid github tree URL");
        }
        return Ok((format!("https://github.com/{owner}/{repo}.git"), path));
    }

    let repo_regex =
        Regex::new(r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+?)(?:\.git)?$")
            .expect("valid github repo regex");
    if let Some(captures) = repo_regex.captures(trimmed) {
        let owner = captures
            .name("owner")
            .map(|value| value.as_str())
            .unwrap_or("");
        let repo = captures
            .name("repo")
            .map(|value| value.as_str())
            .unwrap_or("");
        if owner.is_empty() || repo.is_empty() {
            bail!("invalid github repository URL");
        }
        return Ok((format!("https://github.com/{owner}/{repo}.git"), None));
    }

    bail!("unsupported github source format");
}

fn resolve_skill_source_dir(path: &Path) -> Result<PathBuf> {
    if path.is_dir() && path.join("SKILL.md").exists() {
        return Ok(path.to_path_buf());
    }
    if !path.is_dir() {
        bail!("skill path does not exist: {}", path.display());
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() && child.join("SKILL.md").exists() {
            candidates.push(child);
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => bail!("SKILL.md not found at {}", path.display()),
        _ => bail!(
            "multiple skill folders found under {}, specify the exact skill path",
            path.display()
        ),
    }
}

fn parse_skill_markdown(content: &str) -> SkillMarkdownMeta {
    let mut meta = SkillMarkdownMeta::default();
    let frontmatter = extract_frontmatter(content);
    if let Some(frontmatter) = frontmatter {
        for line in frontmatter.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some((key, raw_value)) = trimmed.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = raw_value.trim().trim_matches('"').trim_matches('\'');
                match key.as_str() {
                    "name" => meta.name = Some(value.to_string()),
                    "description" => meta.description = Some(value.to_string()),
                    "tags" => {
                        let cleaned = value.trim().trim_start_matches('[').trim_end_matches(']');
                        let tags = cleaned
                            .split(',')
                            .map(|part| part.trim().trim_matches('"').trim_matches('\''))
                            .filter(|part| !part.is_empty())
                            .map(str::to_string);
                        meta.tags.extend(tags);
                    }
                    _ => {}
                }
            }
        }
    }

    if meta.name.is_none() {
        meta.name = content
            .lines()
            .find(|line| line.trim_start().starts_with("# "))
            .map(|line| {
                line.trim_start()
                    .trim_start_matches("# ")
                    .trim()
                    .to_string()
            });
    }
    if meta.description.is_none() {
        meta.description = content
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
            .map(str::to_string);
    }
    meta
}

fn extract_frontmatter(content: &str) -> Option<&str> {
    let mut segments = content.split_inclusive('\n');
    let first = segments.next()?;
    if first.trim() != "---" {
        return None;
    }
    let mut consumed = first.len();
    let start = consumed;
    for line in segments {
        if line.trim() == "---" {
            return content.get(start..consumed);
        }
        consumed += line.len();
    }
    None
}

fn sanitize_skill_id(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "installed-skill".to_string()
    } else {
        sanitized
    }
}

fn skill_slug_from_path(path: &Path) -> Result<String> {
    let folder = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("invalid skill directory name"))?;
    Ok(sanitize_skill_id(folder))
}

fn copy_dir_recursively(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".git" {
            continue;
        }
        let target = destination.join(&file_name);
        if path.is_dir() {
            copy_dir_recursively(&path, &target)?;
        } else {
            fs::copy(&path, &target).with_context(|| {
                format!("failed to copy {} to {}", path.display(), target.display())
            })?;
        }
    }
    Ok(())
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

fn glob_match(pattern: &str, candidate: &str) -> bool {
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

fn domain_matches(host: &str, rule: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::ToolRuntime;
    use crate::core::domain::{
        AgentPermissionPolicy, AgentProfile, MemoryPolicy, ModelPolicy, ToolExecutionRequest,
        ToolHandler,
    };
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

    fn agent() -> AgentProfile {
        AgentProfile {
            id: "agent-1".into(),
            name: "Builder".into(),
            avatar: "BD".into(),
            role: "Engineer".into(),
            objective: "Ship".into(),
            model_policy: ModelPolicy::default(),
            skill_ids: vec![],
            tool_ids: vec![
                "file.readwrite".into(),
                "project.search".into(),
                "http.request".into(),
            ],
            max_parallel_runs: 1,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        }
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
                agent: agent(),
                approval_granted: true,
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
                agent: agent(),
                approval_granted: true,
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
                agent: agent(),
                approval_granted: true,
            })
            .await
            .expect("search");
        assert!(result.output.contains("src/main.ts"));
        assert!(result.output.contains("Scout"));
    }

    #[test]
    fn skill_allowlist_blocks_network_tool_authorization() {
        let workspace_root = unique_root("skill-workspace");
        let data_root = unique_root("skill-data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
        let tool = runtime.tool_by_id("http.request").expect("tool");
        let mut restricted_agent = agent();
        restricted_agent.skill_ids = vec!["skill.builder".into()];

        let decision = runtime
            .authorize_tool_call(
                &restricted_agent,
                &tool,
                r#"{"url":"https://example.com","method":"GET"}"#,
            )
            .expect("decision");

        assert!(!decision.allowed);
        assert!(decision
            .reason
            .expect("reason")
            .contains("skills do not allow tool category"));
        assert!(runtime
            .available_tools_for_agent(&restricted_agent)
            .into_iter()
            .all(|candidate| candidate.id != "http.request"));
    }

    #[tokio::test]
    async fn file_tool_respects_agent_fs_roots() {
        let workspace_root = unique_root("permission-workspace");
        let data_root = unique_root("permission-data");
        fs::create_dir_all(workspace_root.join("allowed")).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
        let tool = runtime.tool_by_id("file.readwrite").expect("tool");
        let mut restricted_agent = agent();
        restricted_agent.permission_policy.allow_fs_roots = vec!["allowed".into()];

        let error = runtime
            .execute(ToolExecutionRequest {
                tool: tool.clone(),
                input: r#"{"path":"blocked/spec.md","mode":"write","content":"hello"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: restricted_agent.clone(),
                approval_granted: true,
            })
            .await
            .expect_err("blocked path should fail");
        assert!(error.to_string().contains("permission denied"));

        let result = runtime
            .execute(ToolExecutionRequest {
                tool,
                input: r#"{"path":"allowed/spec.md","mode":"write","content":"hello"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: restricted_agent,
                approval_granted: true,
            })
            .await
            .expect("allowed path");
        assert!(result.output.contains("\"saved\":true"));
    }
}
