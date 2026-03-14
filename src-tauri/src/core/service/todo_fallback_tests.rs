use super::AppService;
use crate::core::domain::{
    AgentPermissionPolicy, CreateAgentInput, CreateWorkGroupInput, MemoryPolicy,
    SendHumanMessageInput, SenderKind, TaskStatus, WorkGroupKind,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("nextchat-service-{prefix}-{nanos}"))
}

fn setup_service() -> (AppService, PathBuf, PathBuf) {
    let workspace_root = unique_root("workspace");
    let data_root = unique_root("data");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&data_root).expect("data");
    let service = AppService::new(workspace_root.clone(), data_root.clone()).expect("service");
    (service, workspace_root, data_root)
}

#[test]
fn plain_message_does_not_trigger_todowrite_permission_denial_when_unbound() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let auditor = service
        .create_agent_profile(CreateAgentInput {
            name: "CodeAuditor".into(),
            avatar: "CA".into(),
            role: "代码安全审计专家".into(),
            objective: "识别和报告安全漏洞".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec![
                "Read".into(),
                "Grep".into(),
                "LS".into(),
                "WebSearch".into(),
            ],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("auditor");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Fallback Guard Group".into(),
            goal: "Verify TodoWrite fallback handling.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &auditor.id)
        .expect("add auditor");

    let human_message = service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "请审计这个仓库并给出风险结论".into(),
            },
        )
        .expect("send message");

    let task = service
        .storage
        .list_task_cards(Some(&work_group.id))
        .expect("tasks")
        .into_iter()
        .find(|item| item.source_message_id == human_message.id)
        .expect("task for human message");

    assert_ne!(
        task.status,
        TaskStatus::NeedsReview,
        "task should not fail immediately due to TodoWrite fallback"
    );
    assert!(
        service
            .storage
            .list_tool_runs()
            .expect("tool runs")
            .into_iter()
            .filter(|run| run.task_card_id == task.id)
            .all(|run| !matches!(
                run.tool_id.as_str(),
                "TaskCreate" | "TaskGet" | "TaskUpdate" | "TaskList"
            )),
        "task should not enqueue task management tools when they are not bound"
    );
    assert!(
        service
            .storage
            .list_messages_for_group(&work_group.id)
            .expect("messages")
            .into_iter()
            .filter(|message| {
                message.task_card_id.as_deref() == Some(task.id.as_str())
                    && matches!(message.sender_kind, SenderKind::System)
            })
            .all(|message| !message.content.contains("cannot use Task")),
        "task should not produce task tool permission denial message"
    );
}

#[test]
fn mentioned_agent_without_todowrite_is_not_failed_by_global_todowrite_fallback() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let auditor = service
        .create_agent_profile(CreateAgentInput {
            name: "CodeAuditor".into(),
            avatar: "CA".into(),
            role: "代码安全审计专家".into(),
            objective: "识别和报告安全漏洞".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec![
                "Read".into(),
                "Grep".into(),
                "LS".into(),
                "WebSearch".into(),
            ],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("auditor");
    let planner = service
        .create_agent_profile(CreateAgentInput {
            name: "Planner".into(),
            avatar: "PL".into(),
            role: "Planning Lead".into(),
            objective: "拆解任务并维护待办".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec![
                "TaskCreate".into(),
                "TaskUpdate".into(),
                "TaskList".into(),
                "Read".into(),
            ],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("planner");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Fallback Multi Group".into(),
            goal: "Verify fallback doesn't break mentioned agent.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &auditor.id)
        .expect("add auditor");
    service
        .add_agent_to_work_group(&work_group.id, &planner.id)
        .expect("add planner");

    let human_message = service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "@CodeAuditor 请审计这个仓库并给出风险结论".into(),
            },
        )
        .expect("send message");

    let task = service
        .storage
        .list_task_cards(Some(&work_group.id))
        .expect("tasks")
        .into_iter()
        .find(|item| item.source_message_id == human_message.id)
        .expect("task for human message");

    assert_ne!(
        task.status,
        TaskStatus::NeedsReview,
        "task should not fail because another member exposes task management tools"
    );
    assert!(
        service
            .storage
            .list_messages_for_group(&work_group.id)
            .expect("messages")
            .into_iter()
            .filter(|message| {
                message.task_card_id.as_deref() == Some(task.id.as_str())
                    && matches!(message.sender_kind, SenderKind::System)
            })
            .all(|message| !message.content.contains("cannot use Task")),
        "task should not produce task tool permission denial for the mentioned agent"
    );
}
