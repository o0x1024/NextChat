use anyhow::Result;
use async_trait::async_trait;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    providers::{anthropic, gemini, openai},
};

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
            let client = if config.base_url.is_empty()
                || config.base_url.contains("api.openai.com")
            {
                openai::Client::new(&config.api_key)?
            } else {
                openai::Client::builder()
                    .api_key(&config.api_key)
                    .base_url(&config.base_url)
                    .build()?
            };

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
        let config = settings
            .providers
            .iter()
            .find(|p| p.id == *provider_id);

        let config = match config {
            Some(c) if c.enabled && !c.api_key.is_empty() => c,
            _ => return Ok(None),
        };

        let summary = match config.rig_provider_type.as_str() {
            "OpenAI" | "DeepSeek" => {
                let client = if config.base_url.is_empty()
                    || config.base_url.contains("api.openai.com")
                {
                    openai::Client::new(&config.api_key)?
                } else {
                    openai::Client::builder()
                        .api_key(&config.api_key)
                        .base_url(&config.base_url)
                        .build()?
                };

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
            let client = if config.base_url.is_empty()
                || config.base_url.contains("api.openai.com")
            {
                openai::Client::new(&config.api_key)?
            } else {
                openai::Client::builder()
                    .api_key(&config.api_key)
                    .base_url(&config.base_url)
                    .build()?
            };
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
