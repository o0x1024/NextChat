use anyhow::{anyhow, Context, Result};
use tauri::{AppHandle, Runtime};

use super::{block_on_service_future, emit, AppService};
use crate::core::domain::{
    new_id, now, AgentExecutor, AgentProfile, ConversationMessage, MessageKind, SenderKind,
    TaskCard, TaskStatus, WorkGroup,
};
use crate::core::skill_policy::selected_skills_for_agent;
use crate::core::workflow::{
    NarrativeEnvelope, NarrativeMessageType, PlannedTask, RequestRouteMode, TaskDispatchRecord,
    TaskDispatchSource,
};

impl AppService {
    pub(super) fn dispatch_direct_answer<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        members: &[AgentProfile],
    ) -> Result<()> {
        let owner = self
            .group_owner_for_work_group(&work_group.id)?
            .or_else(|| members.first().cloned())
            .context("no agent available for direct answer")?;
        let answer = block_on_service_future(self.complete_direct_answer(
            work_group,
            source_message,
            &owner,
            members,
        ))?;
        let mut envelope = NarrativeEnvelope::new(NarrativeMessageType::DirectResult, answer);
        envelope.progress_percent = Some(100);
        let message = self.agent_message_from_envelope(
            &work_group.id,
            &owner,
            envelope,
            MessageKind::Summary,
            None,
        )?;
        self.storage.insert_message(&message)?;
        emit(app, "chat:message-created", &message)?;
        Ok(())
    }

    async fn complete_direct_answer(
        &self,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        agent: &AgentProfile,
        members: &[AgentProfile],
    ) -> Result<String> {
        let current_messages = self.storage.list_messages_for_group(&work_group.id)?;
        let memory_context = self.load_memory_context(agent, work_group)?;
        let task = TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: source_message.id.clone(),
            title: source_message.content.chars().take(40).collect(),
            normalized_goal: source_message.content.clone(),
            input_payload: source_message.content.clone(),
            priority: 20,
            status: TaskStatus::InProgress,
            work_group_id: work_group.id.clone(),
            created_by: "human".into(),
            assigned_agent_id: Some(agent.id.clone()),
            created_at: now(),
        };
        let execution = self
            .agent_runtime
            .execute_task(crate::core::domain::TaskExecutionContext {
                agent: agent.clone(),
                work_group: work_group.clone(),
                work_group_members: members.to_vec(),
                task_card: task,
                conversation_window: current_messages,
                memory_context,
                available_tools: vec![],
                available_skills: selected_skills_for_agent(agent, &self.tool_runtime.all_skills()),
                approved_tool: None,
                approved_tool_input: None,
                settings: self.storage.get_settings()?,
                summary_stream: None,
                tool_stream: None,
                tool_call_stream: None,
            })
            .await?;
        Ok(execution.summary)
    }

    pub(super) fn dispatch_direct_agent_task<R: Runtime>(
        &self,
        app: AppHandle<R>,
        source_message: &ConversationMessage,
        members: &[AgentProfile],
        target_agent_ids: &[String],
    ) -> Result<Vec<TaskCard>> {
        let tasks = match build_direct_tasks(source_message, members, target_agent_ids) {
            Ok(tasks) => tasks,
            Err(_) => {
                if let Some(agent_id) = target_agent_ids.first() {
                    if let Some(agent) = members.iter().find(|member| &member.id == agent_id) {
                        let mut envelope = NarrativeEnvelope::new(
                            NarrativeMessageType::BlockerRaised,
                            "当前多位 agent 的职责划分还不够清晰，请补充每位成员分别负责什么。",
                        );
                        envelope.blocked = Some(true);
                        let message = self.agent_message_from_envelope(
                            &source_message.work_group_id,
                            agent,
                            envelope,
                            MessageKind::Status,
                            None,
                        )?;
                        self.storage.insert_message(&message)?;
                        emit(&app, "chat:message-created", &message)?;
                    }
                }
                return Ok(Vec::new());
            }
        };
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        for spec in &tasks {
            let task = self.create_assigned_task(
                source_message,
                &spec.goal,
                &spec.assignee_agent_id,
                TaskDispatchSource::UserDirect,
            )?;
            self.storage.insert_task_card(&task)?;
            emit(&app, "task:card-created", &task)?;
            let dispatch = TaskDispatchRecord {
                task_id: task.id.clone(),
                workflow_id: None,
                stage_id: None,
                dispatch_source: TaskDispatchSource::UserDirect,
                depends_on_task_ids: vec![],
                acknowledged_at: Some(now()),
                result_message_id: None,
                locked_by_user_mention: true,
                target_agent_id: spec.assignee_agent_id.clone(),
                route_mode: RequestRouteMode::DirectAgentAssign,
                narrative_stage_label: None,
                narrative_task_label: Some(spec.title.clone()),
            };
            self.storage.insert_task_dispatch(&dispatch)?;
            if let Some(agent) = members
                .iter()
                .find(|member| member.id == spec.assignee_agent_id)
            {
                let mut envelope = NarrativeEnvelope::new(
                    NarrativeMessageType::DirectAssign,
                    format!("已将任务直接交给 @{}：{}。", agent.name, spec.goal),
                );
                envelope.task_id = Some(task.id.clone());
                envelope.task_title = Some(spec.title.clone());
                let message = ConversationMessage {
                    id: new_id(),
                    conversation_id: task.work_group_id.clone(),
                    work_group_id: task.work_group_id.clone(),
                    sender_kind: SenderKind::System,
                    sender_id: "router".into(),
                    sender_name: "System".into(),
                    kind: MessageKind::Status,
                    visibility: crate::core::domain::Visibility::Backstage,
                    content: serde_json::to_string(&envelope)?,
                    mentions: vec![agent.id.clone()],
                    task_card_id: Some(task.id.clone()),
                    execution_mode: None,
                    created_at: now(),
                };
                self.storage.insert_message(&message)?;
                emit(&app, "chat:message-created", &message)?;
            }
            self.activate_assigned_task(&app, &task, members, NarrativeMessageType::AgentAck)?;
        }
        Ok(self
            .storage
            .list_task_cards(Some(&source_message.work_group_id))?
            .into_iter()
            .take(tasks.len())
            .collect())
    }

    fn activate_assigned_task<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        task: &TaskCard,
        members: &[AgentProfile],
        narrative_type: NarrativeMessageType,
    ) -> Result<()> {
        let agent_id = task
            .assigned_agent_id
            .clone()
            .context("assigned task missing agent")?;
        let agent = members
            .iter()
            .find(|member| member.id == agent_id)
            .cloned()
            .unwrap_or_else(|| self.storage.get_agent(&agent_id).expect("agent exists"));
        let mut active_task = task.clone();
        active_task.status = TaskStatus::Leased;
        self.storage.update_task_card(&active_task)?;
        emit(app, "task:status-changed", &active_task)?;
        self.storage.insert_lease(&crate::core::domain::Lease {
            id: new_id(),
            task_card_id: active_task.id.clone(),
            owner_agent_id: agent.id.clone(),
            state: crate::core::domain::LeaseState::Active,
            granted_at: now(),
            expires_at: None,
            preempt_requested_at: None,
            released_at: None,
        })?;
        if let Some(lease) = self.storage.get_lease_by_task(&active_task.id)? {
            emit(app, "lease:granted", &lease)?;
        }
        let ack = ConversationMessage {
            id: new_id(),
            conversation_id: active_task.work_group_id.clone(),
            work_group_id: active_task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind: MessageKind::Status,
            visibility: crate::core::domain::Visibility::Main,
            content: self.build_task_narrative_content(
                &active_task,
                narrative_type,
                self.build_agent_ack_text(&active_task, &agent)?,
            )?,
            mentions: vec![],
            task_card_id: Some(active_task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_message(&ack)?;
        emit(app, "chat:message-created", &ack)?;
        self.spawn_task_execution(app.clone(), active_task.id.clone(), None);
        Ok(())
    }

    fn create_assigned_task(
        &self,
        source_message: &ConversationMessage,
        goal: &str,
        assignee_agent_id: &str,
        _dispatch_source: TaskDispatchSource,
    ) -> Result<TaskCard> {
        Ok(TaskCard {
            id: new_id(),
            parent_id: None,
            source_message_id: source_message.id.clone(),
            title: goal.chars().take(72).collect(),
            normalized_goal: goal.to_string(),
            input_payload: goal.to_string(),
            priority: 100,
            status: TaskStatus::Pending,
            work_group_id: source_message.work_group_id.clone(),
            created_by: "human".into(),
            assigned_agent_id: Some(assignee_agent_id.to_string()),
            created_at: now(),
        })
    }
}

fn build_direct_tasks(
    source_message: &ConversationMessage,
    members: &[AgentProfile],
    target_agent_ids: &[String],
) -> Result<Vec<PlannedTask>> {
    if target_agent_ids.len() == 1 {
        let agent_id = target_agent_ids[0].clone();
        let goal = strip_agent_mentions(&source_message.content, members)
            .trim()
            .to_string();
        return Ok(vec![PlannedTask {
            id: new_id(),
            title: compact_title(&goal),
            goal,
            assignee_agent_id: agent_id,
            locked_by_user_mention: true,
            depends_on_task_ids: vec![],
        }]);
    }

    let parts = extract_agent_clauses(&source_message.content, members);
    let mut tasks = Vec::new();
    for target_agent_id in target_agent_ids {
        let Some(agent) = members.iter().find(|member| member.id == *target_agent_id) else {
            continue;
        };
        let Some(goal) = parts.get(&agent.name).cloned() else {
            return Err(anyhow!("需要明确每个被点名 agent 的职责分工"));
        };
        if goal.trim().is_empty() {
            return Err(anyhow!("需要明确每个被点名 agent 的职责分工"));
        }
        tasks.push(PlannedTask {
            id: new_id(),
            title: compact_title(&goal),
            goal,
            assignee_agent_id: target_agent_id.clone(),
            locked_by_user_mention: true,
            depends_on_task_ids: vec![],
        });
    }
    Ok(tasks)
}

fn compact_title(goal: &str) -> String {
    goal.trim().chars().take(40).collect()
}

fn strip_agent_mentions(content: &str, members: &[AgentProfile]) -> String {
    let mut output = content.to_string();
    for member in members {
        output = output.replace(&format!("@{}", member.name), "");
    }
    output.trim().to_string()
}

fn extract_agent_clauses(
    content: &str,
    members: &[AgentProfile],
) -> std::collections::HashMap<String, String> {
    let mut entries = members
        .iter()
        .filter_map(|agent| {
            content
                .find(&format!("@{}", agent.name))
                .map(|index| (index, agent.name.clone()))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(index, _)| *index);
    let mut result = std::collections::HashMap::new();
    for (current_index, (_, name)) in entries.iter().enumerate() {
        let start = content.find(&format!("@{}", name)).unwrap_or_default() + name.len() + 1;
        let end = entries
            .get(current_index + 1)
            .map(|(index, _)| *index)
            .unwrap_or_else(|| content.len());
        result.insert(
            name.clone(),
            content[start..end]
                .trim_matches([' ', '；', ';', '，', ','])
                .to_string(),
        );
    }
    result
}
