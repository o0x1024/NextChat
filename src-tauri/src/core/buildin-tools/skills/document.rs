use super::{SkillDocument, SkillFrontmatterSummary, SkillMarkdownMeta};

pub(super) fn parse_skill_frontmatter_summary(raw: &str) -> SkillFrontmatterSummary {
    let mut summary = SkillFrontmatterSummary::default();
    let Some(frontmatter) = extract_frontmatter(raw) else {
        return summary;
    };
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.as_str() {
            "name" => summary.name = non_empty(value),
            "description" => summary.description = non_empty(value),
            "when_to_use" | "when-to-use" => summary.when_to_use = non_empty(value),
            _ => {}
        }
    }
    summary
}

pub(super) fn parse_skill_markdown(content: &str) -> SkillMarkdownMeta {
    let mut meta = SkillMarkdownMeta::default();
    let document = parse_skill_document(content);
    if !document.name.trim().is_empty() {
        meta.name = Some(document.name);
    }
    if !document.description.trim().is_empty() {
        meta.description = Some(document.description);
    }
    if let Some(frontmatter) = extract_frontmatter(content) {
        for line in frontmatter.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("tags:") {
                continue;
            }
            if let Some((_, raw_value)) = trimmed.split_once(':') {
                let cleaned = raw_value
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']');
                let tags = cleaned
                    .split(',')
                    .map(|part| part.trim().trim_matches('"').trim_matches('\''))
                    .filter(|part| !part.is_empty())
                    .map(str::to_string);
                meta.tags.extend(tags);
            }
        }
    }
    meta
}

pub(super) fn parse_skill_document(markdown: &str) -> SkillDocument {
    let mut document = SkillDocument {
        user_invocable: true,
        disable_model_invocation: false,
        ..SkillDocument::default()
    };

    if let Some(frontmatter) = extract_frontmatter(markdown) {
        for line in frontmatter.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let Some((key, raw_value)) = trimmed.split_once(':') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            let value = raw_value.trim().trim_matches('"').trim_matches('\'');
            match key.as_str() {
                "name" => document.name = value.to_string(),
                "description" => document.description = value.to_string(),
                "argument_hint" | "argument-hint" => {
                    document.argument_hint = non_empty(value);
                }
                "user_invocable" | "user-invocable" => {
                    document.user_invocable = parse_bool(value, true);
                }
                "disable_model_invocation" | "disable-model-invocation" => {
                    document.disable_model_invocation = parse_bool(value, false);
                }
                "allowed_tools" | "allowed-tools" => {
                    document.allowed_tools = non_empty(value);
                }
                "model" => document.model = non_empty(value),
                "context" => document.context = non_empty(value),
                "agent" => document.agent = non_empty(value),
                "hooks" => document.hooks_json = non_empty(value),
                "summary" => document.summary = non_empty(value),
                _ => {}
            }
        }
    }

    let body = strip_frontmatter(markdown);
    if document.name.is_empty() {
        document.name = body
            .lines()
            .find(|line| line.trim_start().starts_with("# "))
            .map(|line| {
                line.trim_start()
                    .trim_start_matches("# ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
    }
    if document.description.is_empty() {
        document.description = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
            .map(str::to_string)
            .unwrap_or_default();
    }
    document.content = body.trim().to_string();
    document
}

pub(super) fn build_skill_document(document: &SkillDocument) -> String {
    let mut lines = vec![
        "---".to_string(),
        format!("name: {}", document.name.trim()),
        format!("description: {}", document.description.trim()),
        format!("user-invocable: {}", document.user_invocable),
        format!(
            "disable-model-invocation: {}",
            document.disable_model_invocation
        ),
    ];

    if let Some(value) = non_empty(document.argument_hint.as_deref().unwrap_or("")) {
        lines.push(format!("argument-hint: {}", value));
    }
    if let Some(value) = non_empty(document.allowed_tools.as_deref().unwrap_or("")) {
        lines.push(format!("allowed-tools: {}", value));
    }
    if let Some(value) = non_empty(document.model.as_deref().unwrap_or("")) {
        lines.push(format!("model: {}", value));
    }
    if let Some(value) = non_empty(document.context.as_deref().unwrap_or("")) {
        lines.push(format!("context: {}", value));
    }
    if let Some(value) = non_empty(document.agent.as_deref().unwrap_or("")) {
        lines.push(format!("agent: {}", value));
    }
    if let Some(value) = non_empty(document.hooks_json.as_deref().unwrap_or("")) {
        lines.push(format!("hooks: {}", value));
    }
    if let Some(value) = non_empty(document.summary.as_deref().unwrap_or("")) {
        lines.push(format!("summary: {}", value));
    }
    lines.push("---".to_string());
    lines.push(String::new());
    if !document.content.trim().is_empty() {
        lines.push(document.content.trim().to_string());
    }
    lines.join("\n")
}

fn strip_frontmatter(markdown: &str) -> String {
    let mut lines = markdown.lines();
    if lines.next().map(str::trim) != Some("---") {
        return markdown.to_string();
    }
    let mut output = Vec::new();
    let mut in_frontmatter = true;
    for line in markdown.lines().skip(1) {
        if in_frontmatter && line.trim() == "---" {
            in_frontmatter = false;
            continue;
        }
        if !in_frontmatter {
            output.push(line);
        }
    }
    output.join("\n")
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_frontmatter(content: &str) -> Option<&str> {
    let mut segments = content.split_inclusive('\n');
    let first = segments.next()?;
    if first.trim() != "---" {
        return None;
    }
    let mut consumed = first.len();
    let start = consumed;
    for line in segments {
        if line.trim() == "---" {
            return content.get(start..consumed);
        }
        consumed += line.len();
    }
    None
}
