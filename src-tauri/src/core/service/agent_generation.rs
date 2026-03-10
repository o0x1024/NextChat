use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::core::{
    domain::{
        AIProviderConfig, CreateAgentInput, ModelPolicy, ModelProviderAdapter, SystemSettings,
    },
    llm_rig::RigModelAdapter,
};

use super::AppService;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedAgentDraft {
    name: Option<String>,
    avatar: Option<String>,
    role: Option<String>,
    objective: Option<String>,
    skill_ids: Option<Vec<String>>,
    tool_ids: Option<Vec<String>>,
    max_parallel_runs: Option<i64>,
    can_spawn_subtasks: Option<bool>,
}

impl AppService {
    pub async fn generate_agent_profile(&self, prompt: &str) -> Result<CreateAgentInput> {
        let trimmed_prompt = prompt.trim();
        if trimmed_prompt.is_empty() {
            return Err(anyhow!("agent generation prompt cannot be empty"));
        }

        let settings = self.storage.get_settings()?;
        let skills = self.tool_runtime.all_skills();
        let tools = self.tool_runtime.builtin_tools();

        let skill_catalog = if skills.is_empty() {
            "- none".to_string()
        } else {
            skills
                .iter()
                .map(|skill| format!("- {}: {}", skill.id, truncate_text(&skill.name, 80)))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let tool_catalog = if tools.is_empty() {
            "- none".to_string()
        } else {
            tools
                .iter()
                .map(|tool| {
                    format!(
                        "- {}: {} ({})",
                        tool.id,
                        truncate_text(&tool.name, 80),
                        tool.category
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let preamble =
            "You design agent profiles for a multi-agent workspace. Output valid JSON only.";
        let generation_prompt = format!(
            "User request:\n{}\n\nReturn exactly one JSON object with keys name, avatar, role, objective, skillIds, toolIds, maxParallelRuns, canSpawnSubtasks.\nRules:\n- name/role/objective should be concise and practical.\n- avatar should be 1-2 uppercase letters.\n- skillIds must come from this list:\n{}\n- toolIds must come from this list:\n{}\n- maxParallelRuns should be 1-8.\n- No markdown, comments, or extra keys.",
            trimmed_prompt, skill_catalog, tool_catalog
        );

        let (reply, model_policy) =
            complete_with_best_provider(&settings, preamble, &generation_prompt).await?;
        let draft = parse_generated_agent_draft(&reply)?;

        let mut skill_ids = Vec::new();
        for skill_id in draft.skill_ids.unwrap_or_default() {
            if skills.iter().any(|skill| skill.id == skill_id) && !skill_ids.contains(&skill_id) {
                skill_ids.push(skill_id);
            }
        }

        let mut tool_ids = Vec::new();
        for tool_id in draft.tool_ids.unwrap_or_default() {
            if tools.iter().any(|tool| tool.id == tool_id) && !tool_ids.contains(&tool_id) {
                tool_ids.push(tool_id);
            }
        }

        let name = non_empty_or_default(
            draft.name,
            truncated_chars(trimmed_prompt.lines().next().unwrap_or("AI Agent"), 40),
        );
        let role = non_empty_or_default(draft.role, "General-purpose specialist".to_string());
        let objective = non_empty_or_default(
            draft.objective,
            format!(
                "Handle requests related to: {}",
                truncated_chars(trimmed_prompt, 120)
            ),
        );
        let avatar = non_empty_or_default(draft.avatar, initials(&name));

        self.record_audit(
            "agent.generated",
            "agent",
            "draft",
            json!({
                "provider": model_policy.provider,
                "model": model_policy.model,
                "name": name,
                "skillCount": skill_ids.len(),
                "toolCount": tool_ids.len(),
            }),
        )?;

        Ok(CreateAgentInput {
            name,
            avatar: truncated_chars(&avatar.to_uppercase(), 2),
            role,
            objective,
            provider: model_policy.provider,
            model: model_policy.model,
            temperature: model_policy.temperature,
            skill_ids,
            tool_ids,
            max_parallel_runs: draft.max_parallel_runs.unwrap_or(2).clamp(1, 8),
            can_spawn_subtasks: draft.can_spawn_subtasks.unwrap_or(true),
            memory_policy: Default::default(),
            permission_policy: Default::default(),
        })
    }
}

fn parse_generated_agent_draft(raw: &str) -> Result<GeneratedAgentDraft> {
    let payload = extract_json_object(raw)
        .ok_or_else(|| anyhow!("model did not return a valid JSON object"))?;
    serde_json::from_str(payload)
        .with_context(|| "failed to parse generated agent profile JSON".to_string())
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (end > start).then_some(&trimmed[start..=end])
}

fn non_empty_or_default(value: Option<String>, fallback: String) -> String {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or(fallback)
}

fn truncated_chars(value: &str, max_len: usize) -> String {
    value.chars().take(max_len).collect()
}

fn truncate_text(value: &str, max_len: usize) -> String {
    let single_line = value.lines().next().unwrap_or_default().trim();
    if single_line.chars().count() <= max_len {
        return single_line.to_string();
    }
    format!(
        "{}...",
        truncated_chars(single_line, max_len.saturating_sub(3))
    )
}

fn initials(name: &str) -> String {
    let letters = name
        .split_whitespace()
        .filter_map(|chunk| chunk.chars().next())
        .take(2)
        .collect::<String>();
    if letters.is_empty() {
        "AI".to_string()
    } else {
        letters.to_uppercase()
    }
}

fn provider_available_for_completion(provider: &AIProviderConfig) -> bool {
    provider.enabled
        && !provider.models.is_empty()
        && (provider.rig_provider_type == "Ollama" || !provider.api_key.trim().is_empty())
}

fn resolve_model(provider: &AIProviderConfig, settings: &SystemSettings) -> String {
    let global_default_model = settings.global_config.default_llm_model.trim();
    if !global_default_model.is_empty()
        && provider
            .models
            .iter()
            .any(|model| model == global_default_model)
    {
        return global_default_model.to_string();
    }
    if provider.models.contains(&provider.default_model) {
        return provider.default_model.clone();
    }
    provider.models[0].clone()
}

async fn complete_with_best_provider(
    settings: &SystemSettings,
    preamble: &str,
    prompt: &str,
) -> Result<(String, ModelPolicy)> {
    let mut candidate_indexes = Vec::new();
    let default_provider_index = settings
        .providers
        .iter()
        .position(|provider| provider.id == settings.global_config.default_llm_provider);
    let global_default_model = settings.global_config.default_llm_model.trim();

    if !global_default_model.is_empty() {
        if let Some(default_index) = default_provider_index {
            if settings.providers[default_index]
                .models
                .iter()
                .any(|model| model == global_default_model)
            {
                candidate_indexes.push(default_index);
            }
        }

        for (index, provider) in settings.providers.iter().enumerate() {
            if Some(index) == default_provider_index {
                continue;
            }
            if provider
                .models
                .iter()
                .any(|model| model == global_default_model)
            {
                candidate_indexes.push(index);
            }
        }
    }

    if let Some(default_index) = default_provider_index {
        if !candidate_indexes.contains(&default_index) {
            candidate_indexes.push(default_index);
        }
    }

    for index in 0..settings.providers.len() {
        if !candidate_indexes.contains(&index) {
            candidate_indexes.push(index);
        }
    }

    let adapter = RigModelAdapter;
    let mut last_error: Option<anyhow::Error> = None;
    for index in candidate_indexes {
        let provider = &settings.providers[index];
        if !provider_available_for_completion(provider) {
            continue;
        }
        let policy = ModelPolicy {
            provider: provider.id.clone(),
            model: resolve_model(provider, settings),
            temperature: provider.temperature,
        };
        match adapter.complete(&policy, settings, preamble, prompt).await {
            Ok(Some(reply)) => return Ok((reply, policy)),
            Ok(None) => continue,
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(error) = last_error {
        Err(error.context("failed to generate agent profile with available providers"))
    } else {
        Err(anyhow!(
            "no available provider is configured for agent generation"
        ))
    }
}
