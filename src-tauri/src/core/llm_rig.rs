use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    providers::{anthropic, gemini, openai},
};
use serde::Deserialize;

use crate::core::{
    domain::{
        AIProviderConfig, ModelPolicy, ModelProviderAdapter, SystemSettings, TaskExecutionContext,
        ToolHandler,
    },
    rig_tools::{build_rig_tools, RigToolCallLog, RigToolEvent},
};

#[derive(Debug, Clone)]
pub struct RigAgentResponse {
    pub summary: String,
    pub tool_events: Vec<RigToolEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct RigModelAdapter;

fn normalized_openai_compatible_base_url(base_url: &str) -> Option<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains("api.openai.com") {
        None
    } else if let Some(prefix) = trimmed.strip_suffix("/v1") {
        Some(prefix.to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn openai_compatible_client(config: &AIProviderConfig) -> Result<openai::CompletionsClient> {
    if let Some(base_url) = normalized_openai_compatible_base_url(&config.base_url) {
        openai::Client::builder()
            .api_key(&config.api_key)
            .base_url(&base_url)
            .build()
            .map(|client| client.completions_api())
            .map_err(Into::into)
    } else {
        openai::Client::new(&config.api_key)
            .map(|client| client.completions_api())
            .map_err(Into::into)
    }
}

fn normalized_api_base_url(base_url: &str, default_base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        default_base_url.to_string()
    } else {
        trimmed.to_string()
    }
}

fn join_api_path(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn openai_compatible_api_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIModelListResponse {
    #[serde(default)]
    data: Vec<OpenAIModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModel {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicModelListResponse {
    #[serde(default)]
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GeminiModelListResponse {
    #[serde(default)]
    models: Vec<GeminiModel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModel {
    name: String,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

fn dedupe_models<I>(models: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut unique = Vec::new();
    for model in models {
        let model = model.trim().to_string();
        if !model.is_empty() && !unique.contains(&model) {
            unique.push(model);
        }
    }
    unique
}

async fn parse_json_response<T>(response: reqwest::Response) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let detail = body.trim();
        let message = if detail.is_empty() {
            format!("HTTP {}", status)
        } else {
            format!(
                "HTTP {}: {}",
                status,
                detail.chars().take(200).collect::<String>()
            )
        };
        return Err(anyhow!(message));
    }

    response.json::<T>().await.map_err(Into::into)
}

async fn fetch_openai_compatible_models(config: &AIProviderConfig) -> Result<Vec<String>> {
    let client = Client::new();
    let mut request = client.get(join_api_path(
        &openai_compatible_api_base_url(&config.base_url),
        "models",
    ));
    if !config.api_key.trim().is_empty() {
        request = request.bearer_auth(config.api_key.trim());
    }

    let payload: OpenAIModelListResponse = parse_json_response(
        request
            .send()
            .await
            .context("Failed to request model list from OpenAI-compatible provider")?,
    )
    .await?;

    Ok(dedupe_models(
        payload.data.into_iter().map(|model| model.id),
    ))
}

async fn fetch_anthropic_models(config: &AIProviderConfig) -> Result<Vec<String>> {
    if config.api_key.trim().is_empty() {
        return Err(anyhow!("API key is required for Anthropic model refresh"));
    }

    let client = Client::new();
    let base_url = normalized_api_base_url(&config.base_url, "https://api.anthropic.com");
    let api_base_url = if base_url.ends_with("/v1") {
        base_url
    } else {
        format!("{base_url}/v1")
    };

    let payload: AnthropicModelListResponse = parse_json_response(
        client
            .get(join_api_path(&api_base_url, "models"))
            .header("x-api-key", config.api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .context("Failed to request model list from Anthropic")?,
    )
    .await?;

    Ok(dedupe_models(
        payload.data.into_iter().map(|model| model.id),
    ))
}

async fn fetch_gemini_models(config: &AIProviderConfig) -> Result<Vec<String>> {
    if config.api_key.trim().is_empty() {
        return Err(anyhow!("API key is required for Gemini model refresh"));
    }

    let client = Client::new();
    let base_url = normalized_api_base_url(
        &config.base_url,
        "https://generativelanguage.googleapis.com/v1beta",
    );
    let payload: GeminiModelListResponse = parse_json_response(
        client
            .get(join_api_path(&base_url, "models"))
            .query(&[("key", config.api_key.trim())])
            .send()
            .await
            .context("Failed to request model list from Gemini")?,
    )
    .await?;

    let models = payload.models.into_iter().filter_map(|model| {
        let supports_generation = model.supported_generation_methods.is_empty()
            || model
                .supported_generation_methods
                .iter()
                .any(|method| method == "generateContent" || method == "streamGenerateContent");
        supports_generation.then(|| {
            model
                .name
                .strip_prefix("models/")
                .unwrap_or(&model.name)
                .to_string()
        })
    });

    Ok(dedupe_models(models))
}

pub async fn refresh_models(config: &AIProviderConfig) -> Result<Vec<String>> {
    let models = match config.rig_provider_type.as_str() {
        "OpenAI" | "DeepSeek" | "Groq" => fetch_openai_compatible_models(config).await?,
        "Anthropic" => fetch_anthropic_models(config).await?,
        "Gemini" => fetch_gemini_models(config).await?,
        _ => {
            return Err(anyhow!(
                "Unsupported provider type for model refresh: {}",
                config.rig_provider_type
            ));
        }
    };

    if models.is_empty() {
        return Err(anyhow!("No models were returned by the provider"));
    }

    Ok(models)
}

pub async fn complete_task_with_tools<TTool>(
    context: &TaskExecutionContext,
    tool_handler: std::sync::Arc<TTool>,
    preamble: &str,
    prompt: &str,
) -> Result<Option<RigAgentResponse>>
where
    TTool: ToolHandler + 'static,
{
    let provider_id = &context.agent.model_policy.provider;
    let config = context
        .settings
        .providers
        .iter()
        .find(|p| p.id == *provider_id);

    let config = match config {
        Some(c) if c.enabled && !c.api_key.is_empty() => c,
        _ => return Ok(None),
    };

    let call_log = RigToolCallLog::new();
    let tools = build_rig_tools(context, tool_handler, call_log.clone());

    let (summary, tool_events) = match config.rig_provider_type.as_str() {
        "OpenAI" | "DeepSeek" => {
            let client = openai_compatible_client(config)?;

            let agent = client
                .agent(&context.agent.model_policy.model)
                .preamble(preamble)
                .temperature(context.agent.model_policy.temperature)
                .max_tokens(config.max_tokens as u64)
                .tools(tools)
                .build();

            (agent.prompt(prompt).await?, call_log.snapshot())
        }
        "Anthropic" => {
            let client = anthropic::Client::new(&config.api_key)?;
            let agent = client
                .agent(&context.agent.model_policy.model)
                .preamble(preamble)
                .temperature(context.agent.model_policy.temperature)
                .max_tokens(config.max_tokens as u64)
                .tools(tools)
                .build();
            (agent.prompt(prompt).await?, call_log.snapshot())
        }
        "Gemini" => {
            let client = gemini::Client::new(&config.api_key)?;
            let agent = client
                .agent(&context.agent.model_policy.model)
                .preamble(preamble)
                .temperature(context.agent.model_policy.temperature)
                .max_tokens(config.max_tokens as u64)
                .tools(tools)
                .build();
            (agent.prompt(prompt).await?, call_log.snapshot())
        }
        _ => return Ok(None),
    };

    Ok(Some(RigAgentResponse {
        summary,
        tool_events,
    }))
}

#[async_trait]
impl ModelProviderAdapter for RigModelAdapter {
    async fn complete(
        &self,
        policy: &ModelPolicy,
        settings: &SystemSettings,
        preamble: &str,
        prompt: &str,
    ) -> Result<Option<String>> {
        let provider_id = &policy.provider;
        let config = settings.providers.iter().find(|p| p.id == *provider_id);

        let config = match config {
            Some(c) if c.enabled && !c.api_key.is_empty() => c,
            _ => return Ok(None),
        };

        let summary = match config.rig_provider_type.as_str() {
            "OpenAI" | "DeepSeek" => {
                let client = openai_compatible_client(config)?;

                let agent = client
                    .agent(&policy.model)
                    .preamble(preamble)
                    .temperature(policy.temperature)
                    .max_tokens(config.max_tokens as u64)
                    .build();

                agent.prompt(prompt).await?
            }
            "Anthropic" => {
                let client = anthropic::Client::new(&config.api_key)?;
                let agent = client
                    .agent(&policy.model)
                    .preamble(preamble)
                    .temperature(policy.temperature)
                    .max_tokens(config.max_tokens as u64)
                    .build();
                agent.prompt(prompt).await?
            }
            "Gemini" => {
                let client = gemini::Client::new(&config.api_key)?;
                let agent = client
                    .agent(&policy.model)
                    .preamble(preamble)
                    .temperature(policy.temperature)
                    .max_tokens(config.max_tokens as u64)
                    .build();
                agent.prompt(prompt).await?
            }
            _ => return Ok(None),
        };

        Ok(Some(summary))
    }
}

pub async fn test_connection(config: &AIProviderConfig) -> Result<()> {
    match config.rig_provider_type.as_str() {
        "OpenAI" | "DeepSeek" => {
            let client = openai_compatible_client(config)?;
            let agent = client.agent(&config.default_model).max_tokens(1).build();
            agent.prompt("ping").await?;
            Ok(())
        }
        "Anthropic" => {
            let client = anthropic::Client::new(&config.api_key)?;
            let agent = client.agent(&config.default_model).max_tokens(1).build();
            agent.prompt("ping").await?;
            Ok(())
        }
        "Gemini" => {
            let client = gemini::Client::new(&config.api_key)?;
            let agent = client.agent(&config.default_model).max_tokens(1).build();
            agent.prompt("ping").await?;
            Ok(())
        }
        _ => Err(anyhow::anyhow!(
            "Unsupported provider type for connection test"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{normalized_openai_compatible_base_url, openai_compatible_api_base_url};

    #[test]
    fn strips_trailing_v1_for_openai_compatible_providers() {
        assert_eq!(
            normalized_openai_compatible_base_url("https://api.deepseek.com/v1"),
            Some("https://api.deepseek.com".into())
        );
        assert_eq!(
            normalized_openai_compatible_base_url("https://api.groq.com/openai/v1/"),
            Some("https://api.groq.com/openai".into())
        );
    }

    #[test]
    fn keeps_openai_default_on_official_host() {
        assert_eq!(
            normalized_openai_compatible_base_url("https://api.openai.com/v1"),
            None
        );
    }

    #[test]
    fn keeps_v1_suffix_for_model_listing_endpoints() {
        assert_eq!(
            openai_compatible_api_base_url("https://api.deepseek.com"),
            "https://api.deepseek.com/v1"
        );
        assert_eq!(
            openai_compatible_api_base_url("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1"
        );
    }
}
