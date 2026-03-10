use serde::Serialize;
use serde_json::Value;

use crate::core::domain::ToolManifest;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallPayload<'a> {
    tool_id: &'a str,
    tool_name: &'a str,
    call_id: &'a str,
    input: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolResultPayload<'a> {
    tool_id: &'a str,
    tool_name: &'a str,
    call_id: &'a str,
    input: &'a str,
    output: &'a str,
}

pub(super) fn structured_tool_call_content(
    tool: &ToolManifest,
    call_id: &str,
    input: &str,
) -> String {
    structured_tool_call_content_for_fields(&tool.id, &tool.name, call_id, input)
}

pub(super) fn structured_tool_call_content_for_fields(
    tool_id: &str,
    tool_name: &str,
    call_id: &str,
    input: &str,
) -> String {
    serde_json::to_string(&ToolCallPayload {
        tool_id,
        tool_name,
        call_id,
        input,
    })
    .unwrap_or_else(|_| {
        format!(
            "{{\"toolId\":\"{}\",\"toolName\":\"{}\",\"callId\":\"{}\",\"input\":{}}}",
            tool_id,
            tool_name,
            call_id,
            serde_json::to_string(input).unwrap_or_else(|_| "\"\"".to_string())
        )
    })
}

pub(super) fn structured_tool_result_content(
    tool: &ToolManifest,
    call_id: Option<&str>,
    input: &str,
    output: &str,
) -> String {
    structured_tool_result_content_for_fields(&tool.id, &tool.name, call_id, input, output)
}

pub(super) fn structured_tool_result_content_for_fields(
    tool_id: &str,
    tool_name: &str,
    call_id: Option<&str>,
    input: &str,
    output: &str,
) -> String {
    if already_structured_tool_result(output) {
        return output.to_string();
    }

    let fallback_call_id = call_id.unwrap_or(tool_id);
    serde_json::to_string(&ToolResultPayload {
        tool_id,
        tool_name,
        call_id: fallback_call_id,
        input,
        output,
    })
    .unwrap_or_else(|_| {
        format!(
            "{{\"toolId\":\"{}\",\"toolName\":\"{}\",\"callId\":\"{}\",\"input\":{},\"output\":{}}}",
            tool_id,
            tool_name,
            fallback_call_id,
            serde_json::to_string(input).unwrap_or_else(|_| "\"\"".to_string()),
            serde_json::to_string(output).unwrap_or_else(|_| "\"\"".to_string())
        )
    })
}

fn already_structured_tool_result(output: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return false;
    };

    let Some(object) = value.as_object() else {
        return false;
    };

    object.contains_key("toolCalls")
        || object.contains_key("tool_calls")
        || (object.contains_key("toolId") && object.contains_key("output"))
        || (object.contains_key("tool_id") && object.contains_key("output"))
}
