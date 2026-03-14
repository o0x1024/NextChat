use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Error, Result};
use futures::StreamExt;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::streaming::StreamedAssistantContent;
use rig::streaming::StreamingPrompt;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    anthropic_client, build_rig_tools, gemini_client, is_openai_compatible_provider_type, logging,
    max_turns_for_config, merge_stream_text, openai_compatible_client, LlmRequestHook,
    LlmRequestLogContext,
};
use crate::core::{
    buildin_tools::ask_user_question::parse_signal_from_error,
    domain::{SummaryStreamSignal, TaskExecutionContext, ToolCallProgressEvent, ToolHandler},
    rig_tools::{RigToolCallLog, RigToolEvent},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolRequestMode {
    Streaming,
    NonStreaming,
}

impl ToolRequestMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::NonStreaming => "non_streaming",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmptyJsonRecoveryAction {
    AcceptPartialSummary,
    RetryStreaming,
    Propagate,
}

#[derive(Debug)]
pub(super) enum ToolRequestFailure {
    RetryableEmptyJson {
        error: Error,
        tool_events: Vec<RigToolEvent>,
    },
    Fatal {
        error: Error,
        tool_events: Vec<RigToolEvent>,
    },
}

pub(super) fn log_llm_recovery<E>(
    context: &LlmRequestLogContext,
    phase: &str,
    strategy: &str,
    mode: ToolRequestMode,
    error: &E,
    tool_event_count: usize,
    partial_summary: Option<&str>,
) where
    E: std::fmt::Display,
{
    logging::llm_event(
        &context.operation,
        phase,
        serde_json::json!({
            "sessionId": context.session_id,
            "providerId": context.provider_id,
            "providerType": context.provider_type,
            "model": context.model,
            "taskCardId": context.task_card_id,
            "agentId": context.agent_id,
            "strategy": strategy,
            "mode": mode.as_str(),
            "error": error.to_string(),
            "toolEventCount": tool_event_count,
            "partialSummary": partial_summary
                .map(|summary| logging::truncate(summary, 4_000)),
        }),
    );
}

pub(super) fn is_empty_json_response_error(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    (normalized.contains("eof while parsing") || normalized.contains("expected value"))
        && (normalized.contains("line 1 column 1") || normalized.contains("line 1 column 0"))
}

pub(super) fn classify_empty_json_recovery_action(
    mode: ToolRequestMode,
    error_message: &str,
    tool_event_count: usize,
    partial_summary: &str,
) -> EmptyJsonRecoveryAction {
    if !is_empty_json_response_error(error_message) {
        return EmptyJsonRecoveryAction::Propagate;
    }

    if !partial_summary.trim().is_empty() {
        return EmptyJsonRecoveryAction::AcceptPartialSummary;
    }

    if matches!(mode, ToolRequestMode::Streaming) && tool_event_count == 0 {
        return EmptyJsonRecoveryAction::RetryStreaming;
    }

    EmptyJsonRecoveryAction::Propagate
}

fn build_llm_request_hook(
    context: &LlmRequestLogContext,
    tool_call_stream: Option<UnboundedSender<ToolCallProgressEvent>>,
    summary_stream: Option<UnboundedSender<SummaryStreamSignal>>,
    summary_reset_epoch: Arc<AtomicUsize>,
    tool_identity_by_hook_name: &HashMap<String, (String, String)>,
) -> LlmRequestHook {
    LlmRequestHook {
        context: context.clone(),
        call_log: RigToolCallLog::new(),
        tool_call_stream,
        summary_stream,
        summary_reset_epoch,
        tool_identity_by_hook_name: tool_identity_by_hook_name.clone(),
    }
}

pub(super) fn pending_user_question_error_from_tool_events(
    tool_events: &[RigToolEvent],
) -> Result<Option<Error>> {
    for event in tool_events.iter().rev() {
        if parse_signal_from_error(&event.output)?.is_some() {
            return Ok(Some(anyhow!(event.output.clone())));
        }
    }
    Ok(None)
}

pub(super) fn reset_summary_stream(summary_stream: Option<&UnboundedSender<SummaryStreamSignal>>) {
    if let Some(stream) = summary_stream {
        let _ = stream.send(SummaryStreamSignal::Reset);
    }
}

pub(super) async fn complete_task_with_tools_attempt<TTool>(
    context: &TaskExecutionContext,
    tool_handler: Arc<TTool>,
    config: &crate::core::domain::AIProviderConfig,
    log_context: &LlmRequestLogContext,
    preamble: &str,
    prompt: &str,
    summary_stream: Option<UnboundedSender<SummaryStreamSignal>>,
    summary_reset_epoch: Arc<AtomicUsize>,
    tool_identity_by_hook_name: &HashMap<String, (String, String)>,
    mode: ToolRequestMode,
) -> std::result::Result<(String, Vec<RigToolEvent>), ToolRequestFailure>
where
    TTool: ToolHandler + 'static,
{
    let hook = build_llm_request_hook(
        log_context,
        context.tool_call_stream.clone(),
        summary_stream.clone(),
        summary_reset_epoch.clone(),
        tool_identity_by_hook_name,
    );
    let call_log = hook.call_log.clone();
    let handle_error = |error: Error, partial_summary: &str| {
        let tool_events = call_log.snapshot();
        match classify_empty_json_recovery_action(
            mode,
            &error.to_string(),
            tool_events.len(),
            partial_summary,
        ) {
            EmptyJsonRecoveryAction::AcceptPartialSummary => {
                log_llm_recovery(
                    log_context,
                    "request_recovered",
                    "accept_partial_stream_summary",
                    mode,
                    &error,
                    tool_events.len(),
                    Some(partial_summary),
                );
                Ok((partial_summary.to_string(), tool_events))
            }
            EmptyJsonRecoveryAction::RetryStreaming => {
                log_llm_recovery(
                    log_context,
                    "request_retry",
                    "retry_after_empty_json",
                    mode,
                    &error,
                    tool_events.len(),
                    None,
                );
                Err(ToolRequestFailure::RetryableEmptyJson { error, tool_events })
            }
            EmptyJsonRecoveryAction::Propagate => {
                Err(ToolRequestFailure::Fatal { error, tool_events })
            }
        }
    };

    let proxy_url = &context.settings.global_config.proxy_url;

    match config.rig_provider_type.as_str() {
        provider_type if is_openai_compatible_provider_type(provider_type) => {
            let agent = openai_compatible_client(config, proxy_url)
                .map_err(|error| ToolRequestFailure::Fatal {
                    error,
                    tool_events: Vec::new(),
                })?
                .agent(&context.agent.model_policy.model)
                .preamble(preamble)
                .default_max_turns(max_turns_for_config(config))
                .temperature(context.agent.model_policy.temperature)
                .max_tokens(config.max_tokens as u64)
                .tools(build_rig_tools(context, tool_handler))
                .build();

            match mode {
                ToolRequestMode::Streaming => {
                    let mut stream = agent.stream_prompt(prompt).with_hook(hook.clone()).await;
                    let mut streamed_summary = String::new();
                    let mut final_summary: Option<String> = None;
                    let mut seen_reset_epoch = 0usize;

                    while let Some(chunk) = stream.next().await {
                        let reset_epoch = summary_reset_epoch.load(Ordering::SeqCst);
                        if reset_epoch != seen_reset_epoch {
                            streamed_summary.clear();
                            seen_reset_epoch = reset_epoch;
                        }
                        match chunk {
                            Ok(rig::agent::MultiTurnStreamItem::StreamAssistantItem(
                                StreamedAssistantContent::Text(text),
                            )) => {
                                if let Some(delta) =
                                    merge_stream_text(&mut streamed_summary, &text.text)
                                {
                                    if let Some(stream) = summary_stream.as_ref() {
                                        let _ = stream.send(SummaryStreamSignal::Delta(delta));
                                    }
                                }
                            }
                            Ok(rig::agent::MultiTurnStreamItem::FinalResponse(response)) => {
                                final_summary = Some(response.response().to_string());
                            }
                            Ok(_) => {}
                            Err(error) => return handle_error(error.into(), &streamed_summary),
                        }
                    }

                    Ok((
                        final_summary.unwrap_or(streamed_summary),
                        call_log.snapshot(),
                    ))
                }
                ToolRequestMode::NonStreaming => agent
                    .prompt(prompt)
                    .with_hook(hook)
                    .await
                    .map(|summary| (summary, call_log.snapshot()))
                    .map_err(Into::into)
                    .map_err(|error| ToolRequestFailure::Fatal {
                        error,
                        tool_events: call_log.snapshot(),
                    }),
            }
        }
        "Anthropic" => {
            let agent = anthropic_client(config, proxy_url)
                .map_err(|error| ToolRequestFailure::Fatal {
                    error,
                    tool_events: Vec::new(),
                })?
                .agent(&context.agent.model_policy.model)
                .preamble(preamble)
                .default_max_turns(max_turns_for_config(config))
                .temperature(context.agent.model_policy.temperature)
                .max_tokens(config.max_tokens as u64)
                .tools(build_rig_tools(context, tool_handler))
                .build();

            match mode {
                ToolRequestMode::Streaming => {
                    let mut stream = agent.stream_prompt(prompt).with_hook(hook.clone()).await;
                    let mut streamed_summary = String::new();
                    let mut final_summary: Option<String> = None;
                    let mut seen_reset_epoch = 0usize;

                    while let Some(chunk) = stream.next().await {
                        let reset_epoch = summary_reset_epoch.load(Ordering::SeqCst);
                        if reset_epoch != seen_reset_epoch {
                            streamed_summary.clear();
                            seen_reset_epoch = reset_epoch;
                        }
                        match chunk {
                            Ok(rig::agent::MultiTurnStreamItem::StreamAssistantItem(
                                StreamedAssistantContent::Text(text),
                            )) => {
                                if let Some(delta) =
                                    merge_stream_text(&mut streamed_summary, &text.text)
                                {
                                    if let Some(stream) = summary_stream.as_ref() {
                                        let _ = stream.send(SummaryStreamSignal::Delta(delta));
                                    }
                                }
                            }
                            Ok(rig::agent::MultiTurnStreamItem::FinalResponse(response)) => {
                                final_summary = Some(response.response().to_string());
                            }
                            Ok(_) => {}
                            Err(error) => return handle_error(error.into(), &streamed_summary),
                        }
                    }

                    Ok((
                        final_summary.unwrap_or(streamed_summary),
                        call_log.snapshot(),
                    ))
                }
                ToolRequestMode::NonStreaming => agent
                    .prompt(prompt)
                    .with_hook(hook)
                    .await
                    .map(|summary| (summary, call_log.snapshot()))
                    .map_err(Into::into)
                    .map_err(|error| ToolRequestFailure::Fatal {
                        error,
                        tool_events: call_log.snapshot(),
                    }),
            }
        }
        "Gemini" => {
            let agent = gemini_client(config, proxy_url)
                .map_err(|error| ToolRequestFailure::Fatal {
                    error,
                    tool_events: Vec::new(),
                })?
                .agent(&context.agent.model_policy.model)
                .preamble(preamble)
                .default_max_turns(max_turns_for_config(config))
                .temperature(context.agent.model_policy.temperature)
                .max_tokens(config.max_tokens as u64)
                .tools(build_rig_tools(context, tool_handler))
                .build();

            match mode {
                ToolRequestMode::Streaming => {
                    let mut stream = agent.stream_prompt(prompt).with_hook(hook.clone()).await;
                    let mut streamed_summary = String::new();
                    let mut final_summary: Option<String> = None;
                    let mut seen_reset_epoch = 0usize;

                    while let Some(chunk) = stream.next().await {
                        let reset_epoch = summary_reset_epoch.load(Ordering::SeqCst);
                        if reset_epoch != seen_reset_epoch {
                            streamed_summary.clear();
                            seen_reset_epoch = reset_epoch;
                        }
                        match chunk {
                            Ok(rig::agent::MultiTurnStreamItem::StreamAssistantItem(
                                StreamedAssistantContent::Text(text),
                            )) => {
                                if let Some(delta) =
                                    merge_stream_text(&mut streamed_summary, &text.text)
                                {
                                    if let Some(stream) = summary_stream.as_ref() {
                                        let _ = stream.send(SummaryStreamSignal::Delta(delta));
                                    }
                                }
                            }
                            Ok(rig::agent::MultiTurnStreamItem::FinalResponse(response)) => {
                                final_summary = Some(response.response().to_string());
                            }
                            Ok(_) => {}
                            Err(error) => return handle_error(error.into(), &streamed_summary),
                        }
                    }

                    Ok((
                        final_summary.unwrap_or(streamed_summary),
                        call_log.snapshot(),
                    ))
                }
                ToolRequestMode::NonStreaming => agent
                    .prompt(prompt)
                    .with_hook(hook)
                    .await
                    .map(|summary| (summary, call_log.snapshot()))
                    .map_err(Into::into)
                    .map_err(|error| ToolRequestFailure::Fatal {
                        error,
                        tool_events: call_log.snapshot(),
                    }),
            }
        }
        _ => Err(ToolRequestFailure::Fatal {
            error: anyhow!(
                "Unsupported provider type '{}' for tool-capable LLM request",
                config.rig_provider_type
            ),
            tool_events: Vec::new(),
        }),
    }
}
