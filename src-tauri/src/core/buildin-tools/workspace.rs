use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use tokio::fs as tokio_fs;
use walkdir::WalkDir;

use crate::core::domain::{ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::{
    glob_match, should_skip_dir, should_skip_file, truncate, ToolRuntime,
};

#[derive(Debug, Deserialize)]
struct GlobToolInput {
    pattern: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GrepToolInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    output_mode: Option<String>,
    #[serde(rename = "-B")]
    before: Option<usize>,
    #[serde(rename = "-A")]
    after: Option<usize>,
    #[serde(rename = "-C")]
    context: Option<usize>,
    #[serde(rename = "-n")]
    line_number: Option<bool>,
    #[serde(rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(rename = "type")]
    file_type: Option<String>,
    head_limit: Option<usize>,
    multiline: Option<bool>,
}

impl ToolRuntime {
    pub(crate) async fn run_glob_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<GlobToolInput>(&request.input, "Glob")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.pattern.trim().is_empty() {
            bail!("glob pattern is required");
        }
        let root = if let Some(path) = input.path {
            self.resolve_path(path.trim(), false, &execution_root)?
        } else {
            execution_root
        };
        if !root.exists() || !root.is_dir() {
            bail!("glob path does not exist or is not a directory");
        }

        let mut matches = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|entry| !should_skip_dir(entry.path()))
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = path.strip_prefix(&root).unwrap_or(path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if glob_match(&input.pattern, &rel_str) {
                matches.push(path.display().to_string());
            }
        }
        matches.sort();

        let output = json!({
            "pattern": input.pattern,
            "path": root.display().to_string(),
            "matches": matches,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_grep_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<GrepToolInput>(&request.input, "Grep")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.pattern.trim().is_empty() {
            bail!("grep pattern is required");
        }
        let root = if let Some(path) = input.path.clone() {
            self.resolve_path(path.trim(), false, &execution_root)?
        } else {
            execution_root.clone()
        };
        let mode = input
            .output_mode
            .clone()
            .unwrap_or_else(|| "files_with_matches".to_string());
        let regex = if input.case_insensitive.unwrap_or(false) {
            Regex::new(&format!("(?i){}", input.pattern))
                .with_context(|| format!("invalid regex pattern '{}'", input.pattern))?
        } else {
            Regex::new(&input.pattern)
                .with_context(|| format!("invalid regex pattern '{}'", input.pattern))?
        };
        let mut output_rows = Vec::new();
        let mut files_with_matches = Vec::new();

        let mut candidates = Vec::new();
        if root.is_file() {
            candidates.push(root);
        } else {
            for entry in WalkDir::new(&root)
                .into_iter()
                .filter_entry(|entry| !should_skip_dir(entry.path()))
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if entry.file_type().is_file() && !should_skip_file(path) {
                    candidates.push(path.to_path_buf());
                }
            }
        }

        for path in candidates {
            if let Some(file_type) = input.file_type.as_ref() {
                let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
                if ext != file_type {
                    continue;
                }
            }
            let rel = path
                .strip_prefix(&execution_root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            if let Some(glob) = input.glob.as_ref() {
                if !glob_match(glob, &rel) {
                    continue;
                }
            }

            let content = tokio_fs::read_to_string(&path).await.unwrap_or_default();
            if input.multiline.unwrap_or(false) {
                if let Some(found) = regex.find(&content) {
                    files_with_matches.push(rel.clone());
                    if mode == "content" {
                        output_rows.push(format!("{}:{}", rel, truncate(found.as_str(), 240)));
                    }
                }
                continue;
            }

            let mut matched_lines = Vec::new();
            for (index, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matched_lines.push((index + 1, line.to_string()));
                }
            }
            if matched_lines.is_empty() {
                continue;
            }
            files_with_matches.push(rel.clone());
            if mode == "files_with_matches" {
                continue;
            }
            if mode == "count" {
                output_rows.push(format!("{}:{}", rel, matched_lines.len()));
                continue;
            }
            for (line_no, line_text) in matched_lines {
                let rendered_line = if input.line_number.unwrap_or(false) {
                    format!("{}:{}:{}", rel, line_no, line_text)
                } else {
                    format!("{}:{}", rel, line_text)
                };
                output_rows.push(rendered_line);
                let before = input.before.or(input.context).unwrap_or(0);
                let after = input.after.or(input.context).unwrap_or(0);
                if before > 0 || after > 0 {
                    let lines = content.lines().collect::<Vec<_>>();
                    let start = line_no.saturating_sub(before + 1);
                    let end = (line_no + after).min(lines.len());
                    for (i, ctx_line) in lines[start..end].iter().enumerate() {
                        let ctx_no = start + i + 1;
                        if ctx_no != line_no {
                            output_rows.push(format!("{}:{}-{}", rel, ctx_no, ctx_line));
                        }
                    }
                }
            }
        }

        if let Some(limit) = input.head_limit {
            let limit = limit.max(1);
            if mode == "files_with_matches" {
                files_with_matches.truncate(limit);
            } else {
                output_rows.truncate(limit);
            }
        }

        let output = match mode.as_str() {
            "count" => json!({
                "pattern": input.pattern,
                "output_mode": mode,
                "matches": output_rows,
            }),
            "content" => json!({
                "pattern": input.pattern,
                "output_mode": mode,
                "matches": output_rows,
            }),
            _ => json!({
                "pattern": input.pattern,
                "output_mode": "files_with_matches",
                "matches": files_with_matches,
            }),
        }
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }
}
