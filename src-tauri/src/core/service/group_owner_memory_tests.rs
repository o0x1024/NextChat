use super::AppService;
use crate::core::domain::{
    AgentPermissionPolicy, CreateAgentInput, CreateWorkGroupInput, MemoryPolicy, MemoryScope,
    MessageKind, SendHumanMessageInput, WorkGroupKind,
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
fn create_work_group_supports_initial_members_and_rejects_unknown_agents() {
    let (service, _, _) = setup_service();
    let alpha = service
        .create_agent_profile(CreateAgentInput {
            name: "Alpha".into(),
            avatar: "A".into(),
            role: "Planner".into(),
            objective: "Plan work.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.builder".into()],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("alpha");
    let beta = service
        .create_agent_profile(CreateAgentInput {
            name: "Beta".into(),
            avatar: "B".into(),
            role: "Reviewer".into(),
            objective: "Review work.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.reviewer".into()],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("beta");

    let group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Initial Members".into(),
            goal: "Verify member assignment during creation.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: Some(vec![alpha.id.clone(), beta.id.clone(), alpha.id.clone()]),
        })
        .expect("group");
    let owner_agent_id = service
        .storage
        .get_work_group_owner_id(&group.id)
        .expect("owner query")
        .expect("owner should exist");
    assert_eq!(group.member_agent_ids[0], owner_agent_id);
    assert_eq!(group.member_agent_ids.len(), 3);
    assert!(group.member_agent_ids.contains(&alpha.id));
    assert!(group.member_agent_ids.contains(&beta.id));
    let owner = service
        .storage
        .get_agent(&owner_agent_id)
        .expect("owner profile");
    assert_eq!(owner.name, "群主");
    assert_eq!(owner.role, "Group Owner");
    assert!(owner.objective.contains("群主"));

    let agents_before_invalid = service.storage.list_agents().expect("agents").len();

    let invalid = service.create_work_group(CreateWorkGroupInput {
        name: "Invalid Members".into(),
        goal: "Should fail".into(),
        working_directory: ".".into(),
        kind: WorkGroupKind::Persistent,
        default_visibility: "summary".into(),
        auto_archive: false,
        member_agent_ids: Some(vec!["missing-agent".into()]),
    });
    assert!(invalid.is_err());
    assert!(invalid
        .expect_err("invalid member should fail")
        .to_string()
        .contains("agent not found"));
    let agents_after_invalid = service.storage.list_agents().expect("agents").len();
    assert_eq!(
        agents_before_invalid, agents_after_invalid,
        "failed creation should not leave orphan owner agents"
    );
}

#[test]
fn send_human_message_records_group_chat_memory() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let specialist = service
        .create_agent_profile(CreateAgentInput {
            name: "Executor".into(),
            avatar: "EX".into(),
            role: "Worker".into(),
            objective: "Execute assigned tasks.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("specialist");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Memory Feed".into(),
            goal: "Track directives".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: Some(vec![specialist.id.clone()]),
        })
        .expect("group");

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "请先整理需求，再安排成员并行完成，最后输出总结。".into(),
            },
        )
        .expect("send message");

    let directive_memory = service
        .storage
        .list_memory_items()
        .expect("memory")
        .into_iter()
        .find(|item| {
            item.scope == MemoryScope::WorkGroup
                && item.scope_id == work_group.id
                && item.tags.iter().any(|tag| tag == "chat_memory")
        })
        .expect("directive memory");
    assert!(directive_memory.content.starts_with("Human directive:"));
    assert_eq!(directive_memory.ttl, Some(14 * 24 * 60 * 60));
}

#[test]
fn builtin_group_owner_is_reused_across_groups() {
    let (service, _, _) = setup_service();

    let g1 = service
        .create_work_group(CreateWorkGroupInput {
            name: "G1".into(),
            goal: "Goal 1".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group1");
    let g2 = service
        .create_work_group(CreateWorkGroupInput {
            name: "G2".into(),
            goal: "Goal 2".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group2");

    let owner1 = service
        .storage
        .get_work_group_owner_id(&g1.id)
        .expect("owner1 query")
        .expect("owner1");
    let owner2 = service
        .storage
        .get_work_group_owner_id(&g2.id)
        .expect("owner2 query")
        .expect("owner2");
    assert_eq!(owner1, owner2, "groups should reuse one builtin owner");
}

#[test]
fn delete_group_does_not_delete_builtin_owner_agent() {
    let (service, _, _) = setup_service();

    let g1 = service
        .create_work_group(CreateWorkGroupInput {
            name: "Delete Target".into(),
            goal: "Goal 1".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group1");
    let g2 = service
        .create_work_group(CreateWorkGroupInput {
            name: "Keep Group".into(),
            goal: "Goal 2".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group2");

    let owner = service
        .storage
        .get_work_group_owner_id(&g2.id)
        .expect("owner query")
        .expect("owner");
    service.delete_work_group(&g1.id).expect("delete group");

    let owner_agent = service.storage.get_agent(&owner).expect("owner agent");
    assert_eq!(owner_agent.name, "群主");
    assert_eq!(owner_agent.role, "Group Owner");
}

#[test]
fn builtin_group_owner_cannot_be_deleted() {
    let (service, _, _) = setup_service();
    let group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Owner Guard".into(),
            goal: "Ensure builtin owner cannot be deleted".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    let owner_id = service
        .storage
        .get_work_group_owner_id(&group.id)
        .expect("owner query")
        .expect("owner");

    let err = service
        .delete_agent_profile(&owner_id)
        .expect_err("builtin owner delete should fail");
    assert!(err
        .to_string()
        .contains("cannot delete builtin group owner"));
    assert!(
        service.storage.get_agent(&owner_id).is_ok(),
        "builtin owner should still exist"
    );
}

#[test]
fn coordination_messages_are_sent_by_group_owner_agent() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let worker = service
        .create_agent_profile(CreateAgentInput {
            name: "Worker".into(),
            avatar: "WK".into(),
            role: "Executor".into(),
            objective: "Execute tasks quickly.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("worker");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Owner Messages".into(),
            goal: "Verify sender identity.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: Some(vec![worker.id.clone()]),
        })
        .expect("group");

    let human_message = service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "请审计下当前项目".into(),
            },
        )
        .expect("human message");

    let task = service
        .storage
        .list_task_cards(Some(&work_group.id))
        .expect("tasks")
        .into_iter()
        .find(|task| task.source_message_id == human_message.id)
        .expect("task");
    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");

    let owner_coordination_messages = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages")
        .into_iter()
        .filter(|message| {
            message.task_card_id.as_deref() == Some(task.id.as_str())
                && matches!(
                    message.kind,
                    MessageKind::Status | MessageKind::Summary | MessageKind::Approval
                )
                && message.sender_id == owner_id
                && message.sender_name == "群主"
        })
        .collect::<Vec<_>>();
    assert!(
        !owner_coordination_messages.is_empty(),
        "coordination messages should be emitted by group owner"
    );
}

#[test]
fn builtin_group_owner_is_not_executor_candidate() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let worker = service
        .create_agent_profile(CreateAgentInput {
            name: "CodeAuditor".into(),
            avatar: "CA".into(),
            role: "Security Code Reviewer".into(),
            objective: "Audit codebases.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("worker");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Owner Delegate Only".into(),
            goal: "Owner organizes, worker executes.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: Some(vec![worker.id.clone()]),
        })
        .expect("group");

    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");
    let human_message = service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "请@群主组织任务并由 CodeAuditor 执行".into(),
            },
        )
        .expect("send");
    let task = service
        .storage
        .list_task_cards(Some(&work_group.id))
        .expect("tasks")
        .into_iter()
        .find(|task| task.source_message_id == human_message.id)
        .expect("task");
    let bids = service
        .storage
        .list_claim_bids()
        .expect("bids")
        .into_iter()
        .filter(|bid| bid.task_card_id == task.id)
        .collect::<Vec<_>>();
    assert!(
        bids.iter().all(|bid| bid.agent_id != owner_id),
        "builtin group owner should not participate in executor bidding"
    );
}
