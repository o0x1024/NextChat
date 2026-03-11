use crate::core::domain::{AgentProfile, SkillPack, ToolManifest};
use crate::core::permissions::is_tool_enabled_for_agent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExposureReason {
    Available,
    NotBound,
    BlockedByPermission,
}

pub fn selected_skills_for_agent(agent: &AgentProfile, skills: &[SkillPack]) -> Vec<SkillPack> {
    if !is_tool_enabled_for_agent(agent, "Skills") {
        return Vec::new();
    }

    skills
        .iter()
        .filter(|skill| skill.enabled)
        .cloned()
        .collect()
}

pub fn effective_tools_for_agent(
    agent: &AgentProfile,
    tools: &[ToolManifest],
    _skills: &[SkillPack],
) -> Vec<ToolManifest> {
    tools
        .iter()
        .filter(|tool| is_tool_enabled_for_agent(agent, &tool.id))
        .cloned()
        .collect()
}

pub fn tool_exposure_reason(
    agent: &AgentProfile,
    tool: &ToolManifest,
    _skills: &[SkillPack],
) -> ToolExposureReason {
    if !is_tool_enabled_for_agent(agent, &tool.id)
        && !agent
            .tool_ids
            .iter()
            .any(|tool_id| compat_tool_binding_matches(tool_id, &tool.id))
    {
        return ToolExposureReason::NotBound;
    }
    if !is_tool_enabled_for_agent(agent, &tool.id) {
        return ToolExposureReason::BlockedByPermission;
    }

    ToolExposureReason::Available
}

fn compat_tool_binding_matches(bound_tool_id: &str, requested_tool_id: &str) -> bool {
    matches!(
        (bound_tool_id, requested_tool_id),
        ("shell.exec", "Bash")
            | ("file.readwrite", "Read" | "Write" | "Edit" | "MultiEdit")
            | ("project.search", "Grep" | "Glob" | "LS")
            | ("http.request", "WebFetch" | "WebSearch")
            | ("plan.summarize", "TodoWrite")
    )
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
            skill_ids: vec![],
            tool_ids: vec![
                "Skills".into(),
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
            enabled: true,
            editable: false,
            source: "builtin".into(),
            install_path: None,
        }]
    }

    #[test]
    fn selected_skills_filter_visible_tools() {
        let available = effective_tools_for_agent(&agent(), &tools(), &skills());
        assert_eq!(available.len(), 3);
    }

    #[test]
    fn missing_tool_reports_not_bound() {
        let mut agent = agent();
        agent.tool_ids.retain(|tool_id| tool_id != "shell.exec");
        let reason = tool_exposure_reason(&agent, &tools()[2], &skills());
        assert_eq!(reason, ToolExposureReason::NotBound);
    }
}
