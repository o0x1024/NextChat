use crate::core::domain::{AgentProfile, ToolManifest};

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
    agent
        .tool_ids
        .iter()
        .any(|id| tool_binding_matches(id, tool_id))
}

pub fn base_tool_authorization(
    agent: &AgentProfile,
    tool: &ToolManifest,
) -> ToolAuthorizationDecision {
    if !agent
        .tool_ids
        .iter()
        .any(|id| tool_binding_matches(id, &tool.id))
    {
        return ToolAuthorizationDecision::denied(format!(
            "agent '{}' is not bound to tool '{}'",
            agent.name, tool.name
        ));
    }

    ToolAuthorizationDecision::allowed(false)
}

pub fn is_permission_guard_error(error: &str) -> bool {
    error.starts_with(PERMISSION_DENIED_PREFIX) || error.starts_with(APPROVAL_REQUIRED_PREFIX)
}

fn tool_binding_matches(bound_tool_id: &str, requested_tool_id: &str) -> bool {
    bound_tool_id == requested_tool_id
        || compat_requested_ids(bound_tool_id)
            .iter()
            .any(|candidate| *candidate == requested_tool_id)
        || compat_requested_ids(requested_tool_id)
            .iter()
            .any(|candidate| *candidate == bound_tool_id)
}

fn compat_requested_ids(tool_id: &str) -> &'static [&'static str] {
    match tool_id {
        "shell.exec" => &["Bash"],
        "file.readwrite" => &["Read", "Write", "Edit"],
        "project.search" => &["Grep", "Glob", "LS"],
        "http.request" => &["WebFetch", "WebSearch"],
        "plan.summarize" => &["TaskCreate", "TaskGet", "TaskUpdate", "TaskList"],
        "Bash" => &["shell.exec"],
        "Read" | "Write" | "Edit" => &["file.readwrite"],
        "Grep" | "Glob" | "LS" => &["project.search"],
        "WebFetch" | "WebSearch" => &["http.request"],
        "TaskCreate" | "TaskGet" | "TaskUpdate" | "TaskList" => &["plan.summarize"],
        _ => &[],
    }
}
