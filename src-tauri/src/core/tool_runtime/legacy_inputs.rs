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

        // LLMs sometimes emit literal newline/tab bytes inside JSON string values instead of
        // the required escape sequences (\\n, \\t).  Try to repair that before falling back to
        // pattern-based normalisation.
        if let Some(repaired) = sanitize_json_string_literals(trimmed) {
            return repaired;
        }

        match tool.id.as_str() {
            "Write" => normalize_write_input(trimmed),
            "Read" => normalize_read_input(trimmed),
            "LS" => normalize_ls_input(trimmed),
            "Bash" => normalize_bash_input(trimmed),
            _ => None,
        }
        .unwrap_or_else(|| input.to_string())
    }
}

/// Repairs JSON that contains literal control characters (newline, carriage-return, tab)
/// inside string values.  Returns `Some(repaired)` only when the result is valid JSON.
fn sanitize_json_string_literals(input: &str) -> Option<String> {
    let trimmed = input.trim();
    // Only attempt repair for JSON objects / arrays.
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }

    let mut result = String::with_capacity(trimmed.len() + 64);
    let mut in_string = false;
    let mut escaped = false;
    let mut modified = false;

    for ch in trimmed.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            result.push(ch);
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            result.push(ch);
            continue;
        }
        if in_string {
            match ch {
                '\n' => {
                    result.push_str("\\n");
                    modified = true;
                }
                '\r' => {
                    result.push_str("\\r");
                    modified = true;
                }
                '\t' => {
                    result.push_str("\\t");
                    modified = true;
                }
                _ => result.push(ch),
            }
        } else {
            result.push(ch);
        }
    }

    if modified && serde_json::from_str::<Value>(&result).is_ok() {
        Some(result)
    } else {
        None
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

#[cfg(test)]
mod tests {
    use super::sanitize_json_string_literals;
    use serde_json::Value;

    /// Helper: round-trips the repaired JSON through serde to confirm it is valid.
    fn parse_repaired(input: &str) -> Value {
        let repaired = sanitize_json_string_literals(input)
            .expect("sanitize_json_string_literals should return Some");
        serde_json::from_str(&repaired).expect("repaired JSON must be valid")
    }

    #[test]
    fn repairs_literal_newline_in_content_field() {
        // Simulates the LLM emitting a real newline byte inside the JSON string value.
        let raw = "{\"file_path\":\"/tmp/a.js\",\"content\":\"line1\nline2\"}";
        let value = parse_repaired(raw);
        assert_eq!(
            value["content"].as_str().unwrap(),
            "line1\nline2",
            "content should decode back to two lines"
        );
    }

    #[test]
    fn repairs_literal_cr_and_tab() {
        let raw = "{\"key\":\"col1\tcol2\r\ncol3\"}";
        let value = parse_repaired(raw);
        let content = value["key"].as_str().unwrap();
        assert!(content.contains('\t'));
        assert!(content.contains('\n'));
    }

    #[test]
    fn returns_none_for_already_valid_json() {
        let valid = r#"{"file_path":"/tmp/a.js","content":"line1\\nline2"}"#;
        // Already valid → sanitize should return None (no modification needed).
        assert!(
            sanitize_json_string_literals(valid).is_none(),
            "no repair needed for already-valid JSON"
        );
    }

    #[test]
    fn returns_none_for_non_json_input() {
        assert!(sanitize_json_string_literals("write foo content: bar").is_none());
    }

    #[test]
    fn does_not_corrupt_escaped_sequences_already_in_string() {
        // Input has a proper \\n escape plus a literal newline — both should survive.
        let raw = "{\"a\":\"already\\\\n escaped\nand literal\"}";
        let value = parse_repaired(raw);
        let s = value["a"].as_str().unwrap();
        // The already-escaped \\n should remain as a literal backslash-n pair.
        assert!(s.contains("\\n"), "escaped \\n must not be double-escaped");
        // The literal newline should now be a real newline in the decoded value.
        assert!(
            s.contains('\n'),
            "literal newline must be present after decode"
        );
    }
}
