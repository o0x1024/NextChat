use anyhow::Result;
use reqwest::Url;

use super::ToolRuntime;
use crate::core::domain::{AgentProfile, ToolManifest};
use crate::core::permissions::{base_tool_authorization, ToolAuthorizationDecision};

impl ToolRuntime {
    pub fn authorize_tool_call(
        &self,
        agent: &AgentProfile,
        tool: &ToolManifest,
        input: &str,
        working_directory: &str,
    ) -> Result<ToolAuthorizationDecision> {
        let input = self.normalize_compat_input(tool, input);
        let execution_root = self.resolve_execution_root(working_directory)?;

        let mut decision = base_tool_authorization(agent, tool);
        if !decision.allowed {
            return Ok(decision);
        }

        match tool.id.as_str() {
            "Read" | "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "LS" => {
                let parsed = serde_json::from_str::<serde_json::Value>(&input).ok();
                let path = match tool.id.as_str() {
                    "Read" | "Edit" | "MultiEdit" | "Write" | "LS" => parsed
                        .as_ref()
                        .and_then(|value| value.get("file_path").or_else(|| value.get("path")))
                        .and_then(|value| value.as_str())
                        .unwrap_or(""),
                    "NotebookEdit" => parsed
                        .as_ref()
                        .and_then(|value| value.get("notebook_path"))
                        .and_then(|value| value.as_str())
                        .unwrap_or(""),
                    _ => "",
                };
                if !path.is_empty() {
                    let create_parent = matches!(tool.id.as_str(), "Write");
                    let path = self.resolve_path(path, create_parent, &execution_root)?;
                    let allowed_roots = &agent.permission_policy.allow_fs_roots;
                    if !allowed_roots.is_empty()
                        && !allowed_roots
                            .iter()
                            .map(|root| self.resolve_permission_root(root, &execution_root))
                            .any(|root| path.starts_with(root))
                    {
                        decision = ToolAuthorizationDecision::denied(format!(
                            "path '{}' is outside allowFsRoots",
                            path.display()
                        ));
                    }
                }
            }
            "WebFetch" | "WebSearch" => {
                let parsed = serde_json::from_str::<serde_json::Value>(&input).ok();
                let url = if tool.id == "WebFetch" {
                    parsed
                        .as_ref()
                        .and_then(|value| value.get("url"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    "https://duckduckgo.com".to_string()
                };
                let allowed_domains = &agent.permission_policy.allow_network_domains;
                if !allowed_domains.is_empty() {
                    let host = Url::parse(&url)
                        .ok()
                        .and_then(|value| value.host_str().map(str::to_string))
                        .unwrap_or_default();
                    let host_allowed = allowed_domains.iter().any(|candidate| {
                        let normalized = candidate
                            .trim()
                            .trim_start_matches("http://")
                            .trim_start_matches("https://")
                            .trim_start_matches('*')
                            .trim_start_matches('.')
                            .trim_end_matches('/')
                            .to_lowercase();
                        let host = host.to_lowercase();
                        host == normalized || host.ends_with(&format!(".{normalized}"))
                    });
                    if !host_allowed {
                        decision = ToolAuthorizationDecision::denied(format!(
                            "domain '{}' is outside allowNetworkDomains",
                            host
                        ));
                    }
                }
            }
            _ => {}
        }

        Ok(decision)
    }
}
