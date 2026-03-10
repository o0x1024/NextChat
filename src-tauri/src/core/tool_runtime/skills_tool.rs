use std::{
    fs,
    path::{Component, Path},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::domain::{SkillPack, ToolExecutionRequest, ToolExecutionResult};

use super::{extract_frontmatter, non_empty, parse_skill_document, ToolRuntime};

const MAX_SKILL_FILE_BYTES: u64 = 200 * 1024;
const MAX_SKILL_FILE_DEPTH: usize = 3;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SkillsAction {
    List,
    Load,
    ReadFile,
}

#[derive(Debug, Clone, Deserialize)]
struct SkillsToolInput {
    action: SkillsAction,
    skill_id: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillSummaryOutput {
    id: String,
    name: String,
    description: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    when_to_use: Option<String>,
    can_read_files: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillsToolOutput {
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    skills: Option<Vec<SkillSummaryOutput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skill: Option<SkillSummaryOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime_hint: Option<String>,
}

#[derive(Debug, Default)]
struct SkillFrontmatterSummary {
    name: Option<String>,
    description: Option<String>,
    when_to_use: Option<String>,
}

impl ToolRuntime {
    pub(super) async fn run_skills_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<SkillsToolInput>(&request.input, "Skills")?;
        let output = match input.action {
            SkillsAction::List => self.list_skills_tool_output()?,
            SkillsAction::Load => self.load_skill_tool_output(input.skill_id.as_deref())?,
            SkillsAction::ReadFile => {
                self.read_skill_file_tool_output(input.skill_id.as_deref(), input.path.as_deref())?
            }
        };
        let output = serde_json::to_string(&output).context("failed to serialize Skills output")?;
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    fn list_skills_tool_output(&self) -> Result<SkillsToolOutput> {
        let mut skills = self
            .all_skills()
            .into_iter()
            .map(|skill| self.skill_summary_output(&skill))
            .collect::<Result<Vec<_>>>()?;
        skills.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(SkillsToolOutput {
            action: "list".to_string(),
            skills: Some(skills),
            skill: None,
            content: None,
            path: None,
            file_path: None,
            runtime_hint: Some(
                "Use action=load with a skillId to inspect one skill, then action=read_file for referenced local files."
                    .to_string(),
            ),
        })
    }

    fn load_skill_tool_output(&self, skill_id: Option<&str>) -> Result<SkillsToolOutput> {
        let skill_id = required_skill_id(skill_id)?;
        let skill = self
            .all_skills()
            .into_iter()
            .find(|candidate| candidate.id == skill_id)
            .with_context(|| format!("skill not found: {skill_id}"))?;
        let summary = self.skill_summary_output(&skill)?;

        if is_local_skill_id(skill_id) {
            let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
            let skill_md = skill_dir.join("SKILL.md");
            let bytes = fs::read(&skill_md)
                .with_context(|| format!("failed reading {}", skill_md.display()))?;
            let content = String::from_utf8(bytes).context("SKILL.md is not valid UTF-8")?;
            let document = parse_skill_document(&content);
            return Ok(SkillsToolOutput {
                action: "load".to_string(),
                skills: None,
                skill: Some(summary),
                content: Some(document.content),
                path: Some("SKILL.md".to_string()),
                file_path: Some(skill_md.display().to_string()),
                runtime_hint: Some(
                    "Use action=read_file with a referenced relative path to inspect additional files from this installed skill."
                        .to_string(),
                ),
            });
        }

        Ok(SkillsToolOutput {
            action: "load".to_string(),
            skills: None,
            skill: Some(summary),
            content: Some(render_builtin_skill_content(&skill)),
            path: None,
            file_path: None,
            runtime_hint: Some(
                "Builtin skills are generated from the current runtime catalog and do not expose extra files."
                    .to_string(),
            ),
        })
    }

    fn read_skill_file_tool_output(
        &self,
        skill_id: Option<&str>,
        relative_path: Option<&str>,
    ) -> Result<SkillsToolOutput> {
        let skill_id = required_skill_id(skill_id)?;
        if !is_local_skill_id(skill_id) {
            bail!("read_file is only supported for installed local skills");
        }
        let relative_path = relative_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Skills read_file requires a non-empty path"))?;
        if !skill_path_within_depth(relative_path) {
            bail!("skill file depth exceeds {MAX_SKILL_FILE_DEPTH} levels");
        }

        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let file = self.resolve_skill_file_path(&skill_dir, relative_path)?;
        let metadata = fs::metadata(&file)
            .with_context(|| format!("failed reading metadata for {}", file.display()))?;
        if metadata.len() > MAX_SKILL_FILE_BYTES {
            bail!("skill file exceeds {} bytes", MAX_SKILL_FILE_BYTES);
        }

        let bytes =
            fs::read(&file).with_context(|| format!("failed reading {}", file.display()))?;
        let content = String::from_utf8(bytes).context("skill file is not valid UTF-8")?;
        let skill = self
            .skill_pack_from_dir(&skill_dir)
            .with_context(|| format!("failed loading installed skill {skill_id}"))?;
        let summary = self.skill_summary_output(&skill)?;
        let normalized_path = normalize_skill_path(relative_path);

        Ok(SkillsToolOutput {
            action: "read_file".to_string(),
            skills: None,
            skill: Some(summary),
            content: Some(content),
            path: Some(normalized_path),
            file_path: Some(file.display().to_string()),
            runtime_hint: Some(
                "Continue calling action=read_file for other referenced files as needed."
                    .to_string(),
            ),
        })
    }

    fn skill_summary_output(&self, skill: &SkillPack) -> Result<SkillSummaryOutput> {
        let frontmatter = if is_local_skill_id(&skill.id) {
            self.read_skill_frontmatter_summary(&skill.id)
                .unwrap_or_default()
        } else {
            SkillFrontmatterSummary::default()
        };

        Ok(SkillSummaryOutput {
            id: skill.id.clone(),
            name: frontmatter.name.unwrap_or_else(|| skill.name.clone()),
            description: frontmatter
                .description
                .unwrap_or_else(|| skill.prompt_template.clone()),
            source: if skill.source.trim().is_empty() {
                "builtin".to_string()
            } else {
                skill.source.clone()
            },
            when_to_use: frontmatter.when_to_use,
            can_read_files: is_local_skill_id(&skill.id),
        })
    }

    fn read_skill_frontmatter_summary(&self, skill_id: &str) -> Result<SkillFrontmatterSummary> {
        let skill_dir = self.resolve_installed_skill_dir(skill_id)?;
        let raw = fs::read_to_string(skill_dir.join("SKILL.md"))
            .with_context(|| format!("failed reading skill metadata for {skill_id}"))?;
        Ok(parse_skill_frontmatter_summary(&raw))
    }
}

fn required_skill_id(skill_id: Option<&str>) -> Result<&str> {
    skill_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Skills requires skill_id for this action"))
}

fn is_local_skill_id(skill_id: &str) -> bool {
    skill_id.starts_with("skill.local.")
}

fn skill_path_within_depth(path: &str) -> bool {
    Path::new(path)
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count()
        <= MAX_SKILL_FILE_DEPTH
}

fn normalize_skill_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn render_builtin_skill_content(skill: &SkillPack) -> String {
    let mut sections = vec![format!("# {}", skill.name.trim())];

    if !skill.prompt_template.trim().is_empty() {
        sections.push(skill.prompt_template.trim().to_string());
    }
    if !skill.planning_rules.is_empty() {
        sections.push(format!(
            "## Planning Rules\n{}",
            skill
                .planning_rules
                .iter()
                .map(|rule| format!("- {rule}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !skill.allowed_tool_tags.is_empty() {
        sections.push(format!(
            "## Allowed Tool Categories\n{}",
            skill
                .allowed_tool_tags
                .iter()
                .map(|tag| format!("- {tag}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !skill.done_criteria.is_empty() {
        sections.push(format!(
            "## Done Criteria\n{}",
            skill
                .done_criteria
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    sections.join("\n\n")
}

fn parse_skill_frontmatter_summary(markdown: &str) -> SkillFrontmatterSummary {
    let mut summary = SkillFrontmatterSummary::default();
    let Some(frontmatter) = extract_frontmatter(markdown) else {
        return summary;
    };

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        let Some((key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        match key.as_str() {
            "name" => summary.name = non_empty(value),
            "description" => summary.description = non_empty(value),
            "when_to_use" | "when-to-use" => summary.when_to_use = non_empty(value),
            _ => {}
        }
    }
    summary
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
        std::env::temp_dir().join(format!("nextchat-skills-{prefix}-{nanos}"))
    }

    fn agent() -> AgentProfile {
        AgentProfile {
            id: "agent-1".into(),
            name: "Scout".into(),
            avatar: "SC".into(),
            role: "Research".into(),
            objective: "Inspect skills".into(),
            model_policy: ModelPolicy::default(),
            skill_ids: vec!["skill.research".into()],
            tool_ids: vec!["Skills".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        }
    }

    #[tokio::test]
    async fn skills_tool_lists_builtin_and_installed_skills() {
        let workspace_root = unique_root("workspace");
        let data_root = unique_root("data");
        let skill_root = data_root.join("skills").join("demo-skill");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&skill_root).expect("skill root");
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: Demo Skill\ndescription: Test installed skill\nwhen-to-use: When a task mentions demo\n---\n\n# Demo\n\nUse this for demo tasks.\n",
        )
        .expect("seed skill");

        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
        let result = runtime
            .execute(ToolExecutionRequest {
                tool: runtime.tool_by_id("Skills").expect("tool"),
                input: r#"{"action":"list"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect("list");

        assert!(result.output.contains("\"skill.research\""));
        assert!(result.output.contains("\"skill.local.demo-skill\""));
        assert!(result.output.contains("When a task mentions demo"));
    }

    #[tokio::test]
    async fn skills_tool_loads_and_reads_local_skill_files() {
        let workspace_root = unique_root("workspace-load");
        let data_root = unique_root("data-load");
        let skill_root = data_root.join("skills").join("demo-skill");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(skill_root.join("docs")).expect("skill docs");
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: Demo Skill\ndescription: Test installed skill\n---\n\n# Demo\n\nSee docs/reference.md for details.\n",
        )
        .expect("seed skill");
        fs::write(skill_root.join("docs/reference.md"), "reference content\n").expect("seed doc");

        let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
        let tool = runtime.tool_by_id("Skills").expect("tool");

        let load_result = runtime
            .execute(ToolExecutionRequest {
                tool: tool.clone(),
                input: r#"{"action":"load","skill_id":"skill.local.demo-skill"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect("load");
        assert!(load_result
            .output
            .contains("See docs/reference.md for details."));
        assert!(load_result.output.contains("\"path\":\"SKILL.md\""));

        let read_result = runtime
            .execute(ToolExecutionRequest {
                tool,
                input: r#"{"action":"read_file","skill_id":"skill.local.demo-skill","path":"docs/reference.md"}"#.into(),
                task_card_id: "task-1".into(),
                agent_id: "agent-1".into(),
                agent: agent(),
                approval_granted: true,
                working_directory: ".".into(),
                tool_stream: None,
            })
            .await
            .expect("read_file");
        assert!(read_result.output.contains("reference content"));
        assert!(read_result
            .output
            .contains("\"path\":\"docs/reference.md\""));
    }
}
