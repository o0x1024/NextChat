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
    AgentProfile, SkillDetail, SkillFileEntry, SkillPack, ToolExecutionRequest,
    ToolExecutionResult, ToolHandler, ToolManifest, ToolStreamChunk, UpdateSkillDetailInput,
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
                .find(|tool| {
                    tool.id == "TodoWrite"
                        && (allowed_tool_ids.is_empty() || allowed_tool_ids.contains(&tool.id))
                })
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

    pub fn get_installed_skill_detail(&self, skill_id: &str) -> Result<SkillDetail> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let skill_pack = self.skill_pack_from_dir(&skill_dir)?;
        let raw = fs::read_to_string(skill_dir.join("SKILL.md"))
            .with_context(|| format!("failed reading {}", skill_dir.join("SKILL.md").display()))?;
        let document = parse_skill_document(&raw);
        Ok(SkillDetail {
            skill_id: skill_id.to_string(),
            enabled: skill_pack.enabled,
            source: skill_pack.source,
            install_path: skill_dir.display().to_string(),
            name: skill_pack.name,
            description: skill_pack.prompt_template,
            argument_hint: document.argument_hint,
            user_invocable: document.user_invocable,
            disable_model_invocation: document.disable_model_invocation,
            allowed_tools: document.allowed_tools,
            model: document.model,
            context: document.context,
            agent: document.agent,
            hooks_json: document.hooks_json,
            summary: document.summary,
            content: document.content,
            files: self.list_skill_files(&skill_dir)?,
        })
    }

    pub fn update_skill_detail(&self, input: UpdateSkillDetailInput) -> Result<SkillDetail> {
        let skill_dir = self.resolve_installed_skill_dir(&input.skill_id)?;
        let mut meta = self.load_skill_meta(&skill_dir)?;
        meta.enabled = input.enabled;
        meta.name = Some(input.name.trim().to_string());
        meta.prompt_template = Some(input.description.trim().to_string());
        self.save_skill_meta(&skill_dir, &meta)?;

        let document = SkillDocument {
            name: input.name,
            description: input.description,
            argument_hint: input.argument_hint,
            user_invocable: input.user_invocable,
            disable_model_invocation: input.disable_model_invocation,
            allowed_tools: input.allowed_tools,
            model: input.model,
            context: input.context,
            agent: input.agent,
            hooks_json: input.hooks_json,
            summary: input.summary,
            content: input.content,
        };
        fs::write(skill_dir.join("SKILL.md"), build_skill_document(&document))?;
        self.get_installed_skill_detail(&input.skill_id)
    }

    pub fn read_installed_skill_file(&self, skill_id: &str, relative_path: &str) -> Result<String> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let file = self.resolve_skill_file_path(&skill_dir, relative_path)?;
        if !file.exists() || !file.is_file() {
            bail!("file not found: {}", relative_path);
        }
        let bytes = fs::read(&file)?;
        if std::str::from_utf8(&bytes).is_err() {
            bail!("file is binary and cannot be edited as text");
        }
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    pub fn upsert_installed_skill_file(
        &self,
        skill_id: &str,
        relative_path: &str,
        content: &str,
    ) -> Result<()> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let file = self.resolve_skill_file_path(&skill_dir, relative_path)?;
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, content.as_bytes())?;
        Ok(())
    }

    pub fn delete_installed_skill_file(&self, skill_id: &str, relative_path: &str) -> Result<()> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let file = self.resolve_skill_file_path(&skill_dir, relative_path)?;
        if !file.exists() {
            bail!("file not found: {}", relative_path);
        }
        if file.is_dir() {
            fs::remove_dir_all(file)?;
        } else {
            fs::remove_file(file)?;
        }
        Ok(())
    }

    pub fn install_skill_from_local_path(&self, source_path: &str) -> Result<Vec<SkillPack>> {
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
        self.install_skills_from_root(
            &source_dir,
            InstalledSkillMeta::default(),
            Some("local source"),
        )
    }

    pub async fn install_skill_from_github(
        &self,
        source: &str,
        skill_path: Option<&str>,
    ) -> Result<Vec<SkillPack>> {
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
        if !candidate.exists() {
            let _ = fs::remove_dir_all(&clone_dir);
            bail!("provided skill path does not exist in repository");
        }
        let result = self.install_skills_from_root(
            &candidate,
            InstalledSkillMeta {
                source: "github".into(),
                source_ref: Some(source.to_string()),
                enabled: true,
                name: None,
                prompt_template: None,
            },
            Some("github source"),
        );
        let _ = fs::remove_dir_all(&clone_dir);
        result
    }

    fn install_skills_from_root(
        &self,
        root: &Path,
        meta: InstalledSkillMeta,
        source_label: Option<&str>,
    ) -> Result<Vec<SkillPack>> {
        let root = root
            .canonicalize()
            .with_context(|| format!("invalid root path: {}", root.display()))?;
        let slug_base = if root.is_file() {
            root.parent()
                .ok_or_else(|| anyhow!("invalid source file path"))?
                .to_path_buf()
        } else {
            root.clone()
        };
        let skill_dirs = discover_skill_dirs(&root)?;
        if skill_dirs.is_empty() {
            bail!(
                "no skills found in {}: {}",
                source_label.unwrap_or("source"),
                root.display()
            );
        }

        let mut installed = Vec::new();
        for skill_dir in skill_dirs {
            let slug = skill_slug_from_root_and_dir(&slug_base, &skill_dir)?;
            let skill = self.install_skill_from_dir(&skill_dir, &slug, meta.clone())?;
            installed.push(skill);
        }
        installed.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(installed)
    }

    fn install_skill_from_dir(
        &self,
        source_dir: &Path,
        slug: &str,
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

    fn list_skill_files(&self, skill_dir: &Path) -> Result<Vec<SkillFileEntry>> {
        let mut files = Vec::new();
        let iter = WalkDir::new(skill_dir).into_iter().filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git")
        });
        for entry in iter.filter_map(Result::ok) {
            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if matches!(name, "SKILL.md" | ".nextchat-skill.json") {
                continue;
            }
            let relative = path
                .strip_prefix(skill_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let bytes = fs::read(path).unwrap_or_default();
            files.push(SkillFileEntry {
                path: relative,
                size: bytes.len() as i64,
                is_binary: std::str::from_utf8(&bytes).is_err(),
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn resolve_skill_file_path(&self, skill_dir: &Path, relative_path: &str) -> Result<PathBuf> {
        let trimmed = relative_path.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            bail!("relative file path is empty");
        }
        if matches!(trimmed, "SKILL.md" | ".nextchat-skill.json") {
            bail!("protected file cannot be edited from file list");
        }
        let candidate = skill_dir.join(trimmed);
        let normalized = if candidate.exists() {
            candidate.canonicalize().unwrap_or(candidate)
        } else {
            candidate
        };
        if !normalized.starts_with(skill_dir) {
            bail!("path escapes skill directory");
        }
        Ok(normalized)
    }

    fn is_path_within_execution_scope(&self, path: &Path, execution_root: &Path) -> bool {
        path.starts_with(execution_root) || path.starts_with(&self.app_data_dir)
    }

    pub fn normalize_working_directory(&self, raw: &str) -> Result<String> {
        let execution_root = self.resolve_execution_root(raw)?;
        Ok(execution_root.display().to_string())
    }

    fn resolve_execution_root(&self, raw: &str) -> Result<PathBuf> {
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

    fn resolve_permission_root(&self, raw: &str, execution_root: &Path) -> PathBuf {
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

    fn resolve_path(
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

    async fn run_shell_command_streaming(
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

    async fn run_glob_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<GlobToolInput>(&request.input, "Glob")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.pattern.trim().is_empty() {
            bail!("glob pattern is required");
        }
        let root = if let Some(path) = input.path {
            self.resolve_path(path.trim(), false, &execution_root)?
        } else {
            execution_root
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.pattern.trim().is_empty() {
            bail!("grep pattern is required");
        }
        let root = if let Some(path) = input.path.clone() {
            self.resolve_path(path.trim(), false, &execution_root)?
        } else {
            execution_root.clone()
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
                .strip_prefix(&execution_root)
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(input.path.trim(), false, &execution_root)?;
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(&input.file_path, false, &execution_root)?;
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.old_string == input.new_string {
            bail!("old_string and new_string cannot be the same");
        }
        let path = self.resolve_path(&input.file_path, false, &execution_root)?;
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.edits.is_empty() {
            bail!("MultiEdit requires at least one edit");
        }
        let path = self.resolve_path(&input.file_path, false, &execution_root)?;
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(&input.file_path, true, &execution_root)?;
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
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(&input.notebook_path, false, &execution_root)?;
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
        let decision = self.authorize_tool_call(
            &request.agent,
            &request.tool,
            &request.input,
            &request.working_directory,
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

        match request.tool.id.as_str() {
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

#[derive(Debug, Clone, Default)]
struct SkillDocument {
    name: String,
    description: String,
    argument_hint: Option<String>,
    user_invocable: bool,
    disable_model_invocation: bool,
    allowed_tools: Option<String>,
    model: Option<String>,
    context: Option<String>,
    agent: Option<String>,
    hooks_json: Option<String>,
    summary: Option<String>,
    content: String,
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

fn discover_skill_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if root.is_file() {
        let is_skill_md = root
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("SKILL.md"))
            .unwrap_or(false);
        if is_skill_md {
            if let Some(parent) = root.parent() {
                found.push(parent.to_path_buf());
            }
        }
        return Ok(found);
    }

    if !root.is_dir() {
        bail!("skill source not found: {}", root.display());
    }

    let iter = WalkDir::new(root).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        !matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist")
    });
    for entry in iter.filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_dir() {
            continue;
        }
        if path.join("SKILL.md").exists() {
            found.push(path.to_path_buf());
        }
    }
    found.sort();
    Ok(found)
}

fn parse_skill_markdown(content: &str) -> SkillMarkdownMeta {
    let mut meta = SkillMarkdownMeta::default();
    let document = parse_skill_document(content);
    if !document.name.trim().is_empty() {
        meta.name = Some(document.name);
    }
    if !document.description.trim().is_empty() {
        meta.description = Some(document.description);
    }
    if let Some(frontmatter) = extract_frontmatter(content) {
        for line in frontmatter.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("tags:") {
                continue;
            }
            if let Some((_, raw_value)) = trimmed.split_once(':') {
                let cleaned = raw_value
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']');
                let tags = cleaned
                    .split(',')
                    .map(|part| part.trim().trim_matches('"').trim_matches('\''))
                    .filter(|part| !part.is_empty())
                    .map(str::to_string);
                meta.tags.extend(tags);
            }
        }
    }
    meta
}

fn parse_skill_document(markdown: &str) -> SkillDocument {
    let mut document = SkillDocument {
        user_invocable: true,
        disable_model_invocation: false,
        ..SkillDocument::default()
    };

    if let Some(frontmatter) = extract_frontmatter(markdown) {
        for line in frontmatter.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let Some((key, raw_value)) = trimmed.split_once(':') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            let value = raw_value.trim().trim_matches('"').trim_matches('\'');
            match key.as_str() {
                "name" => document.name = value.to_string(),
                "description" => document.description = value.to_string(),
                "argument_hint" | "argument-hint" => {
                    document.argument_hint = non_empty(value);
                }
                "user_invocable" | "user-invocable" => {
                    document.user_invocable = parse_bool(value, true);
                }
                "disable_model_invocation" | "disable-model-invocation" => {
                    document.disable_model_invocation = parse_bool(value, false);
                }
                "allowed_tools" | "allowed-tools" => {
                    document.allowed_tools = non_empty(value);
                }
                "model" => document.model = non_empty(value),
                "context" => document.context = non_empty(value),
                "agent" => document.agent = non_empty(value),
                "hooks" => document.hooks_json = non_empty(value),
                "summary" => document.summary = non_empty(value),
                _ => {}
            }
        }
    }

    let body = strip_frontmatter(markdown);
    if document.name.is_empty() {
        document.name = body
            .lines()
            .find(|line| line.trim_start().starts_with("# "))
            .map(|line| {
                line.trim_start()
                    .trim_start_matches("# ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
    }
    if document.description.is_empty() {
        document.description = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
            .map(str::to_string)
            .unwrap_or_default();
    }
    document.content = body.trim().to_string();
    document
}

fn build_skill_document(document: &SkillDocument) -> String {
    let mut lines = vec![
        "---".to_string(),
        format!("name: {}", document.name.trim()),
        format!("description: {}", document.description.trim()),
        format!("user-invocable: {}", document.user_invocable),
        format!(
            "disable-model-invocation: {}",
            document.disable_model_invocation
        ),
    ];

    if let Some(value) = non_empty(document.argument_hint.as_deref().unwrap_or("")) {
        lines.push(format!("argument-hint: {}", value));
    }
    if let Some(value) = non_empty(document.allowed_tools.as_deref().unwrap_or("")) {
        lines.push(format!("allowed-tools: {}", value));
    }
    if let Some(value) = non_empty(document.model.as_deref().unwrap_or("")) {
        lines.push(format!("model: {}", value));
    }
    if let Some(value) = non_empty(document.context.as_deref().unwrap_or("")) {
        lines.push(format!("context: {}", value));
    }
    if let Some(value) = non_empty(document.agent.as_deref().unwrap_or("")) {
        lines.push(format!("agent: {}", value));
    }
    if let Some(value) = non_empty(document.hooks_json.as_deref().unwrap_or("")) {
        lines.push(format!("hooks: {}", value));
    }
    if let Some(value) = non_empty(document.summary.as_deref().unwrap_or("")) {
        lines.push(format!("summary: {}", value));
    }
    lines.push("---".to_string());
    lines.push(String::new());
    if !document.content.trim().is_empty() {
        lines.push(document.content.trim().to_string());
    }
    lines.join("\n")
}

fn strip_frontmatter(markdown: &str) -> String {
    let mut lines = markdown.lines();
    if lines.next().map(str::trim) != Some("---") {
        return markdown.to_string();
    }
    let mut output = Vec::new();
    let mut in_frontmatter = true;
    for line in markdown.lines().skip(1) {
        if in_frontmatter && line.trim() == "---" {
            in_frontmatter = false;
            continue;
        }
        if !in_frontmatter {
            output.push(line);
        }
    }
    output.join("\n")
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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

fn skill_slug_from_root_and_dir(root: &Path, skill_dir: &Path) -> Result<String> {
    if root == skill_dir {
        let folder = skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("installed-skill");
        return Ok(sanitize_skill_id(folder));
    }
    let relative = skill_dir.strip_prefix(root).with_context(|| {
        format!(
            "failed to resolve relative path for {}",
            skill_dir.display()
        )
    })?;
    let relative_text = relative
        .components()
        .map(|item| item.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("-");
    Ok(sanitize_skill_id(&relative_text))
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
                "Read".into(),
                "Write".into(),
                "Grep".into(),
                "WebFetch".into(),
            ],
            max_parallel_runs: 1,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        }
    }

    #[tokio::test]
    async fn read_and_write_tools_work() {
        let workspace_root = unique_root("workspace");
        let data_root = unique_root("data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root.clone(), data_root).expect("runtime");
        let write_tool = runtime.tool_by_id("Write").expect("tool");
        let read_tool = runtime.tool_by_id("Read").expect("tool");

        let write_result = runtime
            .execute(ToolExecutionRequest {
                tool: write_tool,
                input: r#"{"file_path":"notes/spec.md","content":"hello"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect("write");
        assert!(write_result.output.contains("\"status\":\"ok\""));

        let read_result = runtime
            .execute(ToolExecutionRequest {
                tool: read_tool,
                input: r#"{"file_path":"notes/spec.md"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect("read");
        assert!(read_result.output.contains("hello"));
    }

    #[tokio::test]
    async fn grep_returns_matches() {
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
                tool: runtime.tool_by_id("Grep").expect("tool"),
                input: r#"{"pattern":"Scout","output_mode":"content"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
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
        let tool = runtime.tool_by_id("WebFetch").expect("tool");
        let mut restricted_agent = agent();
        restricted_agent.skill_ids = vec!["skill.builder".into()];

        let decision = runtime
            .authorize_tool_call(
                &restricted_agent,
                &tool,
                r#"{"url":"https://example.com","prompt":"summarize"}"#,
                ".",
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
            .all(|candidate| candidate.id != "WebFetch"));
    }

    #[test]
    fn tool_selection_does_not_fallback_to_todowrite_when_not_allowed() {
        let workspace_root = unique_root("selection-workspace");
        let data_root = unique_root("selection-data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

        let selected = runtime.select_tool_for_text("please handle this request", &["Read".into()]);
        assert!(selected.is_none(), "unexpected fallback tool selected");
    }

    #[test]
    fn tool_selection_falls_back_to_todowrite_when_allowed() {
        let workspace_root = unique_root("selection-workspace-allowed");
        let data_root = unique_root("selection-data-allowed");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

        let selected = runtime
            .select_tool_for_text("please handle this request", &["TodoWrite".into()])
            .expect("fallback tool");
        assert_eq!(selected.id, "TodoWrite");
    }

    #[tokio::test]
    async fn file_tool_respects_agent_fs_roots() {
        let workspace_root = unique_root("permission-workspace");
        let data_root = unique_root("permission-data");
        fs::create_dir_all(workspace_root.join("allowed")).expect("workspace");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
        let tool = runtime.tool_by_id("Write").expect("tool");
        let mut restricted_agent = agent();
        restricted_agent.permission_policy.allow_fs_roots = vec!["allowed".into()];

        let error = runtime
            .execute(ToolExecutionRequest {
                tool: tool.clone(),
                input: r#"{"file_path":"blocked/spec.md","content":"hello"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: restricted_agent.clone(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect_err("blocked path should fail");
        assert!(error.to_string().contains("permission denied"));

        let result = runtime
            .execute(ToolExecutionRequest {
                tool,
                input: r#"{"file_path":"allowed/spec.md","content":"hello"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: restricted_agent,
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect("allowed path");
        assert!(result.output.contains("\"status\":\"ok\""));
    }

    #[tokio::test]
    async fn tools_support_working_directories_outside_workspace_root() {
        let workspace_root = unique_root("workspace-root");
        let external_root = unique_root("external-root");
        let data_root = unique_root("external-data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&external_root).expect("external");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

        let result = runtime
            .execute(ToolExecutionRequest {
                tool: runtime.tool_by_id("Write").expect("tool"),
                input: r#"{"file_path":"notes/from_external.md","content":"hello external"}"#
                    .into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: external_root.display().to_string(),
                tool_stream: None,
            })
            .await
            .expect("write in external working directory");
        assert!(result.output.contains("\"status\":\"ok\""));
        assert!(external_root.join("notes/from_external.md").exists());
    }

    #[tokio::test]
    async fn file_paths_cannot_escape_working_directory_scope() {
        let workspace_root = unique_root("escape-workspace");
        let external_root = unique_root("escape-external");
        let data_root = unique_root("escape-data");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&external_root).expect("external");
        fs::create_dir_all(&data_root).expect("data");
        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

        let error = runtime
            .execute(ToolExecutionRequest {
                tool: runtime.tool_by_id("Write").expect("tool"),
                input: r#"{"file_path":"../outside.md","content":"nope"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: external_root.display().to_string(),
                tool_stream: None,
            })
            .await
            .expect_err("escape should fail");
        assert!(
            error.to_string().contains("outside working directory"),
            "unexpected error: {}",
            error
        );
    }
}
