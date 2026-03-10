use super::AppService;
use crate::core::domain::{
    new_id, now, ClaimBid, ClaimScoreBreakdown, ConversationMessage, CreateWorkGroupInput, Lease,
    LeaseState, MemoryItem, MemoryScope, MessageKind, SenderKind, TaskCard, TaskStatus, ToolRun,
    ToolRunState, Visibility, WorkGroupKind,
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
fn clear_work_group_history_removes_messages_tasks_and_tool_artifacts() {
    let (service, _, _) = setup_service();

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "History Group".into(),
            goal: "Validate clear history behavior.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("work group");

    let message = ConversationMessage {
        id: new_id(),
        conversation_id: new_id(),
        work_group_id: work_group.id.clone(),
        sender_kind: SenderKind::Human,
        sender_id: "human".into(),
        sender_name: "Human".into(),
        kind: MessageKind::Text,
        visibility: Visibility::Main,
        content: "hello".into(),
        mentions: vec![],
        task_card_id: None,
        execution_mode: None,
        created_at: now(),
    };
    service
        .storage
        .insert_message(&message)
        .expect("insert message");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: message.id.clone(),
        title: "test task".into(),
        normalized_goal: "test".into(),
        input_payload: "{}".into(),
        priority: 1,
        status: TaskStatus::Completed,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some("agent-test".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_task_card(&task)
        .expect("insert task card");

    service
        .storage
        .insert_claim_bid(&ClaimBid {
            id: new_id(),
            task_card_id: task.id.clone(),
            agent_id: "agent-test".into(),
            rationale: "fit".into(),
            capability_score: 0.9,
            score_breakdown: ClaimScoreBreakdown::default(),
            expected_tools: vec!["Read".into()],
            estimated_cost: 0.1,
            created_at: now(),
        })
        .expect("insert claim bid");

    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: "agent-test".into(),
            state: LeaseState::Released,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: Some(now()),
        })
        .expect("insert lease");

    service
        .storage
        .insert_tool_run(&ToolRun {
            id: new_id(),
            tool_id: "Read".into(),
            task_card_id: task.id.clone(),
            agent_id: "agent-test".into(),
            state: ToolRunState::Completed,
            approval_required: false,
            started_at: Some(now()),
            finished_at: Some(now()),
            result_ref: Some("{}".into()),
        })
        .expect("insert tool run");

    let task_memory = MemoryItem {
        id: new_id(),
        scope: MemoryScope::Task,
        scope_id: task.id.clone(),
        content: "task note".into(),
        tags: vec!["history".into()],
        embedding_ref: None,
        pinned: false,
        ttl: None,
        created_at: now(),
    };
    service
        .storage
        .insert_memory_item(&task_memory)
        .expect("insert task memory");

    let group_memory = MemoryItem {
        id: new_id(),
        scope: MemoryScope::WorkGroup,
        scope_id: work_group.id.clone(),
        content: "group note".into(),
        tags: vec!["history".into()],
        embedding_ref: None,
        pinned: false,
        ttl: None,
        created_at: now(),
    };
    service
        .storage
        .insert_memory_item(&group_memory)
        .expect("insert group memory");

    service
        .clear_work_group_history(&work_group.id)
        .expect("clear history");

    service
        .storage
        .get_work_group(&work_group.id)
        .expect("work group should remain");
    assert!(
        service
            .storage
            .list_messages_for_group(&work_group.id)
            .expect("messages")
            .is_empty(),
        "messages in group should be removed"
    );
    assert!(
        service
            .storage
            .list_task_cards(Some(&work_group.id))
            .expect("tasks")
            .is_empty(),
        "tasks in group should be removed"
    );
    assert!(
        service
            .storage
            .list_claim_bids()
            .expect("claim bids")
            .into_iter()
            .all(|bid| bid.task_card_id != task.id),
        "claim bids for cleared tasks should be removed"
    );
    assert!(
        service
            .storage
            .list_leases()
            .expect("leases")
            .into_iter()
            .all(|lease| lease.task_card_id != task.id),
        "leases for cleared tasks should be removed"
    );
    assert!(
        service
            .storage
            .list_tool_runs()
            .expect("tool runs")
            .into_iter()
            .all(|run| run.task_card_id != task.id),
        "tool runs for cleared tasks should be removed"
    );

    let memory_items = service.storage.list_memory_items().expect("memory");
    assert!(
        memory_items
            .iter()
            .all(|item| !(matches!(item.scope, MemoryScope::Task) && item.scope_id == task.id)),
        "task-scoped memory should be removed"
    );
    assert!(
        memory_items
            .iter()
            .any(|item| matches!(item.scope, MemoryScope::WorkGroup)
                && item.scope_id == work_group.id),
        "work-group memory should be retained"
    );
}

#[test]
fn clear_work_group_history_rejects_when_active_leases_exist() {
    let (service, _, _) = setup_service();

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Active Group".into(),
            goal: "Ensure active tasks block clear history.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("work group");

    let message = ConversationMessage {
        id: new_id(),
        conversation_id: new_id(),
        work_group_id: work_group.id.clone(),
        sender_kind: SenderKind::Human,
        sender_id: "human".into(),
        sender_name: "Human".into(),
        kind: MessageKind::Text,
        visibility: Visibility::Main,
        content: "active task".into(),
        mentions: vec![],
        task_card_id: None,
        execution_mode: None,
        created_at: now(),
    };
    service
        .storage
        .insert_message(&message)
        .expect("insert message");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: message.id.clone(),
        title: "active task".into(),
        normalized_goal: "active".into(),
        input_payload: "{}".into(),
        priority: 1,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some("agent-test".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_task_card(&task)
        .expect("insert task");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: "agent-test".into(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("insert active lease");

    let error = service
        .clear_work_group_history(&work_group.id)
        .expect_err("clear history should fail with active lease");
    assert!(error
        .to_string()
        .contains("cannot clear history while active tasks are running"));
    assert_eq!(
        service
            .storage
            .list_messages_for_group(&work_group.id)
            .expect("messages")
            .len(),
        1
    );
    assert_eq!(
        service
            .storage
            .list_task_cards(Some(&work_group.id))
            .expect("tasks")
            .len(),
        1
    );
}
