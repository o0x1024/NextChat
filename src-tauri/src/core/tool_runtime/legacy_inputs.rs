use regex::Regex;
use serde_json::{json, Value};

use super::ToolRuntime;
use crate::core::domain::ToolManifest;

impl ToolRuntime {
    pub(crate) fn normalize_compat_input(&self, tool: &ToolManifest, input: &str) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return input.to_string();
        }
        if serde_json::from_str::<Value>(trimmed).is_ok() {
            return input.to_string();
        }

        match tool.id.as_str() {
            "Write" => normalize_write_input(trimmed),
            "Read" => normalize_read_input(trimmed),
            "LS" => normalize_ls_input(trimmed),
            "Bash" => normalize_bash_input(trimmed),
            "ExitPlanMode" => Some(json!({ "plan": trimmed }).to_string()),
            "TodoWrite" => normalize_todo_write_input(trimmed),
            _ => None,
        }
        .unwrap_or_else(|| input.to_string())
    }
}

fn normalize_write_input(input: &str) -> Option<String> {
    let pattern =
        Regex::new(r"(?is)^\s*(?:write|save)\s+(.+?)\s+(?:and\s+)?(?:save\s+)?content:\s*(.+)\s*$")
            .ok()?;
    let captures = pattern.captures(input)?;
    let file_path = captures
        .get(1)?
        .as_str()
        .trim()
        .trim_matches(&['"', '\''][..]);
    let content = captures.get(2)?.as_str().trim();
    if file_path.is_empty() || content.is_empty() {
        return None;
    }
    Some(
        json!({
            "file_path": file_path,
            "content": content,
        })
        .to_string(),
    )
}

fn normalize_read_input(input: &str) -> Option<String> {
    let pattern = Regex::new(r"(?is)^\s*(?:read|查看|读取)\s+(?:file\s+)?(.+?)\s*$").ok()?;
    let captures = pattern.captures(input)?;
    let file_path = captures
        .get(1)?
        .as_str()
        .trim()
        .trim_matches(&['"', '\''][..]);
    if file_path.is_empty() {
        return None;
    }
    Some(json!({ "file_path": file_path }).to_string())
}

fn normalize_ls_input(input: &str) -> Option<String> {
    let pattern = Regex::new(r"(?is)^\s*(?:ls|list)\s+(.+?)\s*$").ok()?;
    let captures = pattern.captures(input)?;
    let path = captures
        .get(1)?
        .as_str()
        .trim()
        .trim_matches(&['"', '\''][..]);
    if path.is_empty() {
        return None;
    }
    Some(json!({ "path": path }).to_string())
}

fn normalize_bash_input(input: &str) -> Option<String> {
    let pattern =
        Regex::new(r"(?is)^\s*(?:bash|shell|run shell command|run command)\s*:?\s*(.+?)\s*$")
            .ok()?;
    let captures = pattern.captures(input)?;
    let command = captures.get(1)?.as_str().trim();
    if command.is_empty() {
        return None;
    }
    Some(json!({ "command": command }).to_string())
}

fn normalize_todo_write_input(input: &str) -> Option<String> {
    if !is_explicit_todo_request(input) {
        return None;
    }

    let body = strip_todo_prefix(input).unwrap_or(input).trim();
    let mut todos = body
        .lines()
        .map(str::trim)
        .map(strip_bullet_prefix)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if todos.is_empty() && !body.is_empty() {
        todos.push(body.to_string());
    }
    if todos.is_empty() {
        return None;
    }

    Some(
        json!({
            "todos": todos
                .into_iter()
                .enumerate()
                .map(|(index, content)| json!({
                    "id": format!("todo-{}", index + 1),
                    "content": content,
                    "status": "pending",
                }))
                .collect::<Vec<_>>(),
        })
        .to_string(),
    )
}

fn is_explicit_todo_request(input: &str) -> bool {
    let lowered = input.to_lowercase();
    ["todo", "task list", "待办"]
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

fn strip_todo_prefix(input: &str) -> Option<&str> {
    let lowered = input.to_lowercase();
    ["todo", "task list"]
        .iter()
        .find_map(|keyword| {
            lowered
                .find(keyword)
                .map(|index| &input[index + keyword.len()..])
        })
        .or_else(|| {
            input
                .find("待办")
                .map(|index| &input[index + "待办".len()..])
        })
        .map(|value| value.trim_start_matches([':', '：', '-', ' ']))
}

fn strip_bullet_prefix(line: &str) -> &str {
    line.trim_start_matches(|ch: char| {
        ch.is_ascii_digit() || matches!(ch, '.' | ')' | '-' | '*' | '[' | ']' | ' ')
    })
}
