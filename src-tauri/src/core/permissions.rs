use crate::core::domain::{AgentProfile, ToolManifest, ToolRiskLevel};

pub const PERMISSION_DENIED_PREFIX: &str = "permission denied:";
pub const APPROVAL_REQUIRED_PREFIX: &str = "approval required:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAuthorizationDecision {
    pub allowed: bool,
    pub approval_required: bool,
    pub reason: Option<String>,
}

impl ToolAuthorizationDecision {
    pub fn allowed(approval_required: bool) -> Self {
        Self {
            allowed: true,
            approval_required,
            reason: None,
        }
    }

    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            approval_required: false,
            reason: Some(reason.into()),
        }
    }
}

pub fn is_tool_enabled_for_agent(agent: &AgentProfile, tool_id: &str) -> bool {
    agent.tool_ids.iter().any(|id| id == tool_id) && agent.permission_policy.allows_tool_id(tool_id)
}

pub fn base_tool_authorization(
    agent: &AgentProfile,
    tool: &ToolManifest,
) -> ToolAuthorizationDecision {
    if !agent.tool_ids.iter().any(|id| id == &tool.id) {
        return ToolAuthorizationDecision::denied(format!(
            "agent '{}' is not bound to tool '{}'",
            agent.name, tool.name
        ));
    }

    if !agent.permission_policy.allows_tool_id(&tool.id) {
        return ToolAuthorizationDecision::denied(format!(
            "agent policy blocks tool '{}'",
            tool.name
        ));
    }

    ToolAuthorizationDecision::allowed(
        tool.risk_level == ToolRiskLevel::High
            || agent.permission_policy.requires_approval(&tool.id),
    )
}

pub fn is_permission_guard_error(error: &str) -> bool {
    error.starts_with(PERMISSION_DENIED_PREFIX) || error.starts_with(APPROVAL_REQUIRED_PREFIX)
}
