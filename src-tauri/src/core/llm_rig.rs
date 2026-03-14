use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use rig::{
    agent::{HookAction, PromptHook, ToolCallHookAction},
    client::CompletionClient,
    completion::{CompletionModel, CompletionResponse, Message, Prompt},
    providers::{anthropic, gemini, openai},
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

mod tool_execution;

use self::tool_execution::{
    complete_task_with_tools_attempt, is_empty_json_response_error, log_llm_recovery,
    pending_user_question_error_from_tool_events, reset_summary_stream, ToolRequestFailure,
    ToolRequestMode,
};
use crate::core::{
    domain::{
        AIProviderConfig, ModelPolicy, ModelProviderAdapter, SummaryStreamSignal, SystemSettings,
        TaskExecutionContext, ToolCallProgressEvent, ToolCallProgressPhase, ToolHandler,
    },
    logging,
    permissions::APPROVAL_REQUIRED_PREFIX,
    rig_tools::{build_rig_tools, sanitize_rig_tool_name, RigToolCallLog, RigToolEvent},
    stream_text::merge_stream_text,
    tool_approval::{annotate_approval_request_error, PendingToolApprovalRequest},
};

#[derive(Debug, Clone)]
pub struct RigAgentResponse {
    pub summary: String,
    pub tool_events: Vec<RigToolEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct RigModelAdapter;

#[derive(Debug, Clone)]
struct LlmRequestLogContext {
    session_id: String,
    operation: String,
    provider_id: String,
    provider_type: String,
    model: String,
    base_url: String,
    temperature: f64,
    max_tokens: u64,
    max_turns: usize,
    agent_id: Option<String>,
    agent_name: Option<String>,
    task_card_id: Option<String>,
    work_group_id: Option<String>,
}

#[derive(Debug, Clone)]
struct LlmRequestHook {
    context: LlmRequestLogContext,
    call_log: RigToolCallLog,
    tool_call_stream: Option<UnboundedSender<ToolCallProgressEvent>>,
    summary_stream: Option<UnboundedSender<SummaryStreamSignal>>,
    summary_reset_epoch: Arc<AtomicUsize>,
    tool_identity_by_hook_name: HashMap<String, (String, String)>,
}

impl LlmRequestHook {
    fn log(&self, phase: &str, payload: serde_json::Value) {
        logging::llm_event(&self.context.operation, phase, payload);
    }

    fn common_payload(&self) -> serde_json::Value {
        json!({
            "sessionId": self.context.session_id,
            "providerId": self.context.provider_id,
            "providerType": self.context.provider_type,
            "model": self.context.model,
            "baseUrl": self.context.base_url,
            "temperature": self.context.temperature,
            "maxTokens": self.context.max_tokens,
            "maxTurns": self.context.max_turns,
            "agentId": self.context.agent_id,
            "agentName": self.context.agent_name,
            "taskCardId": self.context.task_card_id,
            "workGroupId": self.context.work_group_id,
        })
    }

    fn resolve_tool_identity(&self, hook_tool_name: &str) -> (String, String) {
        self.tool_identity_by_hook_name
            .get(hook_tool_name)
            .cloned()
            .unwrap_or_else(|| (hook_tool_name.to_string(), hook_tool_name.to_string()))
    }
}

impl<M> PromptHook<M> for LlmRequestHook
where
    M: CompletionModel,
{
    async fn on_completion_call(&self, prompt: &Message, history: &[Message]) -> HookAction {
        let _ = (prompt, history);
        HookAction::cont()
    }

    async fn on_completion_response(
        &self,
        _prompt: &Message,
        response: &CompletionResponse<M::Response>,
    ) -> HookAction {
        let _ = response;
        HookAction::cont()
    }

    async fn on_tool_call(
        &self,
        tool_name: &str,
        tool_call_id: Option<String>,
        internal_call_id: &str,
        args: &str,
    ) -> ToolCallHookAction {
        let call_id = tool_call_id.unwrap_or_else(|| internal_call_id.to_string());
        let (tool_id, display_name) = self.resolve_tool_identity(tool_name);
        self.call_log
            .record_call(&tool_id, &display_name, &call_id, args);
        if let Some(stream) = self.tool_call_stream.as_ref() {
            let _ = stream.send(ToolCallProgressEvent {
                tool_id: tool_id.clone(),
                tool_name: display_name.clone(),
                call_id: call_id.clone(),
                input: args.to_string(),
                output: String::new(),
                phase: ToolCallProgressPhase::Started,
            });
        }
        self.summary_reset_epoch.fetch_add(1, Ordering::SeqCst);
        if let Some(stream) = self.summary_stream.as_ref() {
            let _ = stream.send(SummaryStreamSignal::Reset);
        }
        self.log(
            "tool_call",
            merge_json(
                self.common_payload(),
                json!({
                    "toolName": tool_name,
                    "toolCallId": call_id,
                    "internalCallId": internal_call_id,
                    "args": logging::truncate(args, 8_000),
                }),
            ),
        );
        ToolCallHookAction::cont()
    }

    async fn on_tool_result(
        &self,
        tool_name: &str,
        tool_call_id: Option<String>,
        internal_call_id: &str,
        args: &str,
        result: &str,
    ) -> HookAction {
        let call_id = tool_call_id.unwrap_or_else(|| internal_call_id.to_string());
        let (tool_id, display_name) = self.resolve_tool_identity(tool_name);
        self.call_log
            .record_result(&tool_id, &display_name, &call_id, args, result);
        if let Some(stream) = self.tool_call_stream.as_ref() {
            let _ = stream.send(ToolCallProgressEvent {
                tool_id: tool_id.clone(),
                tool_name: display_name.clone(),
                call_id: call_id.clone(),
                input: args.to_string(),
                output: result.to_string(),
                phase: ToolCallProgressPhase::Completed,
            });
        }
        self.log(
            "tool_result",
            merge_json(
                self.common_payload(),
                json!({
                    "toolName": tool_name,
                    "toolCallId": call_id,
                    "internalCallId": internal_call_id,
                    "args": logging::truncate(args, 8_000),
                    "result": logging::truncate(result, 12_000),
                }),
            ),
        );
        HookAction::cont()
    }
}

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

fn provider_requires_api_key(provider_type: &str) -> bool {
    provider_type != "Ollama"
}

fn is_openai_compatible_provider_type(provider_type: &str) -> bool {
    matches!(
        provider_type,
        "OpenAI"
            | "DeepSeek"
            | "Groq"
            | "xAI"
            | "Moonshot"
            | "Hyperbolic"
            | "Mira"
            | "OpenRouter"
            | "Perplexity"
            | "Together"
            | "Mistral"
            | "HuggingFace"
            | "Galadriel"
            | "Azure"
            | "Ollama"
    )
}

fn parse_custom_headers(config: &AIProviderConfig) -> Result<Vec<(String, String)>> {
    let raw = config.custom_headers.trim();
    if raw.is_empty() || raw == "{}" {
        return Ok(vec![]);
    }
    let parsed: Value = serde_json::from_str(raw)
        .with_context(|| format!("Invalid customHeaders JSON for provider '{}'", config.id))?;
    let map = parsed.as_object().ok_or_else(|| {
        anyhow!(
            "customHeaders for provider '{}' must be a JSON object",
            config.id
        )
    })?;
    let mut headers = Vec::with_capacity(map.len());
    for (key, value) in map {
        match value {
            Value::String(text) => headers.push((key.clone(), text.clone())),
            _ => {
                return Err(anyhow!(
                    "customHeaders['{}'] for provider '{}' must be a string",
                    key,
                    config.id
                ));
            }
        }
    }
    Ok(headers)
}

/// Sets up proxy environment variables for reqwest if proxy_url is configured.
fn configure_proxy_env(proxy_url: &str) {
    let trimmed = proxy_url.trim();
    if trimmed.is_empty() {
        return;
    }

    // Set both HTTP and HTTPS proxy environment variables
    // reqwest will automatically use these for all requests
    if trimmed.starts_with("http://") {
        std::env::set_var("HTTP_PROXY", trimmed);
    } else if trimmed.starts_with("https://") {
        std::env::set_var("HTTPS_PROXY", trimmed);
    } else {
        // Assume HTTPS by default
        std::env::set_var("HTTPS_PROXY", trimmed);
    }
    // Also set the ALL_PROXY for generic proxy support
    std::env::set_var("ALL_PROXY", trimmed);
}

fn openai_compatible_client(
    config: &AIProviderConfig,
    proxy_url: &str,
) -> Result<openai::CompletionsClient> {
    configure_proxy_env(proxy_url);
    let mut builder = openai::Client::builder().api_key(config.api_key.trim());
    if let Some(base_url) = normalized_openai_compatible_base_url(&config.base_url) {
        builder = builder.base_url(&base_url);
    }
    builder
        .build()
        .map(|client| client.completions_api())
        .map_err(Into::into)
}

fn normalized_anthropic_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "https://api.anthropic.com".to_string()
    } else if let Some(prefix) = trimmed.strip_suffix("/v1") {
        prefix.trim_end_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}

fn anthropic_client(config: &AIProviderConfig, proxy_url: &str) -> Result<anthropic::Client> {
    configure_proxy_env(proxy_url);
    anthropic::Client::builder()
        .api_key(config.api_key.trim())
        .base_url(normalized_anthropic_base_url(&config.base_url))
        .build()
        .map_err(Into::into)
}

fn normalized_gemini_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "https://generativelanguage.googleapis.com".to_string()
    } else if let Some(prefix) = trimmed.strip_suffix("/v1beta") {
        prefix.trim_end_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}

fn gemini_api_base_url(base_url: &str) -> String {
    format!("{}/v1beta", normalized_gemini_base_url(base_url))
}

fn gemini_client(config: &AIProviderConfig, proxy_url: &str) -> Result<gemini::Client> {
    configure_proxy_env(proxy_url);
    gemini::Client::builder()
        .api_key(config.api_key.trim())
        .base_url(normalized_gemini_base_url(&config.base_url))
        .build()
        .map_err(Into::into)
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

fn push_unique_url(urls: &mut Vec<String>, candidate: String) {
    if !candidate.is_empty() && !urls.iter().any(|url| url == &candidate) {
        urls.push(candidate);
    }
}

fn openai_compatible_api_base_urls(base_url: &str) -> Vec<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return vec!["https://api.openai.com/v1".to_string()];
    }

    let mut candidates = Vec::new();
    let prefers_v1 = trimmed.contains("api.openai.com");
    if prefers_v1 && !trimmed.ends_with("/v1") {
        push_unique_url(&mut candidates, format!("{trimmed}/v1"));
    }

    push_unique_url(&mut candidates, trimmed.to_string());

    if !trimmed.ends_with("/v1") {
        push_unique_url(&mut candidates, format!("{trimmed}/v1"));
    } else if let Some(prefix) = trimmed.strip_suffix("/v1") {
        push_unique_url(&mut candidates, prefix.trim_end_matches('/').to_string());
    }

    candidates
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

fn fallback_models_from_config(config: &AIProviderConfig) -> Vec<String> {
    dedupe_models(
        config
            .models
            .iter()
            .cloned()
            .chain(std::iter::once(config.default_model.clone())),
    )
}

fn is_http_unauthorized_or_forbidden(message: &str) -> bool {
    message.contains("HTTP 401") || message.contains("HTTP 403")
}

fn max_turns_for_config(config: &AIProviderConfig) -> usize {
    config.max_dialog_rounds.max(1) as usize
}

fn new_llm_log_context(
    operation: &str,
    config: &AIProviderConfig,
    model: &str,
    temperature: f64,
    agent_id: Option<&str>,
    agent_name: Option<&str>,
    task_card_id: Option<&str>,
    work_group_id: Option<&str>,
) -> LlmRequestLogContext {
    LlmRequestLogContext {
        session_id: Uuid::new_v4().to_string(),
        operation: operation.to_string(),
        provider_id: config.id.clone(),
        provider_type: config.rig_provider_type.clone(),
        model: model.to_string(),
        base_url: config.base_url.clone(),
        temperature,
        max_tokens: config.max_tokens as u64,
        max_turns: max_turns_for_config(config),
        agent_id: agent_id.map(ToOwned::to_owned),
        agent_name: agent_name.map(ToOwned::to_owned),
        task_card_id: task_card_id.map(ToOwned::to_owned),
        work_group_id: work_group_id.map(ToOwned::to_owned),
    }
}

fn merge_json(left: serde_json::Value, right: serde_json::Value) -> serde_json::Value {
    match (left, right) {
        (serde_json::Value::Object(mut left), serde_json::Value::Object(right)) => {
            left.extend(right);
            serde_json::Value::Object(left)
        }
        (_, right) => right,
    }
}

fn log_llm_start(
    context: &LlmRequestLogContext,
    preamble: &str,
    prompt: &str,
    extra: serde_json::Value,
) {
    logging::llm_event(
        &context.operation,
        "request_start",
        merge_json(
            json!({
                "sessionId": context.session_id,
                "providerId": context.provider_id,
                "providerType": context.provider_type,
                "model": context.model,
                "baseUrl": context.base_url,
                "temperature": context.temperature,
                "maxTokens": context.max_tokens,
                "maxTurns": context.max_turns,
                "agentId": context.agent_id,
                "agentName": context.agent_name,
                "taskCardId": context.task_card_id,
                "workGroupId": context.work_group_id,
                "preamble": logging::truncate(preamble, 8_000),
                "prompt": logging::truncate(prompt, 16_000),
            }),
            extra,
        ),
    );
}

fn log_llm_skip(operation: &str, provider_id: &str, reason: &str) {
    logging::warn(operation, reason);
    logging::llm_event(
        operation,
        "skipped",
        json!({
            "providerId": provider_id,
            "reason": reason,
        }),
    );
}

fn log_llm_error<E>(context: &LlmRequestLogContext, error: &E)
where
    E: std::fmt::Display,
{
    let message = error.to_string();
    logging::error(&context.operation, &message);
    logging::llm_event(
        &context.operation,
        "request_error",
        json!({
            "sessionId": context.session_id,
            "providerId": context.provider_id,
            "providerType": context.provider_type,
            "model": context.model,
            "taskCardId": context.task_card_id,
            "agentId": context.agent_id,
            "error": message,
        }),
    );
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

fn parse_model_ids(payload: &Value) -> Vec<String> {
    if let Some(data) = payload.get("data").and_then(Value::as_array) {
        let models = data.iter().filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        });
        return dedupe_models(models);
    }

    if let Some(models) = payload.get("models").and_then(Value::as_array) {
        let models = models.iter().filter_map(|item| {
            item.as_str().map(ToOwned::to_owned).or_else(|| {
                item.get("id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("name").and_then(Value::as_str))
                    .map(ToOwned::to_owned)
            })
        });
        return dedupe_models(models);
    }

    if let Some(array) = payload.as_array() {
        let models = array.iter().filter_map(|item| {
            item.as_str().map(ToOwned::to_owned).or_else(|| {
                item.get("id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("name").and_then(Value::as_str))
                    .map(ToOwned::to_owned)
            })
        });
        return dedupe_models(models);
    }

    vec![]
}

fn apply_custom_headers_to_request(
    mut request: reqwest::RequestBuilder,
    config: &AIProviderConfig,
) -> Result<reqwest::RequestBuilder> {
    for (key, value) in parse_custom_headers(config)? {
        let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
            .with_context(|| format!("Invalid header name '{key}' in provider '{}'", config.id))?;
        let header_value = reqwest::header::HeaderValue::from_str(&value).with_context(|| {
            format!(
                "Invalid header value for '{key}' in provider '{}'",
                config.id
            )
        })?;
        request = request.header(header_name, header_value);
    }
    Ok(request)
}

async fn fetch_openai_compatible_models(
    config: &AIProviderConfig,
    proxy_url: &str,
) -> Result<Vec<String>> {
    configure_proxy_env(proxy_url);
    let client = Client::new();
    let mut last_error: Option<anyhow::Error> = None;

    for api_base_url in openai_compatible_api_base_urls(&config.base_url) {
        let endpoint = join_api_path(&api_base_url, "models");
        let mut request = client.get(&endpoint);
        if !config.api_key.trim().is_empty() {
            request = request.bearer_auth(config.api_key.trim());
        }
        request = apply_custom_headers_to_request(request, config)?;

        let response = request.send().await.with_context(|| {
            format!("Failed to request model list from OpenAI-compatible provider: {endpoint}")
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            continue;
        }

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

            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(anyhow!(message));
            }

            last_error = Some(anyhow!(message));
            continue;
        }

        let payload = response
            .json::<Value>()
            .await
            .context("Failed to parse model list response body")?;
        let models = parse_model_ids(&payload);
        if !models.is_empty() {
            return Ok(models);
        }

        last_error = Some(anyhow!(
            "No models were returned by endpoint '{}'",
            endpoint
        ));
    }

    let fallback_models = fallback_models_from_config(config);
    if !fallback_models.is_empty() {
        return Ok(fallback_models);
    }

    if let Some(error) = last_error {
        return Err(error);
    }

    Err(anyhow!("No models were returned by the provider"))
}

async fn fetch_anthropic_models(config: &AIProviderConfig, proxy_url: &str) -> Result<Vec<String>> {
    if config.api_key.trim().is_empty() {
        return Err(anyhow!("API key is required for Anthropic model refresh"));
    }

    configure_proxy_env(proxy_url);
    let client = Client::new();
    let base_url = normalized_api_base_url(&config.base_url, "https://api.anthropic.com");
    let api_base_url = if base_url.ends_with("/v1") {
        base_url
    } else {
        format!("{base_url}/v1")
    };

    let payload: AnthropicModelListResponse = parse_json_response(
        apply_custom_headers_to_request(
            client
                .get(join_api_path(&api_base_url, "models"))
                .header("x-api-key", config.api_key.trim())
                .header("anthropic-version", "2023-06-01"),
            config,
        )?
        .send()
        .await
        .context("Failed to request model list from Anthropic")?,
    )
    .await?;

    Ok(dedupe_models(
        payload.data.into_iter().map(|model| model.id),
    ))
}

async fn fetch_gemini_models(config: &AIProviderConfig, proxy_url: &str) -> Result<Vec<String>> {
    if config.api_key.trim().is_empty() {
        return Err(anyhow!("API key is required for Gemini model refresh"));
    }

    configure_proxy_env(proxy_url);
    let client = Client::new();
    let base_url = gemini_api_base_url(&config.base_url);
    let payload: GeminiModelListResponse = parse_json_response(
        apply_custom_headers_to_request(
            client
                .get(join_api_path(&base_url, "models"))
                .query(&[("key", config.api_key.trim())]),
            config,
        )?
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

async fn fetch_ollama_models(config: &AIProviderConfig, proxy_url: &str) -> Result<Vec<String>> {
    configure_proxy_env(proxy_url);
    let client = Client::new();
    let base_url = normalized_api_base_url(&config.base_url, "http://localhost:11434");
    let payload: Value = parse_json_response(
        apply_custom_headers_to_request(client.get(join_api_path(&base_url, "api/tags")), config)?
            .send()
            .await
            .context("Failed to request model list from Ollama")?,
    )
    .await?;
    Ok(parse_model_ids(&payload))
}

pub async fn refresh_models(config: &AIProviderConfig, proxy_url: &str) -> Result<Vec<String>> {
    logging::llm_event(
        "llm.refresh_models",
        "request_start",
        json!({
            "providerId": config.id,
            "providerType": config.rig_provider_type,
            "baseUrl": config.base_url,
        }),
    );

    let result = async {
        match config.rig_provider_type.as_str() {
            provider_type if is_openai_compatible_provider_type(provider_type) => {
                if provider_type == "Ollama" {
                    fetch_ollama_models(config, proxy_url).await
                } else {
                    fetch_openai_compatible_models(config, proxy_url).await
                }
            }
            "Anthropic" => match fetch_anthropic_models(config, proxy_url).await {
                Ok(models) => Ok(models),
                Err(error) => {
                    let message = error.to_string();
                    let fallback = fallback_models_from_config(config);
                    if is_http_unauthorized_or_forbidden(&message) && !fallback.is_empty() {
                        logging::warn(
                            "llm.refresh_models",
                            format!(
                                "Anthropic model listing returned auth error; using locally configured models for provider '{}'",
                                config.id
                            ),
                        );
                        Ok(fallback)
                    } else {
                        Err(error)
                    }
                }
            },
            "Gemini" => fetch_gemini_models(config, proxy_url).await,
            _ => Err(anyhow!(
                "Unsupported provider type for model refresh: {}",
                config.rig_provider_type
            )),
        }
    }
    .await;

    let result = match result {
        Ok(models) => models,
        Err(error) => {
            logging::error("llm.refresh_models", error.to_string());
            logging::llm_event(
                "llm.refresh_models",
                "request_error",
                json!({
                    "providerId": config.id,
                    "providerType": config.rig_provider_type,
                    "error": error.to_string(),
                }),
            );
            return Err(error);
        }
    };

    if result.is_empty() {
        let error = anyhow!("No models were returned by the provider");
        logging::error("llm.refresh_models", error.to_string());
        logging::llm_event(
            "llm.refresh_models",
            "request_error",
            json!({
                "providerId": config.id,
                "providerType": config.rig_provider_type,
                "error": error.to_string(),
            }),
        );
        return Err(error);
    }

    logging::llm_event(
        "llm.refresh_models",
        "request_success",
        json!({
            "providerId": config.id,
            "providerType": config.rig_provider_type,
            "modelCount": result.len(),
            "models": result.clone(),
        }),
    );

    Ok(result)
}

pub async fn complete_task_with_tools<TTool>(
    context: &TaskExecutionContext,
    tool_handler: std::sync::Arc<TTool>,
    preamble: &str,
    prompt: &str,
    summary_stream: Option<UnboundedSender<SummaryStreamSignal>>,
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
        Some(c)
            if c.enabled
                && (!provider_requires_api_key(&c.rig_provider_type)
                    || !c.api_key.trim().is_empty()) =>
        {
            c
        }
        _ => {
            log_llm_skip(
                "llm.complete_task_with_tools",
                provider_id,
                "Skipped tool-capable LLM request because provider is missing, disabled, or credentials are incomplete",
            );
            return Ok(None);
        }
    };

    let log_context = new_llm_log_context(
        "llm.complete_task_with_tools",
        config,
        &context.agent.model_policy.model,
        context.agent.model_policy.temperature,
        Some(&context.agent.id),
        Some(&context.agent.name),
        Some(&context.task_card.id),
        Some(&context.work_group.id),
    );
    log_llm_start(
        &log_context,
        preamble,
        prompt,
        json!({
            "toolCount": context.available_tools.len(),
            "skillCount": context.available_skills.len(),
        }),
    );

    let summary_reset_epoch = Arc::new(AtomicUsize::new(0));
    let tool_identity_by_hook_name = context
        .available_tools
        .iter()
        .chain(context.approved_tool.iter())
        .map(|tool| {
            (
                sanitize_rig_tool_name(&tool.id),
                (tool.id.clone(), tool.name.clone()),
            )
        })
        .collect::<HashMap<_, _>>();

    let result: std::result::Result<
        (String, Vec<RigToolEvent>),
        (anyhow::Error, Vec<RigToolEvent>),
    > = match complete_task_with_tools_attempt(
        context,
        tool_handler.clone(),
        config,
        &log_context,
        preamble,
        prompt,
        summary_stream.clone(),
        summary_reset_epoch.clone(),
        &tool_identity_by_hook_name,
        ToolRequestMode::Streaming,
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(ToolRequestFailure::RetryableEmptyJson {
            error: first_error, ..
        }) => {
            reset_summary_stream(summary_stream.as_ref());
            match complete_task_with_tools_attempt(
                context,
                tool_handler.clone(),
                config,
                &log_context,
                preamble,
                prompt,
                summary_stream.clone(),
                summary_reset_epoch.clone(),
                &tool_identity_by_hook_name,
                ToolRequestMode::Streaming,
            )
            .await
            {
                Ok(result) => Ok(result),
                Err(ToolRequestFailure::RetryableEmptyJson {
                    error: second_error,
                    ..
                }) => {
                    log_llm_recovery(
                        &log_context,
                        "request_recovery",
                        "fallback_to_non_streaming",
                        ToolRequestMode::Streaming,
                        &second_error,
                        0,
                        None,
                    );
                    reset_summary_stream(summary_stream.as_ref());
                    complete_task_with_tools_attempt(
                        context,
                        tool_handler,
                        config,
                        &log_context,
                        preamble,
                        prompt,
                        summary_stream,
                        summary_reset_epoch,
                        &tool_identity_by_hook_name,
                        ToolRequestMode::NonStreaming,
                    )
                    .await
                    .map_err(|failure| match failure {
                        ToolRequestFailure::RetryableEmptyJson { error, tool_events }
                        | ToolRequestFailure::Fatal { error, tool_events } => {
                            let final_error = if is_empty_json_response_error(&error.to_string()) {
                                anyhow!("{first_error}; fallback failed: {error}")
                            } else {
                                error
                            };
                            (final_error, tool_events)
                        }
                    })
                }
                Err(ToolRequestFailure::Fatal { error, tool_events }) => Err((error, tool_events)),
            }
        }
        Err(ToolRequestFailure::Fatal { error, tool_events }) => Err((error, tool_events)),
    };

    let (summary, tool_events) = match result {
        Ok(result) => result,
        Err((error, tool_events)) => {
            let error = if error.to_string().contains(APPROVAL_REQUIRED_PREFIX) {
                if let Some(event) = tool_events.last() {
                    annotate_approval_request_error(
                        error,
                        &PendingToolApprovalRequest {
                            tool_id: event.tool_id.clone(),
                            tool_name: event.tool_name.clone(),
                            input: event.input.clone(),
                        },
                    )
                } else {
                    error
                }
            } else {
                error
            };
            log_llm_error(&log_context, &error);
            return Err(error);
        }
    };

    if let Some(error) = pending_user_question_error_from_tool_events(&tool_events)? {
        log_llm_error(&log_context, &error);
        return Err(error);
    }

    logging::llm_event(
        "llm.complete_task_with_tools",
        "request_success",
        json!({
            "sessionId": log_context.session_id,
            "providerId": log_context.provider_id,
            "providerType": log_context.provider_type,
            "model": log_context.model,
            "taskCardId": log_context.task_card_id,
            "toolEventCount": tool_events.len(),
            "summary": logging::truncate(&summary, 12_000),
        }),
    );

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
            Some(c)
                if c.enabled
                    && (!provider_requires_api_key(&c.rig_provider_type)
                        || !c.api_key.trim().is_empty()) =>
            {
                c
            }
            _ => {
                log_llm_skip(
                    "llm.complete",
                    provider_id,
                    "Skipped LLM completion because provider is missing, disabled, or credentials are incomplete",
                );
                return Ok(None);
            }
        };

        let log_context = new_llm_log_context(
            "llm.complete",
            config,
            &policy.model,
            policy.temperature,
            None,
            None,
            None,
            None,
        );
        log_llm_start(&log_context, preamble, prompt, json!({}));

        let hook = LlmRequestHook {
            context: log_context.clone(),
            call_log: RigToolCallLog::new(),
            tool_call_stream: None,
            summary_stream: None,
            summary_reset_epoch: Arc::new(AtomicUsize::new(0)),
            tool_identity_by_hook_name: HashMap::new(),
        };

        let proxy_url = &settings.global_config.proxy_url;

        let result = async {
            match config.rig_provider_type.as_str() {
                provider_type if is_openai_compatible_provider_type(provider_type) => {
                    let client = openai_compatible_client(config, proxy_url)?;

                    let agent = client
                        .agent(&policy.model)
                        .preamble(preamble)
                        .default_max_turns(max_turns_for_config(config))
                        .temperature(policy.temperature)
                        .max_tokens(config.max_tokens as u64)
                        .build();

                    Ok::<_, anyhow::Error>(agent.prompt(prompt).with_hook(hook.clone()).await?)
                }
                "Anthropic" => {
                    let client = anthropic_client(config, proxy_url)?;
                    let agent = client
                        .agent(&policy.model)
                        .preamble(preamble)
                        .default_max_turns(max_turns_for_config(config))
                        .temperature(policy.temperature)
                        .max_tokens(config.max_tokens as u64)
                        .build();
                    Ok::<_, anyhow::Error>(agent.prompt(prompt).with_hook(hook.clone()).await?)
                }
                "Gemini" => {
                    let client = gemini_client(config, proxy_url)?;
                    let agent = client
                        .agent(&policy.model)
                        .preamble(preamble)
                        .default_max_turns(max_turns_for_config(config))
                        .temperature(policy.temperature)
                        .max_tokens(config.max_tokens as u64)
                        .build();
                    Ok::<_, anyhow::Error>(agent.prompt(prompt).with_hook(hook).await?)
                }
                _ => Err(anyhow!(
                    "Unsupported provider type '{}' for LLM completion",
                    config.rig_provider_type
                )),
            }
        }
        .await;

        let summary = match result {
            Ok(summary) => summary,
            Err(error) => {
                log_llm_error(&log_context, &error);
                return Err(error);
            }
        };

        logging::llm_event(
            "llm.complete",
            "request_success",
            json!({
                "sessionId": log_context.session_id,
                "providerId": log_context.provider_id,
                "providerType": log_context.provider_type,
                "model": log_context.model,
                "summary": logging::truncate(&summary, 12_000),
            }),
        );

        Ok(Some(summary))
    }
}

/// Lightweight LLM completion without tools — for classification, summarization, etc.
pub async fn simple_complete(
    config: &AIProviderConfig,
    proxy_url: &str,
    model: &str,
    preamble: &str,
    prompt: &str,
    max_tokens: u64,
) -> Result<String> {
    let log_context = new_llm_log_context(
        "llm.simple_complete",
        config,
        model,
        config.temperature,
        None,
        None,
        None,
        None,
    );
    log_llm_start(&log_context, preamble, prompt, json!({}));

    let hook = LlmRequestHook {
        context: log_context.clone(),
        call_log: RigToolCallLog::new(),
        tool_call_stream: None,
        summary_stream: None,
        summary_reset_epoch: Arc::new(AtomicUsize::new(0)),
        tool_identity_by_hook_name: HashMap::new(),
    };

    let result = async {
        match config.rig_provider_type.as_str() {
            provider_type if is_openai_compatible_provider_type(provider_type) => {
                let client = openai_compatible_client(config, proxy_url)?;
                let agent = client
                    .agent(model)
                    .preamble(preamble)
                    .temperature(config.temperature)
                    .max_tokens(max_tokens)
                    .build();
                Ok::<_, anyhow::Error>(agent.prompt(prompt).with_hook(hook.clone()).await?)
            }
            "Anthropic" => {
                let client = anthropic_client(config, proxy_url)?;
                let agent = client
                    .agent(model)
                    .preamble(preamble)
                    .temperature(config.temperature)
                    .max_tokens(max_tokens)
                    .build();
                Ok::<_, anyhow::Error>(agent.prompt(prompt).with_hook(hook.clone()).await?)
            }
            "Gemini" => {
                let client = gemini_client(config, proxy_url)?;
                let agent = client
                    .agent(model)
                    .preamble(preamble)
                    .temperature(config.temperature)
                    .max_tokens(max_tokens)
                    .build();
                Ok::<_, anyhow::Error>(agent.prompt(prompt).with_hook(hook).await?)
            }
            _ => Err(anyhow!(
                "Unsupported provider type '{}' for simple completion",
                config.rig_provider_type
            )),
        }
    }
    .await;

    match result {
        Ok(text) => {
            logging::llm_event(
                "llm.simple_complete",
                "request_success",
                json!({
                    "sessionId": log_context.session_id,
                    "providerId": log_context.provider_id,
                    "providerType": log_context.provider_type,
                    "model": log_context.model,
                    "response": logging::truncate(&text, 2_000),
                }),
            );
            Ok(text)
        }
        Err(error) => {
            log_llm_error(&log_context, &error);
            Err(error)
        }
    }
}

pub async fn test_connection(config: &AIProviderConfig, proxy_url: &str) -> Result<()> {
    let log_context = new_llm_log_context(
        "llm.test_connection",
        config,
        &config.default_model,
        config.temperature,
        None,
        None,
        None,
        None,
    );
    log_llm_start(&log_context, "", "ping", json!({}));

    let hook = LlmRequestHook {
        context: log_context.clone(),
        call_log: RigToolCallLog::new(),
        tool_call_stream: None,
        summary_stream: None,
        summary_reset_epoch: Arc::new(AtomicUsize::new(0)),
        tool_identity_by_hook_name: HashMap::new(),
    };

    let result = async {
        match config.rig_provider_type.as_str() {
            provider_type if is_openai_compatible_provider_type(provider_type) => {
                let client = openai_compatible_client(config, proxy_url)?;
                let agent = client.agent(&config.default_model).max_tokens(1).build();
                Ok::<_, anyhow::Error>(
                    agent
                        .prompt("ping")
                        .with_hook(hook.clone())
                        .await
                        .map(|_| ())?,
                )
            }
            "Anthropic" => {
                let client = anthropic_client(config, proxy_url)?;
                let agent = client.agent(&config.default_model).max_tokens(1).build();
                Ok::<_, anyhow::Error>(
                    agent
                        .prompt("ping")
                        .with_hook(hook.clone())
                        .await
                        .map(|_| ())?,
                )
            }
            "Gemini" => {
                let client = gemini_client(config, proxy_url)?;
                let agent = client.agent(&config.default_model).max_tokens(1).build();
                Ok::<_, anyhow::Error>(agent.prompt("ping").with_hook(hook).await.map(|_| ())?)
            }
            _ => Err(anyhow!("Unsupported provider type for connection test")),
        }
    }
    .await;

    match result {
        Ok(()) => {
            logging::llm_event(
                "llm.test_connection",
                "request_success",
                json!({
                    "sessionId": log_context.session_id,
                    "providerId": log_context.provider_id,
                    "providerType": log_context.provider_type,
                    "model": log_context.model,
                }),
            );
            Ok(())
        }
        Err(error) => {
            log_llm_error(&log_context, &error);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tool_execution::{
        classify_empty_json_recovery_action, is_empty_json_response_error,
        pending_user_question_error_from_tool_events, EmptyJsonRecoveryAction, ToolRequestMode,
    };
    use super::{
        gemini_api_base_url, is_http_unauthorized_or_forbidden, max_turns_for_config,
        normalized_anthropic_base_url, normalized_gemini_base_url,
        normalized_openai_compatible_base_url, openai_compatible_api_base_urls,
    };
    use crate::core::domain::AIProviderConfig;
    use crate::core::rig_tools::RigToolEvent;

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
    fn normalizes_anthropic_base_url_without_duplicate_v1() {
        assert_eq!(
            normalized_anthropic_base_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com"
        );
        assert_eq!(
            normalized_anthropic_base_url("https://ark.cn-beijing.volces.com/api/coding/"),
            "https://ark.cn-beijing.volces.com/api/coding"
        );
    }

    #[test]
    fn normalizes_gemini_base_url_and_api_base_url() {
        assert_eq!(
            normalized_gemini_base_url("https://generativelanguage.googleapis.com/v1beta"),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            normalized_gemini_base_url("https://generativelanguage.googleapis.com"),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            gemini_api_base_url("https://generativelanguage.googleapis.com"),
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(
            gemini_api_base_url("https://generativelanguage.googleapis.com/v1beta"),
            "https://generativelanguage.googleapis.com/v1beta"
        );
    }

    #[test]
    fn detects_auth_status_errors() {
        assert!(is_http_unauthorized_or_forbidden(
            "HTTP 401 Unauthorized: something"
        ));
        assert!(is_http_unauthorized_or_forbidden(
            "HTTP 403 Forbidden: something"
        ));
        assert!(!is_http_unauthorized_or_forbidden(
            "HTTP 404 Not Found: something"
        ));
    }

    #[test]
    fn builds_openai_compatible_model_listing_candidates() {
        assert_eq!(
            openai_compatible_api_base_urls("https://api.deepseek.com"),
            vec![
                "https://api.deepseek.com".to_string(),
                "https://api.deepseek.com/v1".to_string(),
            ]
        );
        assert_eq!(
            openai_compatible_api_base_urls("https://api.groq.com/openai/v1"),
            vec![
                "https://api.groq.com/openai/v1".to_string(),
                "https://api.groq.com/openai".to_string(),
            ]
        );
        assert_eq!(
            openai_compatible_api_base_urls("https://api.openai.com"),
            vec![
                "https://api.openai.com/v1".to_string(),
                "https://api.openai.com".to_string(),
            ]
        );
    }

    #[test]
    fn max_turns_falls_back_to_one_when_config_is_non_positive() {
        let mut config = AIProviderConfig::default();
        config.max_dialog_rounds = 420;
        assert_eq!(max_turns_for_config(&config), 420);

        config.max_dialog_rounds = 0;
        assert_eq!(max_turns_for_config(&config), 1);
    }

    #[test]
    fn detects_empty_json_response_errors() {
        assert!(is_empty_json_response_error(
            "CompletionError: JsonError: EOF while parsing an object at line 1 column 1"
        ));
        assert!(is_empty_json_response_error(
            "error decoding response body: EOF while parsing a value at line 1 column 0"
        ));
        assert!(!is_empty_json_response_error(
            "JsonError: trailing characters at line 1 column 2"
        ));
    }

    #[test]
    fn classifies_partial_stream_summary_as_recoverable() {
        assert_eq!(
            classify_empty_json_recovery_action(
                ToolRequestMode::Streaming,
                "CompletionError: JsonError: EOF while parsing an object at line 1 column 1",
                0,
                "partial summary",
            ),
            EmptyJsonRecoveryAction::AcceptPartialSummary
        );
    }

    #[test]
    fn classifies_empty_streaming_json_as_retryable() {
        assert_eq!(
            classify_empty_json_recovery_action(
                ToolRequestMode::Streaming,
                "CompletionError: JsonError: EOF while parsing an object at line 1 column 1",
                0,
                "",
            ),
            EmptyJsonRecoveryAction::RetryStreaming
        );
        assert_eq!(
            classify_empty_json_recovery_action(
                ToolRequestMode::NonStreaming,
                "CompletionError: JsonError: EOF while parsing an object at line 1 column 1",
                0,
                "",
            ),
            EmptyJsonRecoveryAction::Propagate
        );
    }

    #[test]
    fn detects_pending_user_question_wrapped_inside_tool_result() {
        let tool_events = vec![RigToolEvent {
            tool_id: "AskUserQuestion".into(),
            tool_name: "AskUserQuestion".into(),
            call_id: "call-1".into(),
            input: "{}".into(),
            output: "Toolset error: ToolCallError: ToolCallError: __nextchat_ask_user_question__:{\"question\":\"继续吗？\",\"options\":[\"是\"],\"context\":null,\"allowFreeForm\":true}".into(),
        }];

        let error = pending_user_question_error_from_tool_events(&tool_events)
            .expect("scan tool events")
            .expect("pending user question should be detected");

        assert!(error
            .to_string()
            .contains("__nextchat_ask_user_question__:"));
    }
}
