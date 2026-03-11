use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{anyhow, bail, Context, Result};
use tokio::process::Command;
use walkdir::WalkDir;

use crate::core::domain::{SkillDetail, SkillFileEntry, SkillPack, UpdateSkillDetailInput};
use crate::core::tool_runtime::ToolRuntime;

use super::{
    build_skill_document, parse_skill_document, parse_skill_markdown, sanitize_skill_id,
    InstalledSkillMeta, SkillDocument,
};

impl ToolRuntime {
    pub fn skills_root(&self) -> PathBuf {
        self.app_data_dir.join("skills")
    }

    pub fn installed_skill_meta_path(skill_dir: &Path) -> PathBuf {
        skill_dir.join(".nextchat-skill.json")
    }

    pub fn installed_skills(&self) -> Vec<SkillPack> {
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

    pub(super) fn skill_pack_from_dir(&self, skill_dir: &Path) -> Result<SkillPack> {
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

    pub(super) fn resolve_installed_skill_dir(&self, skill_id: &str) -> Result<PathBuf> {
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

    pub(super) fn list_skill_files(&self, skill_dir: &Path) -> Result<Vec<SkillFileEntry>> {
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

    pub(super) fn resolve_skill_file_path(
        &self,
        skill_dir: &Path,
        relative_path: &str,
    ) -> Result<PathBuf> {
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

    let tree_regex = regex::Regex::new(
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
        regex::Regex::new(r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+?)(?:\.git)?$")
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
