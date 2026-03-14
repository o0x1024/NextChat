use anyhow::Result;

use crate::core::domain::SystemSettings;
use crate::core::llm_rig::simple_complete;
use crate::core::workflow::RequestRouteMode;

/// Semantic classification result from LLM.
#[derive(Debug, Clone)]
pub struct SemanticRouteClassification {
    pub route_mode: RequestRouteMode,
    pub confidence: f64,
    pub reasoning: String,
}

const CLASSIFY_SYSTEM_PROMPT: &str = r#"You are a request classifier for a multi-agent work group system.

Given a user message and the work group context, classify the request into one of these categories:

1. "direct_answer" — Simple question that can be answered directly without creating tasks.
   Examples: factual questions, explanations, comparisons, suggestions, opinions.

2. "direct_agent_assign" — A task that should be assigned directly to a specific agent.
   The user explicitly mentions or @-mentions an agent, or the task clearly maps to a single agent.

3. "owner_orchestrated" — A complex request that needs planning, decomposition into stages/tasks.
   Examples: multi-step projects, implementations, system designs, anything requiring coordination.

Respond with ONLY a JSON object (no markdown, no code fences):
{"route": "direct_answer"|"direct_agent_assign"|"owner_orchestrated", "confidence": 0.0-1.0, "reasoning": "brief explanation"}
"#;

fn build_classify_prompt(
    message: &str,
    group_name: &str,
    group_goal: &str,
    member_names: &[String],
) -> String {
    format!(
        r#"Work group: "{}" (goal: {})
Members: [{}]

User message:
{}

Classify this request."#,
        group_name,
        group_goal,
        member_names.join(", "),
        message
    )
}

pub async fn classify_route_with_llm(
    settings: &SystemSettings,
    message: &str,
    group_name: &str,
    group_goal: &str,
    member_names: &[String],
) -> Result<SemanticRouteClassification> {
    let config = settings
        .providers
        .iter()
        .find(|p| p.id == settings.global_config.default_llm_provider)
        .ok_or_else(|| anyhow::anyhow!("Default LLM provider not configured"))?;

    let model = &settings.global_config.default_llm_model;
    let proxy_url = &settings.global_config.proxy_url;

    let prompt = build_classify_prompt(message, group_name, group_goal, member_names);

    let response = simple_complete(config, proxy_url, model, CLASSIFY_SYSTEM_PROMPT, &prompt, 256).await?;

    parse_classification_response(&response)
}

fn parse_classification_response(response: &str) -> Result<SemanticRouteClassification> {
    // Strip markdown code fences if present
    let mut cleaned = response.trim();
    if let Some(rest) = cleaned.strip_prefix("```json") {
        cleaned = rest.trim();
    } else if let Some(rest) = cleaned.strip_prefix("```") {
        cleaned = rest.trim();
    }
    if let Some(rest) = cleaned.strip_suffix("```") {
        cleaned = rest.trim();
    }

    let parsed: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|e| anyhow::anyhow!("Failed to parse LLM classification response: {e}"))?;

    let route_str = parsed["route"]
        .as_str()
        .unwrap_or("owner_orchestrated");

    let route_mode = match route_str {
        "direct_answer" => RequestRouteMode::DirectAnswer,
        "direct_agent_assign" => RequestRouteMode::DirectAgentAssign,
        _ => RequestRouteMode::OwnerOrchestrated,
    };

    let confidence = parsed["confidence"].as_f64().unwrap_or(0.5);
    let reasoning = parsed["reasoning"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(SemanticRouteClassification {
        route_mode,
        confidence,
        reasoning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_classification() {
        let response = r#"{"route": "direct_answer", "confidence": 0.9, "reasoning": "simple factual question"}"#;
        let result = parse_classification_response(response).unwrap();
        assert!(matches!(result.route_mode, RequestRouteMode::DirectAnswer));
        assert!((result.confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_with_code_fences() {
        let response = "```json\n{\"route\": \"owner_orchestrated\", \"confidence\": 0.8, \"reasoning\": \"complex project\"}\n```";
        let result = parse_classification_response(response).unwrap();
        assert!(matches!(result.route_mode, RequestRouteMode::OwnerOrchestrated));
    }

    #[test]
    fn defaults_to_owner_orchestrated_on_unknown() {
        let response = r#"{"route": "unknown", "confidence": 0.3, "reasoning": "unclear"}"#;
        let result = parse_classification_response(response).unwrap();
        assert!(matches!(result.route_mode, RequestRouteMode::OwnerOrchestrated));
    }
}
