use std::collections::HashSet;

use crate::core::domain::{AgentProfile, SkillPack, ToolManifest};
use crate::core::permissions::is_tool_enabled_for_agent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExposureReason {
    Available,
    NotBound,
    BlockedByPermission,
    BlockedBySkill,
}

pub fn selected_skills_for_agent(agent: &AgentProfile, skills: &[SkillPack]) -> Vec<SkillPack> {
    skills
        .iter()
        .filter(|skill| agent.skill_ids.contains(&skill.id))
        .cloned()
        .collect()
}

pub fn allowed_skill_categories(skills: &[SkillPack]) -> Option<HashSet<String>> {
    let categories = skills
        .iter()
        .flat_map(|skill| skill.allowed_tool_tags.iter().cloned())
        .collect::<HashSet<_>>();
    if categories.is_empty() {
        None
    } else {
        Some(categories)
    }
}

pub fn effective_tools_for_agent(
    agent: &AgentProfile,
    tools: &[ToolManifest],
    skills: &[SkillPack],
) -> Vec<ToolManifest> {
    let allowed_categories = allowed_skill_categories(skills);
    tools
        .iter()
        .filter(|tool| {
            is_tool_enabled_for_agent(agent, &tool.id)
                && allowed_categories
                    .as_ref()
                    .map(|categories| categories.contains(&tool.category))
                    .unwrap_or(true)
        })
        .cloned()
        .collect()
}

pub fn tool_exposure_reason(
    agent: &AgentProfile,
    tool: &ToolManifest,
    skills: &[SkillPack],
) -> ToolExposureReason {
    if !agent.tool_ids.iter().any(|tool_id| tool_id == &tool.id) {
        return ToolExposureReason::NotBound;
    }
    if !is_tool_enabled_for_agent(agent, &tool.id) {
        return ToolExposureReason::BlockedByPermission;
    }

    let allowed_categories = allowed_skill_categories(skills);
    if allowed_categories
        .as_ref()
        .map(|categories| !categories.contains(&tool.category))
        .unwrap_or(false)
    {
        return ToolExposureReason::BlockedBySkill;
    }

    ToolExposureReason::Available
}

#[cfg(test)]
mod tests {
    use super::{effective_tools_for_agent, tool_exposure_reason, ToolExposureReason};
    use crate::core::domain::{
        AgentPermissionPolicy, AgentProfile, MemoryPolicy, ModelPolicy, SkillPack, ToolManifest,
        ToolRiskLevel,
    };

    fn agent() -> AgentProfile {
        AgentProfile {
            id: "agent-1".into(),
            name: "Scout".into(),
            avatar: "SC".into(),
            role: "Research".into(),
            objective: "Find facts".into(),
            model_policy: ModelPolicy::default(),
            skill_ids: vec!["skill.research".into()],
            tool_ids: vec![
                "project.search".into(),
                "http.request".into(),
                "shell.exec".into(),
            ],
            max_parallel_runs: 1,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        }
    }

    fn tools() -> Vec<ToolManifest> {
        vec![
            ToolManifest {
                id: "project.search".into(),
                name: "Project Search".into(),
                category: "workspace".into(),
                risk_level: ToolRiskLevel::Low,
                input_schema: "{}".into(),
                output_schema: "{}".into(),
                timeout_ms: 1,
                concurrency_limit: 1,
                permissions: vec![],
                description: "search".into(),
            },
            ToolManifest {
                id: "http.request".into(),
                name: "HTTP".into(),
                category: "network".into(),
                risk_level: ToolRiskLevel::Low,
                input_schema: "{}".into(),
                output_schema: "{}".into(),
                timeout_ms: 1,
                concurrency_limit: 1,
                permissions: vec![],
                description: "http".into(),
            },
            ToolManifest {
                id: "shell.exec".into(),
                name: "Shell".into(),
                category: "system".into(),
                risk_level: ToolRiskLevel::Low,
                input_schema: "{}".into(),
                output_schema: "{}".into(),
                timeout_ms: 1,
                concurrency_limit: 1,
                permissions: vec![],
                description: "shell".into(),
            },
        ]
    }

    fn skills() -> Vec<SkillPack> {
        vec![SkillPack {
            id: "skill.research".into(),
            name: "Research".into(),
            prompt_template: "".into(),
            planning_rules: vec![],
            allowed_tool_tags: vec!["workspace".into(), "network".into()],
            done_criteria: vec![],
        }]
    }

    #[test]
    fn selected_skills_filter_visible_tools() {
        let available = effective_tools_for_agent(&agent(), &tools(), &skills());
        assert_eq!(available.len(), 2);
        assert!(available.iter().all(|tool| tool.id != "shell.exec"));
    }

    #[test]
    fn blocked_tool_reports_skill_reason() {
        let reason = tool_exposure_reason(&agent(), &tools()[2], &skills());
        assert_eq!(reason, ToolExposureReason::BlockedBySkill);
    }
}
