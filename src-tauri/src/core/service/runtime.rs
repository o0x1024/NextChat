use anyhow::{anyhow, Result};
use serde_json::json;
use tauri::{AppHandle, Runtime};
use tokio::sync::{mpsc, oneshot};

use super::{
    collect_allowed_tools, emit, execution_payloads, scored_candidates,
    should_skip_implicit_todowrite_fallback, summary_stream::SummaryStreamSession,
    tool_stream::ToolStreamSession, AppService,
};
use crate::core::domain::{
    new_id, now, AgentExecutor, AgentProfile, ChatStreamEvent, ChatStreamPhase, ClaimContext,
    ClaimScorer, ConversationMessage, LeaseState, MessageKind, SenderKind, SummaryStreamSignal,
    TaskCard, TaskExecutionContext, TaskStatus, ToolCallProgressEvent, ToolCallProgressPhase,
    ToolRun, ToolRunState, ToolStreamChunk, Visibility,
};
use crate::core::skill_policy::selected_skills_for_agent;
use crate::core::tool_approval::parse_pending_approval_request;
use crate::core::workflow::{NarrativeMessageType, RequestRouteMode};

impl AppService {
    pub(super) fn spawn_task_execution<R: Runtime>(
        &self,
        app: AppHandle<R>,
        task_card_id: String,
        tool_run_id: Option<String>,
    ) {
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = service
                .run_task(app.clone(), &task_card_id, tool_run_id.as_deref())
                .await
            {
                if let Ok(Some(report)) = service.handle_pending_user_question_request(
                    &app,
                    &task_card_id,
                    tool_run_id.as_deref(),
                    &error,
                ) {
                    if let Some(task) = report.task {
                        let _ = emit(&app, "task:status-changed", &task);
                    }
                    for tool_run in report.cancelled_tool_runs {
                        let _ = emit(&app, "tool:run-completed", &tool_run);
                    }
                    if let Some(message) = report.message {
                        let _ = emit(&app, "chat:message-created", &message);
                    }
                    let _ = emit(&app, "audit:event-created", &report.audit_event);
                    return;
                }
                if let Ok(Some(report)) = service.handle_retryable_task_execution_failure(
                    &app,
                    &task_card_id,
                    tool_run_id.as_deref(),
                    &error,
                ) {
                    if let Some(task) = report.task {
                        let _ = emit(&app, "task:status-changed", &task);
                    }
                    for tool_run in report.cancelled_tool_runs {
                        let _ = emit(&app, "tool:run-completed", &tool_run);
                    }
                    if let Some(message) = report.message {
                        let _ = emit(&app, "chat:message-created", &message);
                    }
                    let _ = emit(&app, "audit:event-created", &report.audit_event);
                    return;
                }
                if let Ok(report) = service.handle_task_execution_failure(&task_card_id, &error) {
                    if let Some(task) = report.task {
                        let _ = emit(&app, "task:status-changed", &task);
                    }
                    for tool_run in report.cancelled_tool_runs {
                        let _ = emit(&app, "tool:run-completed", &tool_run);
                    }
                    if let Some(message) = report.message {
                        let _ = emit(&app, "chat:message-created", &message);
                    }
                    let _ = emit(&app, "audit:event-created", &report.audit_event);
                }
            }
        });
    }

    async fn run_task<R: Runtime>(
        &self,
        app: AppHandle<R>,
        task_card_id: &str,
        tool_run_id: Option<&str>,
    ) -> Result<()> {
        let mut task = self.storage.get_task_card(task_card_id)?;
        let mut lease = self
            .storage
            .get_lease_by_task(task_card_id)?
            .ok_or_else(|| anyhow!("lease missing for task"))?;
        if lease.state == LeaseState::Paused {
            return Ok(());
        }

        task.status = TaskStatus::InProgress;
        self.storage.update_task_card(&task)?;
        self.record_task_checkpoint(
            &task,
            crate::core::workflow::WorkflowCheckpointStatus::TaskRunning,
            0,
            None,
            None,
        )?;
        emit(&app, "task:status-changed", &task)?;

        let work_group = self.storage.get_work_group(&task.work_group_id)?;
        let owner_id = lease.owner_agent_id.clone();
        let agent = self.storage.get_agent(&owner_id)?;
        let available_tools = self.tool_runtime.available_tools_for_agent(&agent);
        let auto_runnable_tool_ids = available_tools
            .iter()
            .filter(|tool| {
                !agent.permission_policy.requires_approval(&tool.id)
                    && tool.risk_level != crate::core::domain::ToolRiskLevel::High
            })
            .map(|tool| tool.id.clone())
            .collect::<Vec<_>>();
        let work_group_members = self
            .storage
            .list_agents()?
            .into_iter()
            .filter(|candidate| work_group.member_agent_ids.contains(&candidate.id))
            .collect();
        let messages = self.storage.list_messages_for_group(&work_group.id)?;
        let memory_context = self.load_memory_context(&agent, &work_group)?;
        self.record_memory_context(&app, &task, &memory_context)?;
        if let Some(progress_message) = self.build_agent_progress_message(&task, &agent).await? {
            self.storage.insert_message(&progress_message)?;
            emit(&app, "chat:message-created", &progress_message)?;
        }
        let mut approved_tool_input = None;
        let mut approved_tool = if let Some(id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(id)?;
            tool_run.state = ToolRunState::Running;
            tool_run.started_at = Some(now());
            self.storage.insert_tool_run(&tool_run)?;
            emit(&app, "tool:run-started", &tool_run)?;
            approved_tool_input = tool_run.result_ref.clone();
            self.tool_runtime.tool_by_id(&tool_run.tool_id)
        } else {
            self.tool_runtime
                .select_tool_for_text(&task.input_payload, &auto_runnable_tool_ids)
        };
        if approved_tool
            .as_ref()
            .is_some_and(|tool| should_skip_implicit_todowrite_fallback(&task.input_payload, tool))
        {
            approved_tool = None;
        }
        let normalized_tool_input = approved_tool.as_ref().map(|tool| {
            self.tool_runtime.normalize_compat_input(
                tool,
                approved_tool_input
                    .as_deref()
                    .unwrap_or(task.input_payload.as_str()),
            )
        });
        let tool_call_id = approved_tool.as_ref().map(|_| new_id());

        if let Some(tool) = approved_tool.clone() {
            let decision = self.tool_runtime.authorize_tool_call(
                &agent,
                &tool,
                normalized_tool_input
                    .as_deref()
                    .unwrap_or(task.input_payload.as_str()),
                &work_group.working_directory,
            )?;
            if !decision.allowed {
                self.handle_permission_denial(
                    &app,
                    &work_group.id,
                    &mut task,
                    Some(&mut lease),
                    &agent,
                    &tool,
                    &decision
                        .reason
                        .unwrap_or_else(|| "tool access rejected".to_string()),
                )?;
                if let Some(id) = tool_run_id {
                    let mut tool_run = self.storage.get_tool_run(id)?;
                    tool_run.state = ToolRunState::Cancelled;
                    tool_run.finished_at = Some(now());
                    self.storage.insert_tool_run(&tool_run)?;
                    emit(&app, "tool:run-completed", &tool_run)?;
                }
                return Ok(());
            }
        }

        if let Some(tool) = approved_tool.clone() {
            let tool_call_message = ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::Agent,
                sender_id: agent.id.clone(),
                sender_name: agent.name.clone(),
                kind: MessageKind::ToolCall,
                visibility: Visibility::Backstage,
                content: execution_payloads::structured_tool_call_content(
                    &tool,
                    tool_call_id.as_deref().unwrap_or(tool.id.as_str()),
                    normalized_tool_input
                        .as_deref()
                        .unwrap_or(task.input_payload.as_str()),
                ),
                mentions: vec![agent.id.clone()],
                task_card_id: Some(task.id.clone()),
                execution_mode: None,
                created_at: now(),
            };
            self.storage.insert_message(&tool_call_message)?;
            emit(&app, "chat:message-created", &tool_call_message)?;
            self.record_audit(
                "tool_run.started",
                "task_card",
                &task.id,
                json!({ "toolId": tool.id, "agentId": agent.id }),
            )?;
        }

        let available_skills = selected_skills_for_agent(&agent, &self.tool_runtime.all_skills());

        let summary_stream_id = new_id();

        let (summary_stream_tx, mut summary_stream_rx) =
            mpsc::unbounded_channel::<SummaryStreamSignal>();
        let (summary_stream_state_tx, summary_stream_state_rx) =
            oneshot::channel::<super::summary_stream::SummaryStreamSnapshot>();
        let stream_app = app.clone();
        let summary_stream_conversation_id = work_group.id.clone();
        let summary_stream_work_group_id = work_group.id.clone();
        let summary_stream_sender_id = agent.id.clone();
        let summary_stream_sender_name = agent.name.clone();
        let summary_stream_task_card_id = Some(task.id.clone());
        tokio::spawn(async move {
            let mut stream_session = SummaryStreamSession::new(
                summary_stream_id,
                summary_stream_conversation_id,
                summary_stream_work_group_id,
                summary_stream_sender_id,
                summary_stream_sender_name,
                summary_stream_task_card_id,
            );
            let _ = stream_session.start_current(&stream_app);
            while let Some(signal) = summary_stream_rx.recv().await {
                let _ = stream_session.handle_signal(&stream_app, signal);
            }
            let _ = summary_stream_state_tx.send(stream_session.into_snapshot());
        });

        let (tool_stream_tx, mut tool_stream_rx) = mpsc::unbounded_channel::<ToolStreamChunk>();
        let (tool_stream_state_tx, tool_stream_state_rx) = oneshot::channel::<usize>();
        let tool_stream_app = app.clone();
        let tool_stream_conversation_id = work_group.id.clone();
        let tool_stream_work_group_id = work_group.id.clone();
        let tool_stream_sender_id = agent.id.clone();
        let tool_stream_sender_name = agent.name.clone();
        let tool_stream_task_card_id = Some(task.id.clone());
        tokio::spawn(async move {
            let mut session = ToolStreamSession::new(
                tool_stream_conversation_id,
                tool_stream_work_group_id,
                tool_stream_sender_id,
                tool_stream_sender_name,
                tool_stream_task_card_id,
            );
            while let Some(chunk) = tool_stream_rx.recv().await {
                let _ = session.handle_chunk(&tool_stream_app, chunk);
            }
            let track_count = session.finish(&tool_stream_app).unwrap_or(0);
            let _ = tool_stream_state_tx.send(track_count);
        });

        let (tool_call_stream_tx, mut tool_call_stream_rx) =
            mpsc::unbounded_channel::<ToolCallProgressEvent>();
        let tool_call_storage = self.storage.clone();
        let tool_call_app = app.clone();
        let tool_call_work_group_id = work_group.id.clone();
        let tool_call_agent_id = agent.id.clone();
        let tool_call_agent_name = agent.name.clone();
        let tool_call_task_card_id = task.id.clone();
        tokio::spawn(async move {
            while let Some(event) = tool_call_stream_rx.recv().await {
                let content = match event.phase {
                    ToolCallProgressPhase::Started => {
                        execution_payloads::structured_tool_call_content_for_fields(
                            &event.tool_id,
                            &event.tool_name,
                            &event.call_id,
                            &event.input,
                        )
                    }
                    ToolCallProgressPhase::Completed => {
                        execution_payloads::structured_tool_result_content_for_fields(
                            &event.tool_id,
                            &event.tool_name,
                            Some(&event.call_id),
                            &event.input,
                            &event.output,
                        )
                    }
                };
                let message = ConversationMessage {
                    id: new_id(),
                    conversation_id: tool_call_work_group_id.clone(),
                    work_group_id: tool_call_work_group_id.clone(),
                    sender_kind: SenderKind::Agent,
                    sender_id: tool_call_agent_id.clone(),
                    sender_name: tool_call_agent_name.clone(),
                    kind: match event.phase {
                        ToolCallProgressPhase::Started => MessageKind::ToolCall,
                        ToolCallProgressPhase::Completed => MessageKind::ToolResult,
                    },
                    visibility: Visibility::Backstage,
                    content,
                    mentions: vec![tool_call_agent_id.clone()],
                    task_card_id: Some(tool_call_task_card_id.clone()),
                    execution_mode: None,
                    created_at: now(),
                };
                let _ = tool_call_storage.insert_message(&message);
                let _ = emit(&tool_call_app, "chat:message-created", &message);
            }
        });

        let execution_result = self
            .agent_runtime
            .execute_task(TaskExecutionContext {
                agent: agent.clone(),
                work_group: work_group.clone(),
                work_group_members,
                task_card: task.clone(),
                conversation_window: messages,
                memory_context,
                available_tools,
                available_skills,
                approved_tool: approved_tool.clone(),
                approved_tool_input: normalized_tool_input.clone(),
                settings: self.storage.get_settings()?,
                summary_stream: Some(summary_stream_tx.clone()),
                tool_stream: Some(tool_stream_tx.clone()),
                tool_call_stream: Some(tool_call_stream_tx.clone()),
            })
            .await;
        drop(summary_stream_tx);
        drop(tool_stream_tx);
        drop(tool_call_stream_tx);
        let summary_stream_state = summary_stream_state_rx.await.unwrap_or_else(|_| {
            super::summary_stream::SummaryStreamSnapshot {
                committed_segments: Vec::new(),
                current_stream_id: new_id(),
                current_content: String::new(),
                current_sequence: 0,
                current_started: false,
            }
        });
        let _ = tool_stream_state_rx.await;

        let execution = match execution_result {
            Ok(execution) => execution,
            Err(error) => {
                if let Some(request) = parse_pending_approval_request(&error.to_string()) {
                    if let Some(tool) = self.tool_runtime.tool_by_id(&request.tool_id) {
                        if summary_stream_state.current_started {
                            let done_event = ChatStreamEvent {
                                stream_id: summary_stream_state.current_stream_id.clone(),
                                phase: ChatStreamPhase::Done,
                                conversation_id: work_group.id.clone(),
                                work_group_id: work_group.id.clone(),
                                sender_id: agent.id.clone(),
                                sender_name: agent.name.clone(),
                                kind: MessageKind::Summary,
                                visibility: Visibility::Main,
                                task_card_id: Some(task.id.clone()),
                                sequence: summary_stream_state.current_sequence + 1,
                                delta: None,
                                full_content: Some(summary_stream_state.current_content.clone()),
                                created_at: now(),
                            };
                            let _ = emit(&app, "chat:stream-done", &done_event);
                        }
                        self.request_tool_approval(
                            &app,
                            &work_group.id,
                            &mut task,
                            Some(&mut lease),
                            &agent,
                            &tool,
                            &request.input,
                        )?;
                        return Ok(());
                    }
                }
                if summary_stream_state.current_started {
                    let done_event = ChatStreamEvent {
                        stream_id: summary_stream_state.current_stream_id.clone(),
                        phase: ChatStreamPhase::Done,
                        conversation_id: work_group.id.clone(),
                        work_group_id: work_group.id.clone(),
                        sender_id: agent.id.clone(),
                        sender_name: agent.name.clone(),
                        kind: MessageKind::Summary,
                        visibility: Visibility::Main,
                        task_card_id: Some(task.id.clone()),
                        sequence: summary_stream_state.current_sequence + 1,
                        delta: None,
                        full_content: Some(summary_stream_state.current_content.clone()),
                        created_at: now(),
                    };
                    let _ = emit(&app, "chat:stream-done", &done_event);
                }
                return Err(error);
            }
        };

        if let Some(id) = tool_run_id {
            let mut tool_run = self.storage.get_tool_run(id)?;
            tool_run.state = ToolRunState::Completed;
            tool_run.finished_at = Some(now());
            tool_run.result_ref = execution.tool_output.clone();
            self.storage.insert_tool_run(&tool_run)?;
            emit(&app, "tool:run-completed", &tool_run)?;
        }

        if let Some(tool_output) = execution.tool_output.clone() {
            let tool_result_content = if let Some(tool) = approved_tool.as_ref() {
                execution_payloads::structured_tool_result_content(
                    tool,
                    tool_call_id.as_deref(),
                    &task.input_payload,
                    &tool_output,
                )
            } else {
                tool_output.clone()
            };
            let tool_result_message = ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::Agent,
                sender_id: agent.id.clone(),
                sender_name: agent.name.clone(),
                kind: MessageKind::ToolResult,
                visibility: Visibility::Backstage,
                content: tool_result_content,
                mentions: vec![agent.id.clone()],
                task_card_id: Some(task.id.clone()),
                execution_mode: Some(execution.execution_mode.clone()),
                created_at: now(),
            };
            self.storage.insert_message(&tool_result_message)?;
            emit(&app, "chat:message-created", &tool_result_message)?;
            self.record_audit(
                "tool_run.completed",
                "task_card",
                &task.id,
                json!({ "agentId": agent.id, "result": tool_output }),
            )?;
        }

        for segment in &summary_stream_state.committed_segments {
            let streamed_summary_message = ConversationMessage {
                id: segment.stream_id.clone(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::Agent,
                sender_id: agent.id.clone(),
                sender_name: agent.name.clone(),
                kind: MessageKind::Summary,
                visibility: Visibility::Main,
                content: segment.content.clone(),
                mentions: vec![],
                task_card_id: Some(task.id.clone()),
                execution_mode: Some(execution.execution_mode.clone()),
                created_at: segment.started_at.clone(),
            };
            self.storage.insert_message(&streamed_summary_message)?;
            emit(&app, "chat:message-created", &streamed_summary_message)?;
        }

        if summary_stream_state.current_started {
            let done_event = ChatStreamEvent {
                stream_id: summary_stream_state.current_stream_id.clone(),
                phase: ChatStreamPhase::Done,
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_id: agent.id.clone(),
                sender_name: agent.name.clone(),
                kind: MessageKind::Summary,
                visibility: Visibility::Main,
                task_card_id: Some(task.id.clone()),
                sequence: summary_stream_state.current_sequence + 1,
                delta: None,
                full_content: Some(execution.summary.clone()),
                created_at: now(),
            };
            emit(&app, "chat:stream-done", &done_event)?;
        }

        let narrative_kind =
            self.storage
                .get_task_dispatch(&task.id)?
                .map(|dispatch| match dispatch.route_mode {
                    RequestRouteMode::OwnerOrchestrated => NarrativeMessageType::AgentDelivery,
                    RequestRouteMode::DirectAgentAssign => NarrativeMessageType::DirectResult,
                    RequestRouteMode::DirectAnswer => NarrativeMessageType::DirectResult,
                });

        let summary_message = ConversationMessage {
            id: summary_stream_state.current_stream_id,
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Summary,
            visibility: Visibility::Main,
            content: if let Some(kind) = narrative_kind {
                self.build_task_narrative_content(&task, kind, execution.summary.clone())?
            } else {
                execution.summary.clone()
            },
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            execution_mode: Some(execution.execution_mode.clone()),
            created_at: now(),
        };
        self.storage.insert_message(&summary_message)?;
        emit(&app, "chat:message-created", &summary_message)?;

        let backstage_message = ConversationMessage {
            id: new_id(),
            conversation_id: work_group.id.clone(),
            work_group_id: work_group.id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Backstage,
            content: execution.backstage_notes.clone(),
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            execution_mode: Some(execution.execution_mode.clone()),
            created_at: now(),
        };
        self.storage.insert_message(&backstage_message)?;
        emit(&app, "chat:message-created", &backstage_message)?;
        self.emit_collaboration_result(
            &app,
            &task,
            &agent,
            TaskStatus::Completed,
            &execution.summary,
            Some(execution.execution_mode.clone()),
        )?;

        self.persist_execution_memory(&app, &task, &agent, &work_group, &execution.summary)?;

        let mut spawned_subtasks = Vec::new();
        if !execution.suggested_subtasks.is_empty() {
            for subtask in &execution.suggested_subtasks {
                if let Some(created_task) = self.spawn_subtask(&app, &task, &agent, subtask)? {
                    spawned_subtasks.push(created_task);
                }
            }
        }

        if !spawned_subtasks.is_empty() {
            task.status = TaskStatus::WaitingChildren;
            self.storage.update_task_card(&task)?;
            emit(&app, "task:status-changed", &task)?;
            let mut waiting_message = ConversationMessage {
                id: new_id(),
                conversation_id: work_group.id.clone(),
                work_group_id: work_group.id.clone(),
                sender_kind: SenderKind::System,
                sender_id: "coordinator".into(),
                sender_name: "Coordinator".into(),
                kind: MessageKind::Status,
                visibility: Visibility::Main,
                content: format!(
                    "{} is waiting on {} child task(s) before completion.",
                    task.title,
                    spawned_subtasks.len()
                ),
                mentions: vec![agent.id.clone()],
                task_card_id: Some(task.id.clone()),
                execution_mode: None,
                created_at: now(),
            };
            self.assign_group_owner_sender(&mut waiting_message)?;
            self.storage.insert_message(&waiting_message)?;
            emit(&app, "chat:message-created", &waiting_message)?;
            self.record_audit(
                "task.waiting_children",
                "task_card",
                &task.id,
                json!({
                    "children": spawned_subtasks
                        .iter()
                        .map(|item| item.id.clone())
                        .collect::<Vec<_>>()
                }),
            )?;
            return Ok(());
        }

        lease = self
            .storage
            .get_lease_by_task(&task.id)?
            .ok_or_else(|| anyhow!("lease disappeared"))?;
        if lease.state == LeaseState::PreemptRequested {
            lease.state = LeaseState::Paused;
            self.storage.update_lease(&lease)?;
            task.status = TaskStatus::Paused;
            self.storage.update_task_card(&task)?;
            emit(&app, "task:status-changed", &task)?;
            return Ok(());
        }

        lease.state = LeaseState::Released;
        lease.released_at = Some(now());
        self.storage.update_lease(&lease)?;

        task.status = TaskStatus::Completed;
        self.storage.update_task_card(&task)?;
        self.record_task_checkpoint(
            &task,
            crate::core::workflow::WorkflowCheckpointStatus::TaskCompleted,
            0,
            None,
            None,
        )?;
        emit(&app, "task:status-changed", &task)?;
        self.record_audit(
            "task.completed",
            "task_card",
            &task.id,
            json!({ "ownerAgentId": agent.id }),
        )?;
        self.handle_narrative_task_completion(&app, &task, &summary_message.id)?;
        if let Some(parent_id) = task.parent_id.clone() {
            self.reconcile_parent_task(&app, &parent_id)?;
        }
        Ok(())
    }

    async fn build_agent_progress_message(
        &self,
        task: &TaskCard,
        agent: &AgentProfile,
    ) -> Result<Option<ConversationMessage>> {
        let Some(dispatch) = self.storage.get_task_dispatch(&task.id)? else {
            return Ok(None);
        };

        if dispatch.route_mode == RequestRouteMode::DirectAnswer {
            return Ok(None);
        }
        let decision = self
            .build_agent_progress_decision_async(task, agent)
            .await?;
        let progress_text = decision
            .text
            .ok_or_else(|| anyhow!("agent progress text missing"))?;

        let mut envelope = crate::core::workflow::NarrativeEnvelope::new(
            NarrativeMessageType::AgentProgress,
            progress_text,
        );
        envelope.task_id = Some(task.id.clone());
        envelope.workflow_id = dispatch.workflow_id.clone();
        envelope.stage_id = dispatch.stage_id.clone();
        envelope.stage_title = dispatch.narrative_stage_label.clone();
        envelope.task_title = dispatch
            .narrative_task_label
            .clone()
            .or_else(|| Some(task.title.clone()));
        envelope.progress_percent = decision.progress_percent;

        Ok(Some(ConversationMessage {
            id: new_id(),
            conversation_id: task.work_group_id.clone(),
            work_group_id: task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Status,
            visibility: Visibility::Main,
            content: serde_json::to_string(&envelope)?,
            mentions: vec![],
            task_card_id: Some(task.id.clone()),
            execution_mode: None,
            created_at: now(),
        }))
    }

    pub(super) fn spawn_subtask<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        parent_task: &TaskCard,
        owner_agent: &AgentProfile,
        content: &str,
    ) -> Result<Option<TaskCard>> {
        let work_group = self.storage.get_work_group(&parent_task.work_group_id)?;
        let all_agents = self.storage.list_agents()?;
        let members: Vec<AgentProfile> = all_agents
            .into_iter()
            .filter(|agent| {
                work_group.member_agent_ids.contains(&agent.id)
                    && agent.id != owner_agent.id
                    && !self.is_builtin_group_owner_profile(agent)
            })
            .collect();
        if members.is_empty() {
            return Ok(None);
        }

        let scored_members = scored_candidates(&self.tool_runtime, &members);
        let task_card = TaskCard {
            id: new_id(),
            parent_id: Some(parent_task.id.clone()),
            source_message_id: parent_task.source_message_id.clone(),
            title: crate::core::coordinator::Coordinator::build_task_title(content),
            normalized_goal: content.to_string(),
            input_payload: content.to_string(),
            priority: 60,
            status: TaskStatus::Bidding,
            work_group_id: work_group.id.clone(),
            created_by: owner_agent.id.clone(),
            assigned_agent_id: None,
            created_at: now(),
        };
        let active_loads = self
            .storage
            .counts_for_agents(&work_group.member_agent_ids)?;
        let mut selected_tool = self.tool_runtime.select_tool_for_text(
            content,
            &collect_allowed_tools(&self.tool_runtime, &members),
        );
        if selected_tool
            .as_ref()
            .is_some_and(|tool| should_skip_implicit_todowrite_fallback(content, tool))
        {
            selected_tool = None;
        }
        let mentioned_agent_ids =
            crate::core::coordinator::Coordinator::extract_mentions(content, &members);
        let claim_plan = self.coordinator.score(ClaimContext {
            task_card,
            work_group: work_group.clone(),
            candidates: scored_members,
            content: content.to_string(),
            mentioned_agent_ids,
            active_loads,
            requested_tool: selected_tool.clone(),
        })?;
        let mut claim_plan = claim_plan;
        let selected_tool = claim_plan.requested_tool.clone();
        let mut denied_request = None;
        let mut selected_tool_requires_approval = false;
        if let (Some(tool), Some(lease)) = (selected_tool.clone(), claim_plan.lease.as_ref()) {
            if let Some(agent) = members
                .iter()
                .find(|candidate| candidate.id == lease.owner_agent_id)
                .cloned()
            {
                let decision = self.tool_runtime.authorize_tool_call(
                    &agent,
                    &tool,
                    &claim_plan.task_card.input_payload,
                    &work_group.working_directory,
                )?;
                if !decision.allowed {
                    claim_plan.task_card.status = TaskStatus::NeedsReview;
                    denied_request = Some((
                        agent,
                        tool,
                        decision
                            .reason
                            .unwrap_or_else(|| "tool access rejected".to_string()),
                    ));
                    claim_plan.lease = None;
                } else if decision.approval_required {
                    claim_plan.task_card.status = TaskStatus::WaitingApproval;
                    selected_tool_requires_approval = true;
                }
            }
        }
        self.storage.insert_task_card(&claim_plan.task_card)?;
        emit(app, "task:card-created", &claim_plan.task_card)?;
        for bid in &claim_plan.bids {
            self.storage.insert_claim_bid(bid)?;
            emit(app, "claim:bid-submitted", bid)?;
        }
        for message in &claim_plan.coordinator_messages {
            let mut message = message.clone();
            self.assign_group_owner_sender(&mut message)?;
            self.storage.insert_message(&message)?;
            emit(app, "chat:message-created", &message)?;
        }
        if let Some(ref lease) = claim_plan.lease {
            self.storage.insert_lease(lease)?;
            emit(app, "lease:granted", lease)?;
        }
        let collaborator = claim_plan
            .task_card
            .assigned_agent_id
            .as_deref()
            .and_then(|agent_id| members.iter().find(|candidate| candidate.id == agent_id));
        self.emit_collaboration_request(
            app,
            parent_task,
            &claim_plan.task_card,
            owner_agent,
            collaborator,
        )?;
        if let Some((agent, tool, reason)) = denied_request {
            self.handle_permission_denial(
                app,
                &work_group.id,
                &mut claim_plan.task_card,
                None,
                &agent,
                &tool,
                &reason,
            )?;
        } else if let Some(tool) = selected_tool {
            if let Some(ref lease) = claim_plan.lease {
                let tool_run = ToolRun {
                    id: new_id(),
                    tool_id: tool.id.clone(),
                    task_card_id: claim_plan.task_card.id.clone(),
                    agent_id: lease.owner_agent_id.clone(),
                    state: if selected_tool_requires_approval {
                        ToolRunState::PendingApproval
                    } else {
                        ToolRunState::Queued
                    },
                    approval_required: selected_tool_requires_approval,
                    started_at: None,
                    finished_at: None,
                    result_ref: None,
                };
                self.storage.insert_tool_run(&tool_run)?;
                if tool_run.approval_required {
                    let mut approval_message = ConversationMessage {
                        id: new_id(),
                        conversation_id: work_group.id.clone(),
                        work_group_id: work_group.id.clone(),
                        sender_kind: SenderKind::System,
                        sender_id: "coordinator".into(),
                        sender_name: "Coordinator".into(),
                        kind: MessageKind::Approval,
                        visibility: Visibility::Main,
                        content: format!("Approval required for {} before execution.", tool.name),
                        mentions: vec![lease.owner_agent_id.clone()],
                        task_card_id: Some(claim_plan.task_card.id.clone()),
                        execution_mode: None,
                        created_at: now(),
                    };
                    self.assign_group_owner_sender(&mut approval_message)?;
                    self.storage.insert_message(&approval_message)?;
                    emit(app, "chat:message-created", &approval_message)?;
                    emit(app, "approval:requested", &tool_run)?;
                } else {
                    self.spawn_task_execution(
                        app.clone(),
                        claim_plan.task_card.id.clone(),
                        Some(tool_run.id),
                    );
                }
            }
        } else if claim_plan.lease.is_some() {
            self.spawn_task_execution(app.clone(), claim_plan.task_card.id.clone(), None);
        }
        Ok(Some(claim_plan.task_card))
    }

    pub(super) fn reconcile_parent_task<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        parent_id: &str,
    ) -> Result<()> {
        let parent_task_before = self.storage.get_task_card(parent_id)?;
        let child_tasks = self.storage.list_child_tasks(parent_id)?;
        if !self.reconcile_parent_task_state(parent_id)? {
            return Ok(());
        }
        let parent_task = self.storage.get_task_card(parent_id)?;
        emit(app, "task:status-changed", &parent_task)?;
        let has_issue = matches!(parent_task.status, TaskStatus::NeedsReview);
        let completed_children = child_tasks
            .iter()
            .filter(|child| matches!(child.status, TaskStatus::Completed))
            .count();
        let blocked_children = child_tasks
            .iter()
            .filter(|child| {
                matches!(
                    child.status,
                    TaskStatus::Cancelled | TaskStatus::NeedsReview
                )
            })
            .count();

        let mut status_message = ConversationMessage {
            id: new_id(),
            conversation_id: parent_task.work_group_id.clone(),
            work_group_id: parent_task.work_group_id.clone(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind: MessageKind::Summary,
            visibility: Visibility::Main,
            content: if has_issue {
                format!(
                    "Parent task '{}' moved to needs review after {} child task(s) completed and {} child task(s) ended with issues.",
                    parent_task.title, completed_children, blocked_children
                )
            } else {
                format!(
                    "Parent task '{}' completed after all {} child task(s) finished.",
                    parent_task.title,
                    child_tasks.len()
                )
            },
            mentions: vec![],
            task_card_id: Some(parent_task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        self.assign_group_owner_sender(&mut status_message)?;
        if parent_task_before.status != parent_task.status {
            self.storage.insert_message(&status_message)?;
            emit(app, "chat:message-created", &status_message)?;
        }
        self.record_audit(
            "task.parent_reconciled",
            "task_card",
            &parent_task.id,
            json!({
                "childTaskIds": child_tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
                "status": parent_task.status.clone(),
            }),
        )?;

        if let Some(grand_parent_id) = parent_task.parent_id.clone() {
            self.reconcile_parent_task(app, &grand_parent_id)?;
        }

        Ok(())
    }

    pub(super) fn reconcile_parent_task_state(&self, parent_id: &str) -> Result<bool> {
        let mut parent_task = self.storage.get_task_card(parent_id)?;
        let child_tasks = self.storage.list_child_tasks(parent_id)?;
        if child_tasks.is_empty() {
            return Ok(false);
        }

        let has_terminal_children = child_tasks.iter().all(|child| {
            matches!(
                child.status,
                TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::NeedsReview
            )
        });
        if !has_terminal_children {
            return Ok(false);
        }

        let has_issue = child_tasks.iter().any(|child| {
            matches!(
                child.status,
                TaskStatus::Cancelled | TaskStatus::NeedsReview
            )
        });
        let next_status = if has_issue {
            TaskStatus::NeedsReview
        } else {
            TaskStatus::Completed
        };
        let changed = parent_task.status != next_status;
        if changed {
            parent_task.status = next_status;
            self.storage.update_task_card(&parent_task)?;
        }

        if let Some(mut lease) = self.storage.get_lease_by_task(parent_id)? {
            if lease.state != LeaseState::Released {
                lease.state = LeaseState::Released;
                lease.released_at = Some(now());
                lease.preempt_requested_at = None;
                self.storage.update_lease(&lease)?;
            }
        }

        Ok(changed)
    }
}
