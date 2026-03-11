use super::ToolRuntime;
use crate::core::domain::{
    AgentPermissionPolicy, AgentProfile, MemoryPolicy, ModelPolicy, ToolExecutionRequest,
    ToolHandler,
};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_root(prefix: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("nextchat-{prefix}-{nanos}"))
}

fn agent() -> AgentProfile {
    AgentProfile {
        id: "agent-1".into(),
        name: "Builder".into(),
        avatar: "BD".into(),
        role: "Engineer".into(),
        objective: "Ship".into(),
        model_policy: ModelPolicy::default(),
        skill_ids: vec![],
        tool_ids: vec![
            "Read".into(),
            "Write".into(),
            "Grep".into(),
            "WebFetch".into(),
        ],
        max_parallel_runs: 1,
        can_spawn_subtasks: true,
        memory_policy: MemoryPolicy::default(),
        permission_policy: AgentPermissionPolicy::default(),
    }
}

#[tokio::test]
async fn read_and_write_tools_work() {
    let workspace_root = unique_root("workspace");
    let data_root = unique_root("data");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root.clone(), data_root).expect("runtime");
    let write_tool = runtime.tool_by_id("Write").expect("tool");
    let read_tool = runtime.tool_by_id("Read").expect("tool");

    let write_result = runtime
        .execute(ToolExecutionRequest {
            tool: write_tool,
            input: r#"{"file_path":"notes/spec.md","content":"hello"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: agent(),
            approval_granted: true,
            working_directory: ".".into(),
            tool_stream: None,
        })
        .await
        .expect("write");
    assert!(write_result.output.contains("\"status\":\"ok\""));

    let read_result = runtime
        .execute(ToolExecutionRequest {
            tool: read_tool,
            input: r#"{"file_path":"notes/spec.md"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: agent(),
            approval_granted: true,
            working_directory: ".".into(),
            tool_stream: None,
        })
        .await
        .expect("read");
    assert!(read_result.output.contains("hello"));
}

#[tokio::test]
async fn grep_returns_matches() {
    let workspace_root = unique_root("search-workspace");
    let data_root = unique_root("search-data");
    fs::create_dir_all(workspace_root.join("src")).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    fs::write(
        workspace_root.join("src/main.ts"),
        "const agentName = 'Scout';\nconsole.log(agentName);\n",
    )
    .expect("seed file");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
    let result = runtime
        .execute(ToolExecutionRequest {
            tool: runtime.tool_by_id("Grep").expect("tool"),
            input: r#"{"pattern":"Scout","output_mode":"content"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: agent(),
            approval_granted: true,
            working_directory: ".".into(),
            tool_stream: None,
        })
        .await
        .expect("search");
    assert!(result.output.contains("src/main.ts"));
    assert!(result.output.contains("Scout"));
}

#[test]
fn skill_allowlist_blocks_network_tool_authorization() {
    let workspace_root = unique_root("skill-workspace");
    let data_root = unique_root("skill-data");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
    let tool = runtime.tool_by_id("WebFetch").expect("tool");
    let mut restricted_agent = agent();
    restricted_agent.skill_ids = vec!["skill.builder".into()];

    let decision = runtime
        .authorize_tool_call(
            &restricted_agent,
            &tool,
            r#"{"url":"https://example.com","prompt":"summarize"}"#,
            ".",
        )
        .expect("decision");

    assert!(!decision.allowed);
    assert!(decision
        .reason
        .expect("reason")
        .contains("skills do not allow tool category"));
    assert!(runtime
        .available_tools_for_agent(&restricted_agent)
        .into_iter()
        .all(|candidate| candidate.id != "WebFetch"));
}

#[test]
fn tool_selection_does_not_fallback_to_todowrite_when_not_allowed() {
    let workspace_root = unique_root("selection-workspace");
    let data_root = unique_root("selection-data");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

    let selected = runtime.select_tool_for_text("please handle this request", &["Read".into()]);
    assert!(selected.is_none(), "unexpected fallback tool selected");
}

#[test]
fn tool_selection_falls_back_to_todowrite_when_allowed() {
    let workspace_root = unique_root("selection-workspace-allowed");
    let data_root = unique_root("selection-data-allowed");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

    let selected = runtime
        .select_tool_for_text("please handle this request", &["TodoWrite".into()])
        .expect("fallback tool");
    assert_eq!(selected.id, "TodoWrite");
}

#[tokio::test]
async fn file_tool_respects_agent_fs_roots() {
    let workspace_root = unique_root("permission-workspace");
    let data_root = unique_root("permission-data");
    fs::create_dir_all(workspace_root.join("allowed")).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");
    let tool = runtime.tool_by_id("Write").expect("tool");
    let mut restricted_agent = agent();
    restricted_agent.permission_policy.allow_fs_roots = vec!["allowed".into()];

    let error = runtime
        .execute(ToolExecutionRequest {
            tool: tool.clone(),
            input: r#"{"file_path":"blocked/spec.md","content":"hello"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: restricted_agent.clone(),
            approval_granted: true,
            working_directory: ".".into(),
            tool_stream: None,
        })
        .await
        .expect_err("blocked path should fail");
    assert!(error.to_string().contains("permission denied"));

    let result = runtime
        .execute(ToolExecutionRequest {
            tool,
            input: r#"{"file_path":"allowed/spec.md","content":"hello"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: restricted_agent,
            approval_granted: true,
            working_directory: ".".into(),
            tool_stream: None,
        })
        .await
        .expect("allowed path");
    assert!(result.output.contains("\"status\":\"ok\""));
}

#[tokio::test]
async fn tools_support_working_directories_outside_workspace_root() {
    let workspace_root = unique_root("workspace-root");
    let external_root = unique_root("external-root");
    let data_root = unique_root("external-data");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&external_root).expect("external");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

    let result = runtime
        .execute(ToolExecutionRequest {
            tool: runtime.tool_by_id("Write").expect("tool"),
            input: r#"{"file_path":"notes/from_external.md","content":"hello external"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: agent(),
            approval_granted: true,
            working_directory: external_root.display().to_string(),
            tool_stream: None,
        })
        .await
        .expect("write in external working directory");
    assert!(result.output.contains("\"status\":\"ok\""));
    assert!(external_root.join("notes/from_external.md").exists());
}

#[tokio::test]
async fn file_paths_cannot_escape_working_directory_scope() {
    let workspace_root = unique_root("escape-workspace");
    let external_root = unique_root("escape-external");
    let data_root = unique_root("escape-data");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&external_root).expect("external");
    fs::create_dir_all(&data_root).expect("data");
    let runtime = ToolRuntime::new(workspace_root, data_root).expect("runtime");

    let error = runtime
        .execute(ToolExecutionRequest {
            tool: runtime.tool_by_id("Write").expect("tool"),
            input: r#"{"file_path":"../outside.md","content":"nope"}"#.into(),
            task_card_id: "task-1".into(),
            agent_id: "agent-1".into(),
            agent: agent(),
            approval_granted: true,
            working_directory: external_root.display().to_string(),
            tool_stream: None,
        })
        .await
        .expect_err("escape should fail");
    assert!(
        error.to_string().contains("outside working directory"),
        "unexpected error: {}",
        error
    );
}
