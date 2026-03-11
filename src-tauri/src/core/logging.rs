use std::{
    fmt::Write as FmtWrite,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use chrono::{Local, Months, NaiveDate, Utc};
use flate2::{write::GzEncoder, Compression};
use serde_json::{json, Value};

static APP_LOGGER: OnceLock<DailyFileLogger> = OnceLock::new();
static LLM_LOGGER: OnceLock<DailyFileLogger> = OnceLock::new();
static TOOLCALL_LOGGER: OnceLock<DailyFileLogger> = OnceLock::new();

#[derive(Clone, Copy)]
enum LogFormat {
    JsonLine,
    TextBlock,
}

struct LoggerState {
    active_date: NaiveDate,
    file: File,
}

struct DailyFileLogger {
    log_dir: PathBuf,
    prefix: &'static str,
    format: LogFormat,
    state: Mutex<LoggerState>,
}

impl DailyFileLogger {
    fn new(log_dir: PathBuf, prefix: &'static str, format: LogFormat) -> Result<Self> {
        fs::create_dir_all(&log_dir)
            .with_context(|| format!("failed to create log directory '{}'", log_dir.display()))?;

        migrate_legacy_logs(&log_dir, prefix)?;
        maintain_log_files(&log_dir, prefix)?;

        let today = current_log_date();
        let file = open_log_file(&log_dir, prefix, today)?;
        Ok(Self {
            log_dir,
            prefix,
            format,
            state: Mutex::new(LoggerState {
                active_date: today,
                file,
            }),
        })
    }

    fn write_json_line(&self, payload: &Value) -> Result<()> {
        self.with_active_file(|file| {
            serde_json::to_writer(&mut *file, payload)?;
            writeln!(&mut *file)?;
            Ok(())
        })
    }

    fn write_text_block(&self, content: &str) -> Result<()> {
        self.with_active_file(|file| {
            writeln!(&mut *file, "{content}")?;
            Ok(())
        })
    }

    fn with_active_file(&self, writer: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
        let mut state = self.state.lock().expect("log file mutex poisoned");
        let today = current_log_date();
        if state.active_date != today {
            state.file = open_log_file(&self.log_dir, self.prefix, today)?;
            state.active_date = today;
            maintain_log_files(&self.log_dir, self.prefix)?;
        }
        writer(&mut state.file)?;
        Ok(())
    }
}

pub fn init(log_dir: PathBuf) -> Result<()> {
    init_slot(
        &APP_LOGGER,
        log_dir.clone(),
        "nextchat",
        LogFormat::JsonLine,
    )?;
    init_slot(&LLM_LOGGER, log_dir.clone(), "llm", LogFormat::TextBlock)?;
    init_slot(&TOOLCALL_LOGGER, log_dir, "toolcall", LogFormat::TextBlock)?;
    Ok(())
}

fn init_slot(
    slot: &OnceLock<DailyFileLogger>,
    log_dir: PathBuf,
    prefix: &'static str,
    format: LogFormat,
) -> Result<()> {
    if slot.get().is_some() {
        return Ok(());
    }

    let logger = DailyFileLogger::new(log_dir, prefix, format)?;
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
    let timestamp = Utc::now().to_rfc3339();
    let entry = format_llm_entry(&timestamp, operation, phase, &payload);
    if entry.trim().is_empty() {
        return;
    }
    let target_logger = match phase {
        "tool_call" | "tool_result" => TOOLCALL_LOGGER.get(),
        _ => LLM_LOGGER.get(),
    };
    write_text_entry(target_logger, &entry);
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

fn write_entry(logger: Option<&DailyFileLogger>, payload: &Value) {
    if let Some(logger) = logger {
        let _ = match logger.format {
            LogFormat::JsonLine => logger.write_json_line(payload),
            LogFormat::TextBlock => logger.write_text_block(&payload.to_string()),
        };
    }
}

fn write_text_entry(logger: Option<&DailyFileLogger>, content: &str) {
    if let Some(logger) = logger {
        let _ = logger.write_text_block(content);
    }
}

fn current_log_date() -> NaiveDate {
    Local::now().date_naive()
}

fn open_log_file(log_dir: &Path, prefix: &str, date: NaiveDate) -> Result<File> {
    let path = log_dir.join(dated_log_file_name(prefix, date));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open log file '{}'", path.display()))
}

fn dated_log_file_name(prefix: &str, date: NaiveDate) -> String {
    format!("{}-{}.log", prefix, date.format("%Y-%m-%d"))
}

fn dated_compressed_log_file_name(prefix: &str, date: NaiveDate) -> String {
    format!("{}-{}.log.gz", prefix, date.format("%Y-%m-%d"))
}

fn migrate_legacy_logs(log_dir: &Path, prefix: &str) -> Result<()> {
    if prefix == "nextchat" {
        migrate_legacy_file(log_dir, "warn-error.log", prefix)?;
    }
    migrate_legacy_file(log_dir, &format!("{prefix}.log"), prefix)?;
    Ok(())
}

fn migrate_legacy_file(log_dir: &Path, legacy_name: &str, prefix: &str) -> Result<()> {
    let legacy_path = log_dir.join(legacy_name);
    if !legacy_path.exists() {
        return Ok(());
    }

    let today = current_log_date();
    let target_path = log_dir.join(dated_log_file_name(prefix, today));
    append_file_contents(&legacy_path, &target_path)?;
    fs::remove_file(&legacy_path)
        .with_context(|| format!("failed to remove legacy log '{}'", legacy_path.display()))?;
    Ok(())
}

fn append_file_contents(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory '{}'", parent.display()))?;
    }

    let mut reader = File::open(source)
        .with_context(|| format!("failed to open source log '{}'", source.display()))?;
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(target)
        .with_context(|| format!("failed to open target log '{}'", target.display()))?;
    io::copy(&mut reader, &mut writer)?;
    Ok(())
}

fn maintain_log_files(log_dir: &Path, prefix: &str) -> Result<()> {
    let today = current_log_date();
    let delete_before = today.checked_sub_months(Months::new(6)).unwrap_or(today);

    for entry in fs::read_dir(log_dir)
        .with_context(|| format!("failed to read log directory '{}'", log_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some((date, compressed)) = parse_dated_log_name(prefix, file_name) else {
            continue;
        };

        if date < delete_before {
            fs::remove_file(&path)
                .with_context(|| format!("failed to delete expired log '{}'", path.display()))?;
            continue;
        }

        if compressed || date >= today {
            continue;
        }

        compress_log_file(log_dir, prefix, date)?;
    }

    Ok(())
}

fn parse_dated_log_name(prefix: &str, file_name: &str) -> Option<(NaiveDate, bool)> {
    let dated_part = file_name.strip_prefix(&format!("{prefix}-"))?;
    if let Some(date_part) = dated_part.strip_suffix(".log.gz") {
        return NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
            .ok()
            .map(|date| (date, true));
    }
    if let Some(date_part) = dated_part.strip_suffix(".log") {
        return NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
            .ok()
            .map(|date| (date, false));
    }
    None
}

fn compress_log_file(log_dir: &Path, prefix: &str, date: NaiveDate) -> Result<()> {
    let source_path = log_dir.join(dated_log_file_name(prefix, date));
    if !source_path.exists() {
        return Ok(());
    }

    let compressed_path = log_dir.join(dated_compressed_log_file_name(prefix, date));
    let mut source = File::open(&source_path)
        .with_context(|| format!("failed to open log '{}'", source_path.display()))?;
    let compressed_file = File::create(&compressed_path).with_context(|| {
        format!(
            "failed to create compressed log '{}'",
            compressed_path.display()
        )
    })?;
    let mut encoder = GzEncoder::new(compressed_file, Compression::default());
    io::copy(&mut source, &mut encoder)?;
    encoder.finish()?;
    fs::remove_file(&source_path)
        .with_context(|| format!("failed to remove log '{}'", source_path.display()))?;
    Ok(())
}

fn format_llm_entry(_timestamp: &str, _operation: &str, _phase: &str, payload: &Value) -> String {
    let mut output = String::new();
    if let Some(preamble) = payload.get("preamble").and_then(Value::as_str) {
        append_section(&mut output, "system prompt", preamble);
    }
    if let Some(prompt) = payload.get("prompt").and_then(Value::as_str) {
        let rendered_prompt = render_message_text(prompt);
        append_section(&mut output, "user prompt", &rendered_prompt);
    }
    if let Some(args) = payload.get("args").and_then(Value::as_str) {
        let rendered_args = render_message_text(args);
        append_section(&mut output, "tool args", &rendered_args);
    }
    if let Some(result) = payload.get("result").and_then(Value::as_str) {
        let rendered_result = render_message_text(result);
        append_section(&mut output, "tool result", &rendered_result);
    }
    if let Some(choice) = payload.get("choice").and_then(Value::as_str) {
        let rendered_choice = render_message_text(choice);
        append_section(&mut output, "response", &rendered_choice);
    }
    if let Some(summary) = payload.get("summary").and_then(Value::as_str) {
        append_section(&mut output, "response", summary);
    }
    if let Some(error) = payload.get("error").and_then(Value::as_str) {
        append_section(&mut output, "error", error);
    }
    if let Some(reason) = payload.get("reason").and_then(Value::as_str) {
        append_section(&mut output, "reason", reason);
    }

    output.trim_end().to_string()
}

fn append_section(output: &mut String, title: &str, content: &str) {
    let text = content.trim();
    let _ = writeln!(output);
    let _ = writeln!(output, "\n******************{title}******************\n");
    if text.is_empty() {
        let _ = writeln!(output, "<empty>");
    } else {
        let _ = writeln!(output, "{text}");
    }
}

fn render_message_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if !is_json_like(trimmed) {
        return raw.to_string();
    }

    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => {
            let mut lines = Vec::new();
            collect_text_lines(&value, &mut lines);
            let joined = lines
                .into_iter()
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            if joined.is_empty() {
                raw.to_string()
            } else {
                joined
            }
        }
        Err(_) => raw.to_string(),
    }
}

fn is_json_like(value: &str) -> bool {
    value.starts_with('{') || value.starts_with('[')
}

fn collect_text_lines(value: &Value, lines: &mut Vec<String>) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(text) => lines.push(text.to_string()),
        Value::Array(items) => {
            for item in items {
                collect_text_lines(item, lines);
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                lines.push(text.to_string());
            }
            if let Some(content) = map.get("content") {
                collect_text_lines(content, lines);
            }
            if let Some(prompt) = map.get("prompt") {
                collect_text_lines(prompt, lines);
            }
            if let Some(choice) = map.get("choice") {
                collect_text_lines(choice, lines);
            }
            if let Some(summary) = map.get("summary") {
                collect_text_lines(summary, lines);
            }
            if let Some(result) = map.get("result") {
                collect_text_lines(result, lines);
            }
            if let Some(args) = map.get("args") {
                collect_text_lines(args, lines);
            }
            if let Some(message) = map.get("message") {
                collect_text_lines(message, lines);
            }
        }
    }
}
