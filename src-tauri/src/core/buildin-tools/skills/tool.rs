use std::fs;

use anyhow::{anyhow, bail, Context, Result};

use crate::core::domain::{SkillPack, ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::ToolRuntime;

use super::{
    is_local_skill_id, normalize_skill_path, parse_skill_document, parse_skill_frontmatter_summary,
    render_builtin_skill_content, required_skill_id, skill_path_within_depth,
    SkillFrontmatterSummary, SkillSummaryOutput, SkillsAction, SkillsToolInput, SkillsToolOutput,
    MAX_SKILL_FILE_BYTES, MAX_SKILL_FILE_DEPTH,
};

impl ToolRuntime {
    pub(crate) async fn run_skills_tool(
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
            .ok_or_else(|| anyhow!("Skills read_file requires a non-empty path"))?;
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
