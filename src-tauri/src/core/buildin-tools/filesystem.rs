use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs as tokio_fs;

use crate::core::domain::{ToolExecutionRequest, ToolExecutionResult};
use crate::core::tool_runtime::{glob_match, truncate, ToolRuntime};

#[derive(Debug, Deserialize)]
struct LsToolInput {
    path: String,
    ignore: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ReadToolInput {
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EditToolInput {
    file_path: String,
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MultiEditToolInput {
    file_path: String,
    edits: Vec<EditOperationInput>,
}

#[derive(Debug, Deserialize)]
struct EditOperationInput {
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WriteToolInput {
    file_path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct NotebookEditToolInput {
    notebook_path: String,
    cell_id: Option<String>,
    new_source: String,
    cell_type: Option<String>,
    edit_mode: Option<String>,
}

impl ToolRuntime {
    pub(crate) async fn run_ls_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<LsToolInput>(&request.input, "LS")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(input.path.trim(), false, &execution_root)?;
        if !path.exists() || !path.is_dir() {
            bail!("LS path does not exist or is not a directory");
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let candidate = entry.path();
            let display = candidate.display().to_string();
            if input.ignore.as_ref().is_some_and(|ignores| {
                let rel = candidate
                    .strip_prefix(&path)
                    .unwrap_or(candidate.as_path())
                    .to_string_lossy()
                    .replace('\\', "/");
                ignores.iter().any(|pattern| glob_match(pattern, &rel))
            }) {
                continue;
            }
            entries.push(display);
        }
        entries.sort();
        let output = json!({
            "path": path.display().to_string(),
            "entries": entries,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(output),
        })
    }

    pub(crate) async fn run_read_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<ReadToolInput>(&request.input, "Read")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(&input.file_path, false, &execution_root)?;
        let content = tokio_fs::read_to_string(&path).await?;
        let all_lines = content.lines().collect::<Vec<_>>();
        let start_line = input.offset.unwrap_or(1).max(1);
        let start_idx = start_line.saturating_sub(1);
        let limit = input.limit.unwrap_or(2000).max(1);
        let end_idx = (start_idx + limit).min(all_lines.len());
        let width = (end_idx.max(1)).to_string().len().max(1);
        let mut rendered = String::new();
        for (idx, line) in all_lines[start_idx..end_idx].iter().enumerate() {
            let number = start_idx + idx + 1;
            rendered.push_str(&format!("{:>width$}\t{}\n", number, line, width = width));
        }
        let output = json!({
            "file_path": path.display().to_string(),
            "offset": start_line,
            "limit": limit,
            "content": truncate(&rendered, 30_000),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    pub(crate) async fn run_edit_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<EditToolInput>(&request.input, "Edit")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.old_string == input.new_string {
            bail!("old_string and new_string cannot be the same");
        }
        let path = self.resolve_path(&input.file_path, false, &execution_root)?;
        let content = tokio_fs::read_to_string(&path).await?;
        let occurrences = content.matches(&input.old_string).count();
        if occurrences == 0 {
            bail!("old_string not found in file");
        }
        let replace_all = input.replace_all.unwrap_or(false);
        if !replace_all && occurrences != 1 {
            bail!("old_string is not unique; use replace_all or provide more context");
        }
        let updated = if replace_all {
            content.replace(&input.old_string, &input.new_string)
        } else {
            content.replacen(&input.old_string, &input.new_string, 1)
        };
        tokio_fs::write(&path, updated.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "file_path": path.display().to_string(),
            "replacements": if replace_all { occurrences } else { 1 },
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    pub(crate) async fn run_multiedit_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<MultiEditToolInput>(&request.input, "MultiEdit")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        if input.edits.is_empty() {
            bail!("MultiEdit requires at least one edit");
        }
        let path = self.resolve_path(&input.file_path, false, &execution_root)?;
        let mut content = tokio_fs::read_to_string(&path).await?;
        let mut replacements = 0usize;
        for edit in &input.edits {
            if edit.old_string == edit.new_string {
                bail!("MultiEdit old_string and new_string cannot be identical");
            }
            let occurrences = content.matches(&edit.old_string).count();
            if occurrences == 0 {
                bail!("MultiEdit old_string not found");
            }
            let replace_all = edit.replace_all.unwrap_or(false);
            if !replace_all && occurrences != 1 {
                bail!("MultiEdit old_string is not unique");
            }
            content = if replace_all {
                replacements += occurrences;
                content.replace(&edit.old_string, &edit.new_string)
            } else {
                replacements += 1;
                content.replacen(&edit.old_string, &edit.new_string, 1)
            };
        }
        tokio_fs::write(&path, content.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "file_path": path.display().to_string(),
            "replacements": replacements,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    pub(crate) async fn run_write_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input = self.parse_json_input::<WriteToolInput>(&request.input, "Write")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(&input.file_path, true, &execution_root)?;
        tokio_fs::write(&path, input.content.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "file_path": path.display().to_string(),
            "bytes": input.content.len(),
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }

    pub(crate) async fn run_notebook_edit_compat_tool(
        &self,
        request: &ToolExecutionRequest,
    ) -> Result<ToolExecutionResult> {
        let input =
            self.parse_json_input::<NotebookEditToolInput>(&request.input, "NotebookEdit")?;
        let execution_root = self.resolve_execution_root(&request.working_directory)?;
        let path = self.resolve_path(&input.notebook_path, false, &execution_root)?;
        let raw = tokio_fs::read_to_string(&path).await?;
        let mut notebook: Value =
            serde_json::from_str(&raw).context("invalid notebook JSON content")?;
        let cells = notebook
            .get_mut("cells")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("notebook does not contain a cells array"))?;

        let mode = input
            .edit_mode
            .clone()
            .unwrap_or_else(|| "replace".to_string());
        let target_idx = input
            .cell_id
            .as_ref()
            .and_then(|cell_id| {
                cells.iter().position(|cell| {
                    cell.get("id")
                        .and_then(Value::as_str)
                        .map(|id| id == cell_id)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(0);

        match mode.as_str() {
            "delete" => {
                if cells.is_empty() {
                    bail!("cannot delete from an empty notebook");
                }
                let idx = target_idx.min(cells.len() - 1);
                cells.remove(idx);
            }
            "insert" => {
                let cell_type = input
                    .cell_type
                    .clone()
                    .unwrap_or_else(|| "code".to_string());
                let new_cell = json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "cell_type": cell_type,
                    "metadata": {},
                    "source": input.new_source,
                    "outputs": [],
                    "execution_count": Value::Null,
                });
                if cells.is_empty() {
                    cells.push(new_cell);
                } else {
                    let idx = (target_idx + 1).min(cells.len());
                    cells.insert(idx, new_cell);
                }
            }
            _ => {
                if cells.is_empty() {
                    bail!("cannot replace cell in an empty notebook");
                }
                let idx = target_idx.min(cells.len() - 1);
                let existing_type = cells[idx]
                    .get("cell_type")
                    .and_then(Value::as_str)
                    .unwrap_or("code")
                    .to_string();
                let next_type = input.cell_type.clone().unwrap_or(existing_type);
                cells[idx]["cell_type"] = Value::String(next_type);
                cells[idx]["source"] = Value::String(input.new_source);
                if cells[idx].get("id").is_none() {
                    cells[idx]["id"] = Value::String(uuid::Uuid::new_v4().to_string());
                }
            }
        }

        let serialized = serde_json::to_string_pretty(&notebook)?;
        tokio_fs::write(&path, serialized.as_bytes()).await?;
        let output = json!({
            "status": "ok",
            "notebook_path": path.display().to_string(),
            "edit_mode": mode,
        })
        .to_string();
        Ok(ToolExecutionResult {
            output: output.clone(),
            result_ref: Some(path.display().to_string()),
        })
    }
}
