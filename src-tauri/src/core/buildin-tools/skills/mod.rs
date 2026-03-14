mod document;
mod installed;
mod tool;

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::core::domain::SkillPack;

use document::{
    build_skill_document, parse_skill_document, parse_skill_frontmatter_summary,
    parse_skill_markdown,
};

pub(super) const MAX_SKILL_FILE_BYTES: u64 = 200 * 1024;
pub(super) const MAX_SKILL_FILE_DEPTH: usize = 3;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SkillsAction {
    List,
    Load,
    ReadFile,
}

/// Legacy input format with action-based routing.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SkillsToolInput {
    pub(super) action: SkillsAction,
    pub(super) skill_id: Option<String>,
    pub(super) path: Option<String>,
}

/// New input format matching Claude Code's Skill tool schema.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SkillToolInput {
    pub(super) skill: String,
    pub(super) args: Option<String>,
}

impl SkillToolInput {
    /// Convert the new Skill input format into the legacy SkillsToolInput.
    pub(super) fn into_legacy(self) -> SkillsToolInput {
        let skill_lower = self.skill.to_lowercase();
        if skill_lower == "list" || skill_lower.is_empty() {
            SkillsToolInput {
                action: SkillsAction::List,
                skill_id: None,
                path: None,
            }
        } else if skill_lower == "read_file" {
            SkillsToolInput {
                action: SkillsAction::ReadFile,
                skill_id: self.args.clone(),
                path: self.args,
            }
        } else {
            SkillsToolInput {
                action: SkillsAction::Load,
                skill_id: Some(self.skill),
                path: self.args,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkillSummaryOutput {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) when_to_use: Option<String>,
    pub(super) can_read_files: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkillsToolOutput {
    pub(super) action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) skills: Option<Vec<SkillSummaryOutput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) skill: Option<SkillSummaryOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) runtime_hint: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct SkillFrontmatterSummary {
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) when_to_use: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct SkillMarkdownMeta {
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SkillDocument {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) argument_hint: Option<String>,
    pub(super) user_invocable: bool,
    pub(super) disable_model_invocation: bool,
    pub(super) allowed_tools: Option<String>,
    pub(super) model: Option<String>,
    pub(super) context: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) hooks_json: Option<String>,
    pub(super) summary: Option<String>,
    pub(super) content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct InstalledSkillMeta {
    pub(super) source: String,
    pub(super) source_ref: Option<String>,
    pub(super) enabled: bool,
    pub(super) name: Option<String>,
    pub(super) prompt_template: Option<String>,
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

pub(super) fn required_skill_id(skill_id: Option<&str>) -> anyhow::Result<&str> {
    skill_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Skills requires skill_id for this action"))
}

pub(super) fn is_local_skill_id(skill_id: &str) -> bool {
    skill_id.starts_with("skill.local.")
}

pub(super) fn skill_path_within_depth(path: &str) -> bool {
    Path::new(path)
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count()
        <= MAX_SKILL_FILE_DEPTH
}

pub(super) fn normalize_skill_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub(super) fn render_builtin_skill_content(skill: &SkillPack) -> String {
    let mut sections = vec![format!("# {}", skill.name.trim())];

    if !skill.prompt_template.trim().is_empty() {
        sections.push(skill.prompt_template.trim().to_string());
    }
    if !skill.planning_rules.is_empty() {
        sections.push("## Planning Rules".to_string());
        sections.extend(skill.planning_rules.iter().map(|rule| format!("- {rule}")));
    }
    if !skill.done_criteria.is_empty() {
        sections.push("## Done Criteria".to_string());
        sections.extend(skill.done_criteria.iter().map(|item| format!("- {item}")));
    }
    if !skill.allowed_tool_tags.is_empty() {
        sections.push("## Allowed Tool Tags".to_string());
        sections.push(skill.allowed_tool_tags.join(", "));
    }

    sections.join("\n\n")
}

pub(super) fn sanitize_skill_id(input: &str) -> String {
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
