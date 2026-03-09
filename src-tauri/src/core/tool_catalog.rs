use once_cell::sync::Lazy;

use crate::core::domain::{SkillPack, ToolManifest, ToolRiskLevel};

pub static BUILTIN_TOOLS: Lazy<Vec<ToolManifest>> = Lazy::new(|| {
    vec![
        ToolManifest {
            id: "file.readwrite".into(),
            name: "File Read/Write".into(),
            category: "filesystem".into(),
            risk_level: ToolRiskLevel::High,
            input_schema: r#"{"path":"string","mode":"read|write","content":"string?"}"#.into(),
            output_schema: r#"{"content":"string","saved":"boolean","path":"string"}"#.into(),
            timeout_ms: 30_000,
            concurrency_limit: 2,
            permissions: vec!["fs:read".into(), "fs:write".into()],
            description: "Inspect or modify local workspace files.".into(),
        },
        ToolManifest {
            id: "project.search".into(),
            name: "Project Search".into(),
            category: "workspace".into(),
            risk_level: ToolRiskLevel::Low,
            input_schema: r#"{"query":"string"}"#.into(),
            output_schema: r#"{"matches":["string"]}"#.into(),
            timeout_ms: 15_000,
            concurrency_limit: 4,
            permissions: vec!["workspace:index".into()],
            description: "Search code and notes inside the active workspace.".into(),
        },
        ToolManifest {
            id: "shell.exec".into(),
            name: "Shell Command".into(),
            category: "system".into(),
            risk_level: ToolRiskLevel::High,
            input_schema: r#"{"command":"string"}"#.into(),
            output_schema: r#"{"stdout":"string","stderr":"string","status":"number"}"#.into(),
            timeout_ms: 45_000,
            concurrency_limit: 1,
            permissions: vec!["system:shell".into()],
            description: "Run shell commands in an isolated worker.".into(),
        },
        ToolManifest {
            id: "http.request".into(),
            name: "HTTP Request".into(),
            category: "network".into(),
            risk_level: ToolRiskLevel::Medium,
            input_schema: r#"{"url":"string","method":"string","body":"string?"}"#.into(),
            output_schema: r#"{"status":"number","body":"string"}"#.into(),
            timeout_ms: 20_000,
            concurrency_limit: 3,
            permissions: vec!["network:http".into()],
            description: "Fetch remote APIs and websites with audit logging.".into(),
        },
        ToolManifest {
            id: "browser.automation".into(),
            name: "Browser Automation".into(),
            category: "browser".into(),
            risk_level: ToolRiskLevel::High,
            input_schema: r#"{"url":"string","actions":["string"]}"#.into(),
            output_schema: r#"{"snapshot":"string","result":"string"}"#.into(),
            timeout_ms: 60_000,
            concurrency_limit: 1,
            permissions: vec!["browser:automation".into()],
            description: "Drive a browser session for UI workflows.".into(),
        },
        ToolManifest {
            id: "markdown.compose".into(),
            name: "Markdown Compose".into(),
            category: "content".into(),
            risk_level: ToolRiskLevel::Low,
            input_schema: r#"{"topic":"string","format":"string"}"#.into(),
            output_schema: r#"{"markdown":"string"}"#.into(),
            timeout_ms: 8_000,
            concurrency_limit: 4,
            permissions: vec!["content:markdown".into()],
            description: "Draft markdown reports, specs, and release notes.".into(),
        },
        ToolManifest {
            id: "plan.summarize".into(),
            name: "Plan & Summarize".into(),
            category: "coordination".into(),
            risk_level: ToolRiskLevel::Low,
            input_schema: r#"{"input":"string"}"#.into(),
            output_schema: r#"{"summary":"string"}"#.into(),
            timeout_ms: 5_000,
            concurrency_limit: 8,
            permissions: vec!["coordination:summary".into()],
            description: "Generate execution summaries and concise plans.".into(),
        },
    ]
});

pub static BUILTIN_SKILLS: Lazy<Vec<SkillPack>> = Lazy::new(|| {
    vec![
        SkillPack {
            id: "skill.research".into(),
            name: "Research Sweep".into(),
            prompt_template: "Break the task into evidence collection, synthesis, and gaps.".into(),
            planning_rules: vec![
                "Collect facts before proposing action.".into(),
                "Surface uncertainty explicitly.".into(),
            ],
            allowed_tool_tags: vec!["workspace".into(), "network".into(), "coordination".into()],
            done_criteria: vec!["Summarize findings".into(), "Call out blockers".into()],
        },
        SkillPack {
            id: "skill.builder".into(),
            name: "Builder Loop".into(),
            prompt_template: "Prefer implementation-ready artifacts and concrete next steps."
                .into(),
            planning_rules: vec![
                "Decompose by dependency order.".into(),
                "Prefer actionable outputs.".into(),
            ],
            allowed_tool_tags: vec![
                "filesystem".into(),
                "workspace".into(),
                "coordination".into(),
            ],
            done_criteria: vec!["Produce a change set".into(), "Note verification".into()],
        },
        SkillPack {
            id: "skill.reviewer".into(),
            name: "Review Lens".into(),
            prompt_template: "Prioritize regressions, security, and missing tests.".into(),
            planning_rules: vec![
                "List findings by severity.".into(),
                "Separate findings from summary.".into(),
            ],
            allowed_tool_tags: vec!["workspace".into(), "coordination".into()],
            done_criteria: vec!["Identify risks".into(), "Recommend fixes".into()],
        },
    ]
});
