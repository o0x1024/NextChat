use super::AppService;
use crate::core::domain::{
    new_id, now, AgentPermissionPolicy, CreateAgentInput, CreateWorkGroupInput, Lease, LeaseState,
    MemoryItem, MemoryPolicy, MemoryScope, MessageKind, SendHumanMessageInput, TaskCard,
    TaskStatus, ToolRun, ToolRunState, WorkGroupKind,
};
use anyhow::anyhow;
use std::{
    fs,
    path::PathBuf,
    thread,
    time::Duration,
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
fn minimal_group_flow_creates_task_lease_and_agent_summary() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let planner = service
        .create_agent_profile(CreateAgentInput {
            name: "Planner".into(),
            avatar: "PL".into(),
            role: "Planning Lead".into(),
            objective: "Produce clear plans and summaries.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.builder".into()],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("planner");
    let reviewer = service
        .create_agent_profile(CreateAgentInput {
            name: "Reviewer2".into(),
            avatar: "R2".into(),
            role: "Review Lead".into(),
            objective: "Review plans and keep answers concise.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.reviewer".into()],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("reviewer");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Smoke Group".into(),
            goal: "Validate the minimal task flow.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &planner.id)
        .expect("add planner");
    service
        .add_agent_to_work_group(&work_group.id, &reviewer.id)
        .expect("add reviewer");

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "Please create a concise plan for launch readiness.".into(),
            },
        )
        .expect("send message");

    let task_id = (0..40)
        .find_map(|_| {
            let task = service
                .storage
                .list_task_cards(Some(&work_group.id))
                .expect("tasks")
                .into_iter()
                .find(|task| task.created_by == "human");
            if let Some(task) = task {
                Some(task.id)
            } else {
                thread::sleep(Duration::from_millis(50));
                None
            }
        })
        .expect("task created");

    let final_task = (0..80)
        .find_map(|_| {
            let task = service.storage.get_task_card(&task_id).expect("task");
            if matches!(
                task.status,
                TaskStatus::Completed | TaskStatus::WaitingApproval | TaskStatus::NeedsReview
            ) {
                Some(task)
            } else {
                thread::sleep(Duration::from_millis(50));
                None
            }
        })
        .expect("task reached terminal-ish state");

    assert_eq!(final_task.status, TaskStatus::Completed);

    let lease = service
        .storage
        .get_lease_by_task(&task_id)
        .expect("lease query")
        .expect("lease exists");
    assert_eq!(lease.state, LeaseState::Released);

    let bids = service
        .storage
        .list_claim_bids()
        .expect("bids")
        .into_iter()
        .filter(|bid| bid.task_card_id == task_id)
        .collect::<Vec<_>>();
    assert!(!bids.is_empty(), "expected at least one claim bid");

    let summary_messages = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages")
        .into_iter()
        .filter(|message| {
            message.task_card_id.as_deref() == Some(task_id.as_str())
                && matches!(message.sender_kind, crate::core::domain::SenderKind::Agent)
        })
        .collect::<Vec<_>>();
    assert!(
        !summary_messages.is_empty(),
        "expected an agent summary message for the task"
    );
}

#[test]
fn permission_denial_moves_task_to_review_and_records_audit() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let builder = service
        .create_agent_profile(CreateAgentInput {
            name: "Builder".into(),
            avatar: "BD".into(),
            role: "Systems Engineer".into(),
            objective: "Write files safely.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.builder".into()],
            tool_ids: vec!["file.readwrite".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy {
                allow_tool_ids: vec![],
                deny_tool_ids: vec![],
                require_approval_tool_ids: vec![],
                allow_fs_roots: vec!["allowed".into()],
                allow_network_domains: vec![],
            },
        })
        .expect("builder");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Permissions Group".into(),
            goal: "Verify permission denials.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &builder.id)
        .expect("add builder");

    service
        .send_human_message(
            app_handle.clone(),
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "Write blocked/spec.md and save content: hello".into(),
            },
        )
        .expect("send message");

    let denied_task = (0..40)
        .find_map(|_| {
            service
                .storage
                .list_task_cards(Some(&work_group.id))
                .expect("tasks")
                .into_iter()
                .find(|task| task.created_by == "human" && task.status == TaskStatus::NeedsReview)
                .or_else(|| {
                    thread::sleep(Duration::from_millis(50));
                    None
                })
        })
        .expect("task reached review");

    assert_eq!(denied_task.status, TaskStatus::NeedsReview);
    assert!(
        service
            .storage
            .list_tool_runs()
            .expect("tool runs")
            .into_iter()
            .all(|run| run.task_card_id != denied_task.id),
        "permission denial should block tool run creation"
    );

    let audit = service
        .storage
        .list_audit_events(None)
        .expect("audit events")
        .into_iter()
        .find(|event| {
            event.entity_id == denied_task.id && event.event_type == "tool_run.permission_denied"
        })
        .expect("permission denial audit");
    assert!(audit.payload_json.contains("allowFsRoots"));
}

#[test]
fn memory_policy_injects_snapshot_and_respects_write_scope() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let agent = service
        .create_agent_profile(CreateAgentInput {
            name: "Memory Scout".into(),
            avatar: "MS".into(),
            role: "Research Lead".into(),
            objective: "Use memory carefully.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.research".into()],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 1,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy {
                read_scope: vec!["agent".into()],
                write_scope: vec!["work_group".into()],
                pinned_memory_ids: vec!["wg-pin".into()],
            },
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("agent");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Memory Group".into(),
            goal: "Verify memory policies.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &agent.id)
        .expect("add agent");

    service
        .storage
        .insert_memory_item(&MemoryItem {
            id: "wg-pin".into(),
            scope: MemoryScope::WorkGroup,
            scope_id: work_group.id.clone(),
            content: "Shared release constraint".into(),
            tags: vec!["shared".into()],
            embedding_ref: None,
            pinned: true,
            ttl: None,
            created_at: now(),
        })
        .expect("work group memory");
    service
        .storage
        .insert_memory_item(&MemoryItem {
            id: new_id(),
            scope: MemoryScope::Agent,
            scope_id: agent.id.clone(),
            content: "Agent-only context".into(),
            tags: vec!["agent".into()],
            embedding_ref: None,
            pinned: false,
            ttl: None,
            created_at: now(),
        })
        .expect("agent memory");

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "Summarize the latest context and next step.".into(),
            },
        )
        .expect("send message");

    let task = (0..80)
        .find_map(|_| {
            service
                .storage
                .list_task_cards(Some(&work_group.id))
                .expect("tasks")
                .into_iter()
                .find(|task| task.created_by == "human" && task.status == TaskStatus::Completed)
                .or_else(|| {
                    thread::sleep(Duration::from_millis(50));
                    None
                })
        })
        .expect("completed task");

    let persisted = service.storage.list_memory_items().expect("memory");
    let task_memory = persisted
        .iter()
        .find(|item| item.scope == MemoryScope::Task && item.scope_id == task.id)
        .expect("task snapshot");
    assert!(task_memory.content.contains("Shared release constraint"));
    assert!(task_memory.content.contains("Agent-only context"));
    assert!(
        persisted.iter().any(|item| {
            item.scope == MemoryScope::WorkGroup
                && item.scope_id == work_group.id
                && item.tags.iter().any(|tag| tag == "summary")
        }),
        "expected work group summary memory"
    );
    assert!(
        persisted.iter().all(|item| {
            !(item.scope == MemoryScope::Agent
                && item.scope_id == agent.id
                && item.tags.iter().any(|tag| tag == "summary"))
        }),
        "agent scope should not receive summary memory when write_scope excludes it"
    );
}

#[test]
fn parent_task_waits_for_all_children_before_completion() {
    let (service, _, _) = setup_service();
    let work_group = service
        .storage
        .list_work_groups()
        .expect("groups")
        .into_iter()
        .next()
        .expect("seeded group");
    let agent = service
        .storage
        .list_agents()
        .expect("agents")
        .into_iter()
        .next()
        .expect("seeded agent");

    let parent = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Parent".into(),
        normalized_goal: "Coordinate multiple child tasks.".into(),
        input_payload: "coordinate".into(),
        priority: 80,
        status: TaskStatus::WaitingChildren,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(agent.id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&parent).expect("parent");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: parent.id.clone(),
            owner_agent_id: agent.id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");

    for status in [TaskStatus::Completed, TaskStatus::InProgress] {
        service
            .storage
            .insert_task_card(&TaskCard {
                id: new_id(),
                parent_id: Some(parent.id.clone()),
                source_message_id: parent.source_message_id.clone(),
                title: format!("Child {status:?}"),
                normalized_goal: "child".into(),
                input_payload: "child".into(),
                priority: 40,
                status,
                work_group_id: work_group.id.clone(),
                created_by: agent.id.clone(),
                assigned_agent_id: Some(agent.id.clone()),
                created_at: now(),
            })
            .expect("child");
    }

    assert!(
        !service
            .reconcile_parent_task_state(&parent.id)
            .expect("first reconciliation"),
        "parent should keep waiting while one child is still active"
    );
    assert_eq!(
        service
            .storage
            .get_task_card(&parent.id)
            .expect("parent")
            .status,
        TaskStatus::WaitingChildren
    );

    let mut active_child = service
        .storage
        .list_child_tasks(&parent.id)
        .expect("children")
        .into_iter()
        .find(|child| child.status == TaskStatus::InProgress)
        .expect("active child");
    active_child.status = TaskStatus::Completed;
    service
        .storage
        .update_task_card(&active_child)
        .expect("update child");

    assert!(
        service
            .reconcile_parent_task_state(&parent.id)
            .expect("second reconciliation"),
        "parent should complete after all children are terminal"
    );
    assert_eq!(
        service
            .storage
            .get_task_card(&parent.id)
            .expect("parent")
            .status,
        TaskStatus::Completed
    );
}

#[test]
fn child_issue_bubbles_parent_to_review() {
    let (service, _, _) = setup_service();
    let work_group = service
        .storage
        .list_work_groups()
        .expect("groups")
        .into_iter()
        .next()
        .expect("seeded group");
    let agent = service
        .storage
        .list_agents()
        .expect("agents")
        .into_iter()
        .next()
        .expect("seeded agent");

    let parent = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Parent Review".into(),
        normalized_goal: "Coordinate multiple child tasks.".into(),
        input_payload: "coordinate".into(),
        priority: 80,
        status: TaskStatus::WaitingChildren,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(agent.id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&parent).expect("parent");

    for status in [TaskStatus::Completed, TaskStatus::NeedsReview] {
        service
            .storage
            .insert_task_card(&TaskCard {
                id: new_id(),
                parent_id: Some(parent.id.clone()),
                source_message_id: parent.source_message_id.clone(),
                title: format!("Child {status:?}"),
                normalized_goal: "child".into(),
                input_payload: "child".into(),
                priority: 40,
                status,
                work_group_id: work_group.id.clone(),
                created_by: agent.id.clone(),
                assigned_agent_id: Some(agent.id.clone()),
                created_at: now(),
            })
            .expect("child");
    }

    assert!(
        service
            .reconcile_parent_task_state(&parent.id)
            .expect("reconciliation"),
        "parent should reconcile when all children are terminal"
    );
    assert_eq!(
        service
            .storage
            .get_task_card(&parent.id)
            .expect("parent")
            .status,
        TaskStatus::NeedsReview
    );
}

#[test]
fn child_task_emits_collaboration_request_and_result_messages() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let requester = service
        .create_agent_profile(CreateAgentInput {
            name: "Lead".into(),
            avatar: "LD".into(),
            role: "Planning Lead".into(),
            objective: "Coordinate multi-agent work.".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec!["skill.builder".into()],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: true,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("requester");
    let collaborator = service
        .create_agent_profile(CreateAgentInput {
            name: "Reviewer".into(),
            avatar: "RV".into(),
            role: "Quality Reviewer".into(),
            objective: "Return concise review results.".into(),
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
        .expect("collaborator");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Collaboration Group".into(),
            goal: "Trace explicit agent collaboration.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &requester.id)
        .expect("add requester");
    service
        .add_agent_to_work_group(&work_group.id, &collaborator.id)
        .expect("add collaborator");

    let parent = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Coordinate release checklist".into(),
        normalized_goal: "Coordinate release checklist".into(),
        input_payload: "Coordinate release checklist".into(),
        priority: 90,
        status: TaskStatus::WaitingChildren,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(requester.id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&parent).expect("parent");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: parent.id.clone(),
            owner_agent_id: requester.id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("parent lease");

    let child = service
        .spawn_subtask(
            &app_handle,
            &parent,
            &requester,
            "@Reviewer audit launch checklist and return blockers.",
        )
        .expect("spawn child")
        .expect("child task");

    let final_child = (0..80)
        .find_map(|_| {
            let task = service
                .storage
                .get_task_card(&child.id)
                .expect("child task");
            if matches!(task.status, TaskStatus::Completed | TaskStatus::NeedsReview) {
                Some(task)
            } else {
                thread::sleep(Duration::from_millis(50));
                None
            }
        })
        .expect("child completion");
    assert_eq!(final_child.status, TaskStatus::Completed);

    let collaboration_messages = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages")
        .into_iter()
        .filter(|message| {
            message.task_card_id.as_deref() == Some(child.id.as_str())
                && message.kind == MessageKind::Collaboration
        })
        .collect::<Vec<_>>();

    let request = collaboration_messages
        .iter()
        .find(|message| message.sender_id == requester.id)
        .expect("request message");
    assert!(request.content.contains("Collaboration request"));
    assert!(request.content.contains(&parent.title));
    assert!(request.content.contains(&child.title));
    assert!(request.content.contains(&collaborator.name));
    assert_eq!(request.mentions, vec![collaborator.id.clone()]);

    let result = collaboration_messages
        .iter()
        .find(|message| message.sender_id == collaborator.id)
        .expect("result message");
    assert!(result.content.contains("Collaboration result"));
    assert!(result.content.contains(&parent.title));
    assert!(result.content.contains(&child.title));
    assert!(result.content.contains("Status: completed"));
    assert_eq!(result.mentions, vec![requester.id.clone()]);

    let audits = service
        .storage
        .list_audit_events(None)
        .expect("audits")
        .into_iter()
        .filter(|event| {
            event.entity_id == child.id
                && matches!(
                    event.event_type.as_str(),
                    "task.collaboration_requested" | "task.collaboration_reported"
                )
        })
        .count();
    assert_eq!(audits, 2);
}

#[test]
fn startup_recovery_pauses_low_risk_inflight_work() {
    let (service, workspace_root, data_root) = setup_service();
    let work_group = service
        .storage
        .list_work_groups()
        .expect("groups")
        .into_iter()
        .next()
        .expect("seeded group");
    let agent = service
        .storage
        .list_agents()
        .expect("agents")
        .into_iter()
        .next()
        .expect("seeded agent");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Recover queued search".into(),
        normalized_goal: "Search the workspace after restart.".into(),
        input_payload: "search the workspace".into(),
        priority: 10,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(agent.id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: agent.id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");
    service
        .storage
        .insert_tool_run(&ToolRun {
            id: new_id(),
            tool_id: "project.search".into(),
            task_card_id: task.id.clone(),
            agent_id: agent.id.clone(),
            state: ToolRunState::Running,
            approval_required: false,
            started_at: Some(now()),
            finished_at: None,
            result_ref: None,
        })
        .expect("tool run");

    let recovered = AppService::new(workspace_root, data_root).expect("recovered service");

    let recovered_task = recovered.storage.get_task_card(&task.id).expect("task");
    assert_eq!(recovered_task.status, TaskStatus::Paused);

    let recovered_lease = recovered
        .storage
        .get_lease_by_task(&task.id)
        .expect("lease")
        .expect("lease exists");
    assert_eq!(recovered_lease.state, LeaseState::Paused);

    let recovered_run = recovered
        .storage
        .list_tool_runs()
        .expect("runs")
        .into_iter()
        .find(|run| run.task_card_id == task.id)
        .expect("run");
    assert_eq!(recovered_run.state, ToolRunState::Queued);
    assert!(recovered_run.started_at.is_none());
}

#[test]
fn startup_recovery_marks_high_risk_inflight_work_for_review() {
    let (service, workspace_root, data_root) = setup_service();
    let work_group = service
        .storage
        .list_work_groups()
        .expect("groups")
        .into_iter()
        .next()
        .expect("seeded group");
    let agent = service
        .storage
        .list_agents()
        .expect("agents")
        .into_iter()
        .find(|item| item.tool_ids.iter().any(|tool| tool == "shell.exec"))
        .expect("agent with shell");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Recover shell execution".into(),
        normalized_goal: "Run a shell command after approval.".into(),
        input_payload: "run shell command".into(),
        priority: 10,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(agent.id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: agent.id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");
    service
        .storage
        .insert_tool_run(&ToolRun {
            id: new_id(),
            tool_id: "shell.exec".into(),
            task_card_id: task.id.clone(),
            agent_id: agent.id.clone(),
            state: ToolRunState::Running,
            approval_required: true,
            started_at: Some(now()),
            finished_at: None,
            result_ref: None,
        })
        .expect("tool run");

    let recovered = AppService::new(workspace_root, data_root).expect("recovered service");

    let recovered_task = recovered.storage.get_task_card(&task.id).expect("task");
    assert_eq!(recovered_task.status, TaskStatus::NeedsReview);

    let recovered_lease = recovered
        .storage
        .get_lease_by_task(&task.id)
        .expect("lease")
        .expect("lease exists");
    assert_eq!(recovered_lease.state, LeaseState::Released);
    assert!(recovered_lease.released_at.is_some());

    let recovered_run = recovered
        .storage
        .list_tool_runs()
        .expect("runs")
        .into_iter()
        .find(|run| run.task_card_id == task.id)
        .expect("run");
    assert_eq!(recovered_run.state, ToolRunState::Cancelled);
    assert!(recovered_run.finished_at.is_some());
}

#[test]
fn execution_failure_moves_task_to_review_and_releases_lease() {
    let (service, _, _) = setup_service();
    let work_group = service
        .storage
        .list_work_groups()
        .expect("groups")
        .into_iter()
        .next()
        .expect("seeded group");
    let agent = service
        .storage
        .list_agents()
        .expect("agents")
        .into_iter()
        .next()
        .expect("seeded agent");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Failing task".into(),
        normalized_goal: "Simulate execution failure.".into(),
        input_payload: "simulate failure".into(),
        priority: 10,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(agent.id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: agent.id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");
    service
        .storage
        .insert_tool_run(&ToolRun {
            id: new_id(),
            tool_id: "project.search".into(),
            task_card_id: task.id.clone(),
            agent_id: agent.id.clone(),
            state: ToolRunState::Running,
            approval_required: false,
            started_at: Some(now()),
            finished_at: None,
            result_ref: None,
        })
        .expect("tool run");

    let report = service
        .handle_task_execution_failure(&task.id, &anyhow!("boom"))
        .expect("failure report");

    assert_eq!(report.task.expect("task").status, TaskStatus::NeedsReview);
    assert_eq!(report.cancelled_tool_runs.len(), 1);

    let lease = service
        .storage
        .get_lease_by_task(&task.id)
        .expect("lease")
        .expect("lease exists");
    assert_eq!(lease.state, LeaseState::Released);
    assert!(lease.released_at.is_some());

    let tool_run = service
        .storage
        .list_tool_runs()
        .expect("runs")
        .into_iter()
        .find(|run| run.task_card_id == task.id)
        .expect("tool run");
    assert_eq!(tool_run.state, ToolRunState::Cancelled);
    assert!(tool_run.finished_at.is_some());
}
