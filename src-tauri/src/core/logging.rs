use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::{json, Value};

static APP_LOGGER: OnceLock<FileLogger> = OnceLock::new();
static LLM_LOGGER: OnceLock<FileLogger> = OnceLock::new();

struct FileLogger {
    file: Mutex<File>,
}

impl FileLogger {
    fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create log directory '{}'", parent.display())
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open log file '{}'", path.display()))?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    fn write_json_line(&self, payload: &Value) -> Result<()> {
        let mut file = self.file.lock().expect("log file mutex poisoned");
        serde_json::to_writer(&mut *file, payload)?;
        writeln!(&mut *file)?;
        Ok(())
    }
}

pub fn init(log_dir: PathBuf) -> Result<()> {
    init_slot(&APP_LOGGER, log_dir.join("warn-error.log"))?;
    init_slot(&LLM_LOGGER, log_dir.join("llm.log"))?;
    Ok(())
}

fn init_slot(slot: &OnceLock<FileLogger>, path: PathBuf) -> Result<()> {
    if slot.get().is_some() {
        return Ok(());
    }

    let logger = FileLogger::new(path)?;
    let _ = slot.set(logger);
    Ok(())
}

pub fn warn(target: &str, message: impl AsRef<str>) {
    write_app_entry("WARN", target, message.as_ref());
}

pub fn error(target: &str, message: impl AsRef<str>) {
    write_app_entry("ERROR", target, message.as_ref());
}

pub fn llm_event(operation: &str, phase: &str, payload: Value) {
    let entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "operation": operation,
        "phase": phase,
        "payload": payload,
    });
    write_entry(LLM_LOGGER.get(), &entry);
}

pub fn truncate(value: &str, max_chars: usize) -> String {
    let total_chars = value.chars().count();
    if total_chars <= max_chars {
        return value.to_string();
    }

    let truncated = value.chars().take(max_chars).collect::<String>();
    format!(
        "{truncated}...<truncated {} chars>",
        total_chars - max_chars
    )
}

fn write_app_entry(level: &str, target: &str, message: &str) {
    let entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "level": level,
        "target": target,
        "message": message,
    });
    write_entry(APP_LOGGER.get(), &entry);
}

fn write_entry(logger: Option<&FileLogger>, payload: &Value) {
    if let Some(logger) = logger {
        let _ = logger.write_json_line(payload);
    }
}
