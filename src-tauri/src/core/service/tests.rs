use super::AppService;
use crate::core::domain::{
    new_id, now, AgentPermissionPolicy, CreateAgentInput, CreateWorkGroupInput, Lease, LeaseState,
    MemoryItem, MemoryPolicy, MemoryScope, MessageKind, SendHumanMessageInput, TaskCard,
    TaskStatus, ToolRun, ToolRunState, WorkGroupKind,
};
use crate::core::workflow::{
    BlockerCategory, BlockerResolutionTarget, NarrativeEnvelope, RaiseTaskBlockerInput,
    RequestRouteMode, StageStatus, TaskDispatchRecord, TaskDispatchSource,
    WorkflowCheckpointStatus, WorkflowExecutionMode, WorkflowRecord, WorkflowStageRecord,
    WorkflowStatus,
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

fn create_agent(service: &AppService, name: &str, role: &str) -> String {
    service
        .create_agent_profile(CreateAgentInput {
            name: name.into(),
            avatar: name.chars().take(2).collect(),
            role: role.into(),
            objective: role.into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec!["plan.summarize".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("agent")
        .id
}

fn parse_narrative(content: &str) -> Option<NarrativeEnvelope> {
    serde_json::from_str::<NarrativeEnvelope>(content).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn block_on_service_future_is_safe_inside_runtime() {
    let value = super::block_on_service_future(async { 7usize });
    assert_eq!(value, 7);
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
            skill_ids: vec![],
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
            skill_ids: vec![],
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
            skill_ids: vec![],
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
fn direct_assign_route_skips_owner_plan_messages() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let frontend_id = create_agent(&service, "前端开发1", "Frontend Engineer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Direct Group".into(),
            goal: "Verify direct assignment.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &frontend_id)
        .expect("add frontend");

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "@前端开发1 做一个登录页".into(),
            },
        )
        .expect("send");

    let messages = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages");
    assert!(
        messages.iter().any(|message| {
            parse_narrative(&message.content).is_some_and(|item| {
                item.narrative_type == crate::core::workflow::NarrativeMessageType::AgentAck
            })
        }),
        "expected direct agent ack narrative",
    );
    assert!(
        !messages.iter().any(|message| {
            parse_narrative(&message.content).is_some_and(|item| {
                matches!(
                    item.narrative_type,
                    crate::core::workflow::NarrativeMessageType::OwnerAck
                        | crate::core::workflow::NarrativeMessageType::OwnerPlan
                )
            })
        }),
        "direct assignment should not emit owner plan messages",
    );
    assert!(
        messages.iter().any(|message| {
            matches!(
                message.visibility,
                crate::core::domain::Visibility::Backstage
            ) && parse_narrative(&message.content).is_some_and(|item| {
                item.narrative_type == crate::core::workflow::NarrativeMessageType::DirectAssign
            })
        }),
        "expected backstage direct assign narrative",
    );
    let has_progress = (0..40).any(|_| {
        let messages = service
            .storage
            .list_messages_for_group(&work_group.id)
            .expect("messages");
        if messages.iter().any(|message| {
            matches!(message.visibility, crate::core::domain::Visibility::Main)
                && parse_narrative(&message.content).is_some_and(|item| {
                    item.narrative_type
                        == crate::core::workflow::NarrativeMessageType::AgentProgress
                })
        }) {
            true
        } else {
            thread::sleep(Duration::from_millis(25));
            false
        }
    });
    assert!(
        has_progress,
        "expected direct assignment progress narrative"
    );
    let direct_ack = messages
        .iter()
        .filter_map(|message| parse_narrative(&message.content))
        .find(|item| item.narrative_type == crate::core::workflow::NarrativeMessageType::AgentAck)
        .expect("direct ack narrative");
    assert!(
        direct_ack.text.contains("关键问题开始推进"),
        "expected llm-generated direct ack text"
    );
    let direct_progress = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages")
        .into_iter()
        .filter_map(|message| parse_narrative(&message.content))
        .find(|item| {
            item.narrative_type == crate::core::workflow::NarrativeMessageType::AgentProgress
        })
        .expect("direct progress narrative");
    assert_eq!(direct_progress.progress_percent, Some(35));
    assert!(
        direct_progress.text.contains("推进关键部分"),
        "expected llm-generated progress text"
    );
}

#[test]
fn owner_orchestrated_route_emits_owner_plan() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let product_id = create_agent(&service, "产品经理", "Product Manager");
    let architect_id = create_agent(&service, "架构师", "Architect");
    let backend_id = create_agent(&service, "后端开发1", "Backend Engineer");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Project Group".into(),
            goal: "Verify owner orchestration.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    for agent_id in [product_id, architect_id, backend_id] {
        service
            .add_agent_to_work_group(&work_group.id, &agent_id)
            .expect("add member");
    }

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "帮我开发一个图书管理系统".into(),
            },
        )
        .expect("send");

    let messages = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages");
    assert!(
        messages.iter().any(|message| {
            parse_narrative(&message.content).is_some_and(|item| {
                item.narrative_type == crate::core::workflow::NarrativeMessageType::OwnerAck
            })
        }),
        "expected owner ack narrative",
    );
    assert!(
        messages.iter().any(|message| {
            parse_narrative(&message.content).is_some_and(|item| {
                item.narrative_type == crate::core::workflow::NarrativeMessageType::OwnerPlan
            })
        }),
        "expected owner plan narrative",
    );
}

#[test]
fn single_task_owner_route_emits_narrative_dispatch_in_main_chat() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let auditor_id = create_agent(&service, "CodeAuditor", "Security Code Reviewer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Audit Group".into(),
            goal: "Verify single-task owner narration.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &auditor_id)
        .expect("add auditor");

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "审计一下当前项目".into(),
            },
        )
        .expect("send");

    let main_messages = service
        .storage
        .list_messages_for_group(&work_group.id)
        .expect("messages")
        .into_iter()
        .filter(|message| matches!(message.visibility, crate::core::domain::Visibility::Main))
        .collect::<Vec<_>>();

    assert!(
        main_messages.iter().any(|message| {
            parse_narrative(&message.content).is_some_and(|item| {
                item.narrative_type == crate::core::workflow::NarrativeMessageType::OwnerAck
            })
        }),
        "expected owner ack narrative in main chat",
    );
    assert!(
        main_messages.iter().any(|message| {
            parse_narrative(&message.content).is_some_and(|item| {
                item.narrative_type == crate::core::workflow::NarrativeMessageType::OwnerDispatch
            })
        }),
        "expected owner dispatch narrative in main chat",
    );
    assert!(
        main_messages
            .iter()
            .all(|message| { !message.content.contains("Task card created and leased to") }),
        "legacy coordinator status should not stay in main chat",
    );
    let agent_ack = main_messages
        .iter()
        .filter_map(|message| parse_narrative(&message.content))
        .find(|item| item.narrative_type == crate::core::workflow::NarrativeMessageType::AgentAck)
        .expect("owner route agent ack");
    assert!(
        agent_ack.text.contains("关键点梳理清楚"),
        "expected llm-generated owner-route agent ack text"
    );
}

#[test]
fn owner_blocker_resolution_resumes_blocked_task() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let worker_id = create_agent(&service, "后端开发2", "Backend Engineer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Blocker Group".into(),
            goal: "Verify owner blocker resolution.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &worker_id)
        .expect("add worker");

    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");

    let workflow = WorkflowRecord {
        id: new_id(),
        work_group_id: work_group.id.clone(),
        source_message_id: new_id(),
        route_mode: RequestRouteMode::OwnerOrchestrated,
        title: "借阅系统".into(),
        normalized_intent: "借阅系统".into(),
        status: WorkflowStatus::Running,
        owner_agent_id: owner_id,
        current_stage_id: Some("stage-1".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_workflow(&workflow)
        .expect("workflow");
    service
        .storage
        .insert_workflow_stage(&WorkflowStageRecord {
            id: "stage-1".into(),
            workflow_id: workflow.id.clone(),
            title: "实施开发".into(),
            goal: "实现借阅模块".into(),
            order_index: 1,
            execution_mode: WorkflowExecutionMode::Serial,
            status: StageStatus::Running,
            entry_message_id: None,
            completion_message_id: None,
            created_at: now(),
        })
        .expect("stage");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: workflow.source_message_id.clone(),
        title: "实现借阅状态流转".into(),
        normalized_goal: "实现借阅状态流转".into(),
        input_payload: "实现借阅状态流转".into(),
        priority: 80,
        status: TaskStatus::Leased,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(worker_id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_task_dispatch(&TaskDispatchRecord {
            task_id: task.id.clone(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: Some("stage-1".into()),
            dispatch_source: TaskDispatchSource::OwnerAssign,
            depends_on_task_ids: vec![],
            acknowledged_at: Some(now()),
            result_message_id: None,
            locked_by_user_mention: false,
            target_agent_id: worker_id.clone(),
            route_mode: RequestRouteMode::OwnerOrchestrated,
            narrative_stage_label: Some("实施开发".into()),
            narrative_task_label: Some(task.title.clone()),
        })
        .expect("dispatch");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: worker_id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");

    let blocker = service
        .raise_task_blocker(
            app_handle,
            &task.id,
            RaiseTaskBlockerInput {
                raised_by_agent_id: worker_id.clone(),
                resolution_target: BlockerResolutionTarget::Owner,
                category: BlockerCategory::MissingDependency,
                summary: "缺少借阅状态流转规则".into(),
                details: "需要先补接口与状态约束".into(),
            },
        )
        .expect("raise blocker");

    let resolved = service
        .storage
        .get_task_blocker(&blocker.id)
        .expect("blocker");
    assert_eq!(
        resolved.status,
        crate::core::workflow::BlockerStatus::Resolved
    );
    assert_ne!(
        service
            .storage
            .get_workflow(&workflow.id)
            .expect("workflow")
            .status,
        WorkflowStatus::Blocked
    );
    assert!(
        service
            .storage
            .list_messages_for_group(&work_group.id)
            .expect("messages")
            .into_iter()
            .any(|message| {
                parse_narrative(&message.content).is_some_and(|item| {
                    item.narrative_type
                        == crate::core::workflow::NarrativeMessageType::BlockerResolved
                })
            }),
        "expected blocker resolved narrative",
    );
}

#[test]
fn owner_blocker_can_escalate_to_user_and_resume_after_answer() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let worker_id = create_agent(&service, "前端开发1", "Frontend Engineer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "User Escalation Group".into(),
            goal: "Verify owner escalates blockers to user.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &worker_id)
        .expect("add worker");

    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");

    let workflow = WorkflowRecord {
        id: new_id(),
        work_group_id: work_group.id.clone(),
        source_message_id: new_id(),
        route_mode: RequestRouteMode::OwnerOrchestrated,
        title: "登录页项目".into(),
        normalized_intent: "登录页项目".into(),
        status: WorkflowStatus::Running,
        owner_agent_id: owner_id,
        current_stage_id: Some("stage-1".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_workflow(&workflow)
        .expect("workflow");
    service
        .storage
        .insert_workflow_stage(&WorkflowStageRecord {
            id: "stage-1".into(),
            workflow_id: workflow.id.clone(),
            title: "实施开发".into(),
            goal: "开发登录页".into(),
            order_index: 1,
            execution_mode: WorkflowExecutionMode::Serial,
            status: StageStatus::Running,
            entry_message_id: None,
            completion_message_id: None,
            created_at: now(),
        })
        .expect("stage");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: workflow.source_message_id.clone(),
        title: "开发登录页".into(),
        normalized_goal: "开发登录页".into(),
        input_payload: "开发登录页".into(),
        priority: 80,
        status: TaskStatus::Leased,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(worker_id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_task_dispatch(&TaskDispatchRecord {
            task_id: task.id.clone(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: Some("stage-1".into()),
            dispatch_source: TaskDispatchSource::OwnerAssign,
            depends_on_task_ids: vec![],
            acknowledged_at: Some(now()),
            result_message_id: None,
            locked_by_user_mention: false,
            target_agent_id: worker_id.clone(),
            route_mode: RequestRouteMode::OwnerOrchestrated,
            narrative_stage_label: Some("实施开发".into()),
            narrative_task_label: Some(task.title.clone()),
        })
        .expect("dispatch");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: worker_id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");

    service
        .raise_task_blocker(
            app_handle.clone(),
            &task.id,
            RaiseTaskBlockerInput {
                raised_by_agent_id: worker_id.clone(),
                resolution_target: BlockerResolutionTarget::Owner,
                category: BlockerCategory::NeedUserDecision,
                summary: "缺少页面风格决策".into(),
                details: "需要确认是管理后台风格还是读者端风格".into(),
            },
        )
        .expect("raise blocker");

    assert_eq!(
        service
            .storage
            .get_task_card(&task.id)
            .expect("task")
            .status,
        TaskStatus::WaitingUserInput
    );
    assert_eq!(
        service
            .storage
            .get_workflow(&workflow.id)
            .expect("workflow")
            .status,
        WorkflowStatus::NeedsUserInput
    );
    let pending = service
        .storage
        .latest_pending_user_question_for_group(&work_group.id)
        .expect("pending query")
        .expect("pending question");
    assert!(pending.question.contains("登录页要采用"));

    service
        .send_human_message(
            app_handle,
            SendHumanMessageInput {
                work_group_id: work_group.id.clone(),
                content: "采用管理后台风格".into(),
            },
        )
        .expect("answer question");

    let final_task = (0..40)
        .find_map(|_| {
            let current = service.storage.get_task_card(&task.id).expect("task");
            if !matches!(
                current.status,
                TaskStatus::WaitingUserInput | TaskStatus::Paused
            ) {
                Some(current)
            } else {
                thread::sleep(Duration::from_millis(25));
                None
            }
        })
        .expect("task resumed");
    assert_ne!(final_task.status, TaskStatus::WaitingUserInput);
    assert_ne!(
        service
            .storage
            .get_workflow(&workflow.id)
            .expect("workflow")
            .status,
        WorkflowStatus::NeedsUserInput
    );
}

#[test]
fn owner_blocker_can_create_dependency_task() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let worker_id = create_agent(&service, "后端开发2", "Backend Engineer");
    let architect_id = create_agent(&service, "架构师", "Architect");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Dependency Group".into(),
            goal: "Verify dependency task creation.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &worker_id)
        .expect("add worker");
    service
        .add_agent_to_work_group(&work_group.id, &architect_id)
        .expect("add architect");

    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");

    let workflow = WorkflowRecord {
        id: new_id(),
        work_group_id: work_group.id.clone(),
        source_message_id: new_id(),
        route_mode: RequestRouteMode::OwnerOrchestrated,
        title: "图书系统".into(),
        normalized_intent: "图书系统".into(),
        status: WorkflowStatus::Running,
        owner_agent_id: owner_id,
        current_stage_id: Some("stage-1".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_workflow(&workflow)
        .expect("workflow");
    service
        .storage
        .insert_workflow_stage(&WorkflowStageRecord {
            id: "stage-1".into(),
            workflow_id: workflow.id.clone(),
            title: "实施开发".into(),
            goal: "实现借阅逻辑".into(),
            order_index: 1,
            execution_mode: WorkflowExecutionMode::Serial,
            status: StageStatus::Running,
            entry_message_id: None,
            completion_message_id: None,
            created_at: now(),
        })
        .expect("stage");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: workflow.source_message_id.clone(),
        title: "实现借阅模块".into(),
        normalized_goal: "实现借阅模块".into(),
        input_payload: "实现借阅模块".into(),
        priority: 80,
        status: TaskStatus::Leased,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(worker_id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_task_dispatch(&TaskDispatchRecord {
            task_id: task.id.clone(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: Some("stage-1".into()),
            dispatch_source: TaskDispatchSource::OwnerAssign,
            depends_on_task_ids: vec![],
            acknowledged_at: Some(now()),
            result_message_id: None,
            locked_by_user_mention: false,
            target_agent_id: worker_id.clone(),
            route_mode: RequestRouteMode::OwnerOrchestrated,
            narrative_stage_label: Some("实施开发".into()),
            narrative_task_label: Some(task.title.clone()),
        })
        .expect("dispatch");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: worker_id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");

    service
        .raise_task_blocker(
            app_handle.clone(),
            &task.id,
            RaiseTaskBlockerInput {
                raised_by_agent_id: worker_id.clone(),
                resolution_target: BlockerResolutionTarget::Owner,
                category: BlockerCategory::MissingDependency,
                summary: "缺少架构层借阅状态规则".into(),
                details: "需要先补接口与状态约束".into(),
            },
        )
        .expect("raise blocker");

    let dependency = service
        .storage
        .list_task_cards(Some(&work_group.id))
        .expect("tasks")
        .into_iter()
        .find(|item| item.id != task.id)
        .expect("dependency task");
    assert_eq!(
        dependency.assigned_agent_id.as_deref(),
        Some(architect_id.as_str())
    );
    assert_eq!(
        service
            .storage
            .get_task_card(&task.id)
            .expect("task")
            .status,
        TaskStatus::Pending
    );
    let original_dispatch = service
        .storage
        .get_task_dispatch(&task.id)
        .expect("dispatch query")
        .expect("dispatch");
    assert!(original_dispatch
        .depends_on_task_ids
        .contains(&dependency.id));
}

#[test]
fn owner_blocker_can_request_approval() {
    let (service, _, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let worker_id = create_agent(&service, "Builder", "Systems Engineer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Approval Group".into(),
            goal: "Verify owner approval escalation.".into(),
            working_directory: ".".into(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &worker_id)
        .expect("add worker");

    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");

    let workflow = WorkflowRecord {
        id: new_id(),
        work_group_id: work_group.id.clone(),
        source_message_id: new_id(),
        route_mode: RequestRouteMode::OwnerOrchestrated,
        title: "部署审批".into(),
        normalized_intent: "部署审批".into(),
        status: WorkflowStatus::Running,
        owner_agent_id: owner_id,
        current_stage_id: Some("stage-1".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_workflow(&workflow)
        .expect("workflow");
    service
        .storage
        .insert_workflow_stage(&WorkflowStageRecord {
            id: "stage-1".into(),
            workflow_id: workflow.id.clone(),
            title: "发布收尾".into(),
            goal: "执行发布".into(),
            order_index: 1,
            execution_mode: WorkflowExecutionMode::Serial,
            status: StageStatus::Running,
            entry_message_id: None,
            completion_message_id: None,
            created_at: now(),
        })
        .expect("stage");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: workflow.source_message_id.clone(),
        title: "执行生产发布".into(),
        normalized_goal: "执行生产发布".into(),
        input_payload: "执行生产发布".into(),
        priority: 80,
        status: TaskStatus::Leased,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(worker_id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_task_dispatch(&TaskDispatchRecord {
            task_id: task.id.clone(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: Some("stage-1".into()),
            dispatch_source: TaskDispatchSource::OwnerAssign,
            depends_on_task_ids: vec![],
            acknowledged_at: Some(now()),
            result_message_id: None,
            locked_by_user_mention: false,
            target_agent_id: worker_id.clone(),
            route_mode: RequestRouteMode::OwnerOrchestrated,
            narrative_stage_label: Some("发布收尾".into()),
            narrative_task_label: Some(task.title.clone()),
        })
        .expect("dispatch");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: worker_id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");

    service
        .raise_task_blocker(
            app_handle.clone(),
            &task.id,
            RaiseTaskBlockerInput {
                raised_by_agent_id: worker_id,
                resolution_target: BlockerResolutionTarget::Owner,
                category: BlockerCategory::PermissionRequired,
                summary: "生产发布需要额外审批".into(),
                details: "请确认是否批准当前发布时间窗".into(),
            },
        )
        .expect("raise blocker");

    assert_eq!(
        service
            .storage
            .get_task_card(&task.id)
            .expect("task")
            .status,
        TaskStatus::WaitingApproval
    );
    let pending = service
        .storage
        .latest_pending_user_question_for_group(&work_group.id)
        .expect("pending query")
        .expect("pending approval");
    assert!(pending.question.contains("是否批准"));
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
            skill_ids: vec![],
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
            skill_ids: vec![],
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
            skill_ids: vec![],
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

#[test]
fn task_checkpoint_persists_repo_snapshot_and_resume_hint() {
    let (service, workspace_root, _) = setup_service();
    let worker_id = create_agent(&service, "执行成员", "Execution Engineer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Checkpoint Group".into(),
            goal: "Verify checkpoint persistence.".into(),
            working_directory: workspace_root.to_string_lossy().into_owned(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &worker_id)
        .expect("add worker");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Persist checkpoint".into(),
        normalized_goal: "Persist checkpoint for retry.".into(),
        input_payload: "Persist checkpoint for retry.".into(),
        priority: 10,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(worker_id),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");

    service
        .record_task_checkpoint(
            &task,
            WorkflowCheckpointStatus::TaskRunning,
            1,
            Some("resume from checkpoint".into()),
            Some("boom".into()),
        )
        .expect("checkpoint");

    let checkpoint = service
        .storage
        .latest_workflow_checkpoint_for_task(&task.id)
        .expect("checkpoint query")
        .expect("checkpoint exists");
    assert_eq!(checkpoint.task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(checkpoint.status, WorkflowCheckpointStatus::TaskRunning);
    assert_eq!(checkpoint.failure_count, 1);
    assert_eq!(
        checkpoint.resume_hint.as_deref(),
        Some("resume from checkpoint")
    );
    assert_eq!(checkpoint.last_error.as_deref(), Some("boom"));
    assert!(checkpoint.repo_snapshot.is_empty);
}

#[test]
fn retryable_failure_reassigns_greenfield_architecture_task_to_execution_agent() {
    let (service, workspace_root, _) = setup_service();
    let app = tauri::test::mock_app();
    let app_handle = app.handle().clone();

    let architect_id = create_agent(&service, "架构师1", "系统架构师");
    let fullstack = service
        .create_agent_profile(CreateAgentInput {
            name: "全栈开发专家".into(),
            avatar: "FS".into(),
            role: "全栈工程师".into(),
            objective: "直接实现前端项目并交付最小可运行版本。".into(),
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
            skill_ids: vec![],
            tool_ids: vec!["Write".into(), "Edit".into(), "Bash".into()],
            max_parallel_runs: 2,
            can_spawn_subtasks: false,
            memory_policy: MemoryPolicy::default(),
            permission_policy: AgentPermissionPolicy::default(),
        })
        .expect("fullstack");

    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Retry Group".into(),
            goal: "Verify retryable failures can degrade into execution handoff.".into(),
            working_directory: workspace_root.to_string_lossy().into_owned(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &architect_id)
        .expect("add architect");
    service
        .add_agent_to_work_group(&work_group.id, &fullstack.id)
        .expect("add fullstack");

    let owner_id = service
        .storage
        .get_work_group_owner_id(&work_group.id)
        .expect("owner query")
        .expect("owner");

    let workflow = WorkflowRecord {
        id: new_id(),
        work_group_id: work_group.id.clone(),
        source_message_id: new_id(),
        route_mode: RequestRouteMode::OwnerOrchestrated,
        title: "创新贪吃蛇".into(),
        normalized_intent: "开发一个不一样的贪吃蛇游戏，仅前端即可，不需要后端".into(),
        status: WorkflowStatus::Running,
        owner_agent_id: owner_id,
        current_stage_id: Some("stage-architecture".into()),
        created_at: now(),
    };
    service
        .storage
        .insert_workflow(&workflow)
        .expect("workflow");
    service
        .storage
        .insert_workflow_stage(&WorkflowStageRecord {
            id: "stage-architecture".into(),
            workflow_id: workflow.id.clone(),
            title: "技术方案与架构设计".into(),
            goal: "确定技术栈和核心模块".into(),
            order_index: 1,
            execution_mode: WorkflowExecutionMode::Serial,
            status: StageStatus::Running,
            entry_message_id: None,
            completion_message_id: None,
            created_at: now(),
        })
        .expect("stage");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: workflow.source_message_id.clone(),
        title: "设计前端技术架构与模块划分".into(),
        normalized_goal: "主导设计游戏的整体前端架构，包括技术选型和模块划分".into(),
        input_payload: "主导设计游戏的整体前端架构，包括技术选型和模块划分".into(),
        priority: 80,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id.clone(),
        created_by: "human".into(),
        assigned_agent_id: Some(architect_id.clone()),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .storage
        .insert_task_dispatch(&TaskDispatchRecord {
            task_id: task.id.clone(),
            workflow_id: Some(workflow.id.clone()),
            stage_id: Some("stage-architecture".into()),
            dispatch_source: TaskDispatchSource::OwnerAssign,
            depends_on_task_ids: vec![],
            acknowledged_at: Some(now()),
            result_message_id: None,
            locked_by_user_mention: false,
            target_agent_id: architect_id.clone(),
            route_mode: RequestRouteMode::OwnerOrchestrated,
            narrative_stage_label: Some("技术方案与架构设计".into()),
            narrative_task_label: Some(task.title.clone()),
        })
        .expect("dispatch");
    service
        .storage
        .insert_lease(&Lease {
            id: new_id(),
            task_card_id: task.id.clone(),
            owner_agent_id: architect_id.clone(),
            state: LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })
        .expect("lease");
    service
        .record_task_checkpoint(
            &task,
            WorkflowCheckpointStatus::TaskRetryScheduled,
            2,
            Some("old checkpoint".into()),
            Some("InternalServiceError".into()),
        )
        .expect("seed checkpoint");

    let report = service
        .handle_retryable_task_execution_failure(
            &app_handle,
            &task.id,
            None,
            &anyhow!(
                "CompletionError: HttpError: Invalid status code 500 Internal Server Error with message: {{\"error\":{{\"code\":\"InternalServiceError\"}}}}"
            ),
        )
        .expect("retryable handler")
        .expect("report");

    let updated_task = service
        .storage
        .get_task_card(&task.id)
        .expect("updated task");
    assert_eq!(
        updated_task.assigned_agent_id.as_deref(),
        Some(fullstack.id.as_str())
    );
    assert!(
        updated_task.input_payload.contains("恢复执行要求"),
        "expected greenfield resume hint to be appended"
    );

    let updated_dispatch = service
        .storage
        .get_task_dispatch(&task.id)
        .expect("dispatch query")
        .expect("dispatch");
    assert_eq!(updated_dispatch.target_agent_id, fullstack.id);

    let checkpoint = service
        .storage
        .latest_workflow_checkpoint_for_task(&task.id)
        .expect("checkpoint query")
        .expect("checkpoint");
    assert_eq!(checkpoint.status, WorkflowCheckpointStatus::TaskReassigned);
    assert!(checkpoint.repo_snapshot.is_empty);
    assert!(report
        .message
        .expect("message")
        .content
        .contains("直接实现 MVP"));
}

#[test]
fn dashboard_state_exposes_workflow_checkpoints() {
    let (service, workspace_root, _) = setup_service();
    let worker_id = create_agent(&service, "执行成员", "Execution Engineer");
    let work_group = service
        .create_work_group(CreateWorkGroupInput {
            name: "Dashboard Checkpoint Group".into(),
            goal: "Verify dashboard state includes workflow checkpoints.".into(),
            working_directory: workspace_root.to_string_lossy().into_owned(),
            kind: WorkGroupKind::Persistent,
            default_visibility: "summary".into(),
            auto_archive: false,
            member_agent_ids: None,
        })
        .expect("group");
    service
        .add_agent_to_work_group(&work_group.id, &worker_id)
        .expect("add worker");

    let task = TaskCard {
        id: new_id(),
        parent_id: None,
        source_message_id: new_id(),
        title: "Checkpoint for dashboard".into(),
        normalized_goal: "Expose checkpoint in dashboard state.".into(),
        input_payload: "Expose checkpoint in dashboard state.".into(),
        priority: 10,
        status: TaskStatus::InProgress,
        work_group_id: work_group.id,
        created_by: "human".into(),
        assigned_agent_id: Some(worker_id),
        created_at: now(),
    };
    service.storage.insert_task_card(&task).expect("task");
    service
        .record_task_checkpoint(
            &task,
            WorkflowCheckpointStatus::TaskRunning,
            0,
            Some("dashboard resume hint".into()),
            None,
        )
        .expect("checkpoint");

    let state = service.dashboard_state().expect("dashboard state");
    let checkpoint = state
        .workflow_checkpoints
        .into_iter()
        .find(|item| item.task_id.as_deref() == Some(task.id.as_str()))
        .expect("checkpoint in dashboard");
    assert_eq!(checkpoint.status, WorkflowCheckpointStatus::TaskRunning);
    assert_eq!(
        checkpoint.resume_hint.as_deref(),
        Some("dashboard resume hint")
    );
}
