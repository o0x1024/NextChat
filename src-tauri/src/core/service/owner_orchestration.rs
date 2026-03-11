use std::collections::HashSet;

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::json;

use super::{
    block_on_service_future, owner_blocker_orchestration::mock_owner_blocker_completion, AppService,
};
use crate::core::domain::{
    new_id, now, AgentProfile, ClaimBid, ConversationMessage, ModelProviderAdapter, TaskCard,
    WorkGroup,
};
use crate::core::llm_rig::RigModelAdapter;
use crate::core::workflow::{
    NarrativeEnvelope, OwnerPlannedStageDraft, OwnerStageNarrativeDecision,
    OwnerTaskAssignmentDecision, OwnerWorkflowPlanDecision, OwnerWorkflowSummaryDecision,
    PlannedStage, PlannedTask, RequestRouteMode, StageStatus, WorkflowExecutionMode, WorkflowPlan,
    WorkflowRecord, WorkflowStageRecord, WorkflowStatus,
};

const MAX_OWNER_STAGES: usize = 6;
const MAX_OWNER_TASKS_PER_STAGE: usize = 4;

impl AppService {
    pub(super) fn build_owner_task_assignment_decision(
        &self,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        members: &[AgentProfile],
        task: &TaskCard,
        bids: &[ClaimBid],
        fallback_agent_id: Option<&str>,
    ) -> Result<OwnerTaskAssignmentDecision> {
        let owner = self
            .group_owner_for_work_group(&work_group.id)?
            .context("work group owner missing")?;
        let candidate_agents = members
            .iter()
            .filter(|agent| agent.id != owner.id)
            .cloned()
            .collect::<Vec<_>>();
        if candidate_agents.is_empty() {
            return Err(anyhow!(
                "owner orchestrated mode requires at least one non-owner agent"
            ));
        }

        block_on_service_future(self.complete_owner_json::<OwnerTaskAssignmentDecision>(
            &owner,
            "owner.single_task_assignment",
            build_owner_assignment_prompt(
                work_group,
                source_message,
                task,
                &candidate_agents,
                bids,
                fallback_agent_id,
            ),
            json!({
                "workGroupId": work_group.id,
                "taskId": task.id,
                "sourceMessageId": source_message.id,
                "actorId": owner.id,
            }),
        ))
    }

    pub(super) fn build_owner_workflow_plan(
        &self,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        members: &[AgentProfile],
    ) -> Result<WorkflowPlan> {
        let owner = self
            .group_owner_for_work_group(&work_group.id)?
            .context("work group owner missing")?;
        let member_pool = members
            .iter()
            .filter(|agent| agent.id != owner.id)
            .cloned()
            .collect::<Vec<_>>();
        if member_pool.is_empty() {
            return Err(anyhow!(
                "owner orchestrated mode requires at least one non-owner agent"
            ));
        }

        let decision =
            block_on_service_future(self.complete_owner_json::<OwnerWorkflowPlanDecision>(
                &owner,
                "owner.workflow_plan",
                build_owner_workflow_prompt(work_group, source_message, &member_pool),
                json!({
                    "workGroupId": work_group.id,
                    "sourceMessageId": source_message.id,
                    "actorId": owner.id,
                }),
            ))?;
        self.owner_workflow_plan_from_decision(
            work_group,
            source_message,
            &owner,
            members,
            decision,
        )
    }

    pub(super) fn build_owner_stage_dispatch_text(
        &self,
        workflow: &WorkflowRecord,
        stage: &WorkflowStageRecord,
        tasks: &[TaskCard],
        members: &[AgentProfile],
    ) -> Result<String> {
        let owner = self
            .group_owner_for_work_group(&workflow.work_group_id)?
            .context("work group owner missing")?;
        let decision =
            block_on_service_future(self.complete_owner_json::<OwnerStageNarrativeDecision>(
                &owner,
                "owner.stage_dispatch",
                build_owner_stage_dispatch_prompt(workflow, stage, tasks, members),
                json!({
                    "workGroupId": workflow.work_group_id,
                    "workflowId": workflow.id,
                    "stageId": stage.id,
                    "taskIds": tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
                    "actorId": owner.id,
                }),
            ))?;
        require_text(decision.dispatch_text, "owner stage dispatch text missing")
    }

    pub(super) fn build_owner_stage_transition_text(
        &self,
        workflow: &WorkflowRecord,
        completed_stage: &WorkflowStageRecord,
        next_stage: &WorkflowStageRecord,
    ) -> Result<String> {
        let owner = self
            .group_owner_for_work_group(&workflow.work_group_id)?
            .context("work group owner missing")?;
        let completed_summaries = self.collect_stage_result_summaries(&completed_stage.id)?;
        let decision =
            block_on_service_future(self.complete_owner_json::<OwnerStageNarrativeDecision>(
                &owner,
                "owner.stage_transition",
                build_owner_stage_transition_prompt(
                    workflow,
                    completed_stage,
                    next_stage,
                    &completed_summaries,
                ),
                json!({
                    "workGroupId": workflow.work_group_id,
                    "workflowId": workflow.id,
                    "completedStageId": completed_stage.id,
                    "nextStageId": next_stage.id,
                    "actorId": owner.id,
                }),
            ))?;
        require_text(
            decision.transition_text,
            "owner stage transition text missing",
        )
    }

    pub(super) fn build_owner_workflow_summary_text(
        &self,
        workflow: &WorkflowRecord,
    ) -> Result<String> {
        let owner = self
            .group_owner_for_work_group(&workflow.work_group_id)?
            .context("work group owner missing")?;
        let delivered_summaries = self.collect_workflow_result_summaries(&workflow.id)?;
        let decision =
            block_on_service_future(self.complete_owner_json::<OwnerWorkflowSummaryDecision>(
                &owner,
                "owner.workflow_summary",
                build_owner_summary_prompt(workflow, &delivered_summaries),
                json!({
                    "workGroupId": workflow.work_group_id,
                    "workflowId": workflow.id,
                    "actorId": owner.id,
                }),
            ))?;
        require_text(decision.summary_text, "owner workflow summary text missing")
    }

    pub(super) async fn complete_owner_json<T>(
        &self,
        owner: &AgentProfile,
        decision_kind: &str,
        prompt: String,
        audit_context: serde_json::Value,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let settings = self.storage.get_settings()?;
        let preamble = format!(
            "你是工作群的群主。你的职责是理解用户目标、组织团队协作、选择合适成员并用自然中文在群里推进任务。\
你必须只返回 JSON，不要输出 Markdown、代码块或解释。\
你只能从给定候选成员中做决定，不能编造新的成员、阶段或工具权限。\
如果信息不足，可以返回保守决策，但仍需给出可执行安排。\
群主身份信息：{} / {} / {}。",
            owner.name, owner.role, owner.objective
        );

        let raw =
            if owner.model_policy.provider == "mock" || owner.model_policy.model == "simulation" {
                mock_owner_completion(decision_kind, &prompt)
            } else {
                RigModelAdapter
                    .complete(&owner.model_policy, &settings, &preamble, &prompt)
                    .await?
                    .ok_or_else(|| anyhow!("owner model is unavailable or not configured"))?
            };

        let Some(payload) = extract_json_object(&raw) else {
            self.record_audit(
                "owner.decision.parse_failed",
                "owner_decision",
                decision_kind,
                json!({
                    "provider": owner.model_policy.provider,
                    "model": owner.model_policy.model,
                    "prompt": truncate_text(&prompt, 4_000),
                    "raw": truncate_text(&raw, 8_000),
                    "context": audit_context,
                }),
            )?;
            return Err(anyhow!("owner model did not return valid JSON"));
        };

        match serde_json::from_str::<T>(payload) {
            Ok(parsed) => {
                self.record_audit(
                    "owner.decision.generated",
                    "owner_decision",
                    decision_kind,
                    json!({
                        "provider": owner.model_policy.provider,
                        "model": owner.model_policy.model,
                        "prompt": truncate_text(&prompt, 4_000),
                        "raw": truncate_text(payload, 8_000),
                        "context": audit_context,
                    }),
                )?;
                Ok(parsed)
            }
            Err(error) => {
                self.record_audit(
                    "owner.decision.parse_failed",
                    "owner_decision",
                    decision_kind,
                    json!({
                        "provider": owner.model_policy.provider,
                        "model": owner.model_policy.model,
                        "error": error.to_string(),
                        "prompt": truncate_text(&prompt, 4_000),
                        "raw": truncate_text(payload, 8_000),
                        "context": audit_context,
                    }),
                )?;
                Err(error).context("failed to parse owner decision JSON")
            }
        }
    }

    fn owner_workflow_plan_from_decision(
        &self,
        work_group: &WorkGroup,
        source_message: &ConversationMessage,
        owner: &AgentProfile,
        members: &[AgentProfile],
        decision: OwnerWorkflowPlanDecision,
    ) -> Result<WorkflowPlan> {
        if decision.stages.is_empty() {
            return Err(anyhow!("owner workflow plan did not include stages"));
        }

        let valid_assignees = members
            .iter()
            .filter(|agent| agent.id != owner.id)
            .map(|agent| agent.id.clone())
            .collect::<HashSet<_>>();
        let mentioned_ids = source_message
            .mentions
            .iter()
            .filter(|agent_id| **agent_id != owner.id)
            .cloned()
            .collect::<HashSet<_>>();
        let workflow = WorkflowRecord {
            id: new_id(),
            work_group_id: work_group.id.clone(),
            source_message_id: source_message.id.clone(),
            route_mode: RequestRouteMode::OwnerOrchestrated,
            title: non_empty_or(
                decision.workflow_title,
                owner_workflow_title(&source_message.content),
            ),
            normalized_intent: source_message.content.trim().to_string(),
            status: WorkflowStatus::Planning,
            owner_agent_id: owner.id.clone(),
            current_stage_id: None,
            created_at: now(),
        };

        let mut stages = Vec::new();
        for (stage_index, stage_draft) in decision
            .stages
            .into_iter()
            .take(MAX_OWNER_STAGES)
            .enumerate()
        {
            let filtered_tasks = stage_draft
                .tasks
                .iter()
                .filter(|task| valid_assignees.contains(&task.assignee_agent_id))
                .cloned()
                .collect::<Vec<_>>();
            let stage_draft = OwnerPlannedStageDraft {
                title: stage_draft.title,
                goal: stage_draft.goal,
                execution_mode: stage_draft.execution_mode,
                tasks: Vec::new(),
            };
            let tasks = filtered_tasks
                .into_iter()
                .take(MAX_OWNER_TASKS_PER_STAGE)
                .collect::<Vec<_>>();
            if tasks.is_empty() {
                continue;
            }
            stages.push(self.owner_stage_from_draft(
                &workflow.id,
                stage_index,
                stage_draft,
                &tasks,
                &mentioned_ids,
            )?);
        }

        if stages.is_empty() {
            return Err(anyhow!(
                "owner workflow plan did not contain valid assignees"
            ));
        }
        if !mentioned_ids.is_empty() {
            let assigned_ids = stages
                .iter()
                .flat_map(|stage| {
                    stage
                        .tasks
                        .iter()
                        .map(|task| task.assignee_agent_id.clone())
                })
                .collect::<HashSet<_>>();
            if !mentioned_ids
                .iter()
                .all(|agent_id| assigned_ids.contains(agent_id))
            {
                return Err(anyhow!(
                    "owner workflow plan ignored explicitly mentioned members"
                ));
            }
        }

        Ok(WorkflowPlan {
            workflow,
            stages,
            owner_ack_text: Some(require_text(
                decision.owner_ack_text,
                "owner workflow ack text missing",
            )?),
            owner_plan_text: Some(require_text(
                decision.owner_plan_text,
                "owner workflow plan text missing",
            )?),
        })
    }

    fn owner_stage_from_draft(
        &self,
        workflow_id: &str,
        stage_index: usize,
        stage_draft: OwnerPlannedStageDraft,
        tasks: &[crate::core::workflow::OwnerPlannedTaskDraft],
        mentioned_ids: &HashSet<String>,
    ) -> Result<PlannedStage> {
        let stage_id = new_id();
        let stage = WorkflowStageRecord {
            id: stage_id,
            workflow_id: workflow_id.to_string(),
            title: non_empty_or(Some(stage_draft.title), format!("阶段 {}", stage_index + 1)),
            goal: non_empty_or(Some(stage_draft.goal), "推进当前目标".to_string()),
            order_index: stage_index as i64 + 1,
            execution_mode: stage_draft.execution_mode,
            status: if stage_index == 0 {
                StageStatus::Ready
            } else {
                StageStatus::Pending
            },
            entry_message_id: None,
            completion_message_id: None,
            created_at: now(),
        };

        let mut planned_tasks = Vec::new();
        let mut serial_dependency: Option<String> = None;
        for task in tasks {
            let depends_on_task_ids = if stage.execution_mode == WorkflowExecutionMode::Serial {
                serial_dependency.iter().cloned().collect()
            } else {
                Vec::new()
            };
            let planned_task = PlannedTask {
                id: new_id(),
                title: compact_title(&task.title),
                goal: non_empty_or(Some(task.goal.clone()), task.title.clone()),
                assignee_agent_id: task.assignee_agent_id.clone(),
                locked_by_user_mention: mentioned_ids.contains(&task.assignee_agent_id),
                depends_on_task_ids,
            };
            serial_dependency = Some(planned_task.id.clone());
            planned_tasks.push(planned_task);
        }

        if planned_tasks.is_empty() {
            return Err(anyhow!("stage does not include any valid tasks"));
        }

        Ok(PlannedStage {
            stage,
            tasks: planned_tasks,
        })
    }

    fn collect_stage_result_summaries(&self, stage_id: &str) -> Result<Vec<String>> {
        let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
        let messages = self.storage.list_messages()?;
        dispatches
            .iter()
            .filter_map(|dispatch| {
                dispatch
                    .result_message_id
                    .as_ref()
                    .map(|message_id| (dispatch, message_id))
            })
            .map(|(dispatch, message_id)| {
                let message = messages
                    .iter()
                    .find(|message| message.id == *message_id)
                    .context("stage result message missing")?;
                let task = self.storage.get_task_card(&dispatch.task_id)?;
                Ok(format!(
                    "{}: {}",
                    task.title,
                    narrative_text_or_raw(&message.content)
                ))
            })
            .collect()
    }

    fn collect_workflow_result_summaries(&self, workflow_id: &str) -> Result<Vec<String>> {
        let dispatches = self.storage.list_workflow_task_dispatches(workflow_id)?;
        let messages = self.storage.list_messages()?;
        dispatches
            .iter()
            .filter_map(|dispatch| {
                dispatch
                    .result_message_id
                    .as_ref()
                    .map(|message_id| (dispatch, message_id))
            })
            .map(|(dispatch, message_id)| {
                let message = messages
                    .iter()
                    .find(|message| message.id == *message_id)
                    .context("workflow result message missing")?;
                let task = self.storage.get_task_card(&dispatch.task_id)?;
                Ok(format!(
                    "{}: {}",
                    task.title,
                    narrative_text_or_raw(&message.content)
                ))
            })
            .collect()
    }
}

fn build_owner_assignment_prompt(
    work_group: &WorkGroup,
    source_message: &ConversationMessage,
    task: &TaskCard,
    candidates: &[AgentProfile],
    bids: &[ClaimBid],
    fallback_agent_id: Option<&str>,
) -> String {
    let candidates_text = candidates
        .iter()
        .map(|agent| {
            let bid = bids.iter().find(|item| item.agent_id == agent.id);
            format!(
                "- id: {}\n  name: {}\n  role: {}\n  objective: {}\n  capabilityScore: {}\n  rationale: {}",
                agent.id,
                agent.name,
                agent.role,
                truncate_text(&agent.objective, 120),
                bid.map(|item| format!("{:.1}", item.capability_score))
                    .unwrap_or_else(|| "n/a".to_string()),
                bid.map(|item| truncate_text(&item.rationale, 200))
                    .unwrap_or_else(|| "n/a".to_string()),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "请作为群主，为一个单任务请求做分派决定。\n\
工作组目标：{}\n\
用户原话：{}\n\
任务标题：{}\n\
任务目标：{}\n\
候选成员：\n{}\n\
当前系统默认候选：{}\n\
请只输出一个 JSON 对象，字段为：assigneeAgentId, ownerAckText, ownerDispatchText, ownerBlockerText。\n\
规则：\n\
- assigneeAgentId 只能是候选成员里的 id，若确实无人适合则返回 null。\n\
- ownerAckText 是群主在群里的自然回复，例如“我先来安排”。\n\
- ownerDispatchText 是群主对被分派成员说的话，必须像真实项目群消息，不要生硬模板。\n\
- ownerBlockerText 是无人可接时群主在群里的说明。\n\
- 不要输出多余字段，不要带 Markdown、解释文字或代码块围栏。",
        work_group.goal,
        source_message.content,
        task.title,
        task.normalized_goal,
        candidates_text,
        fallback_agent_id.unwrap_or("none"),
    )
}

fn build_owner_workflow_prompt(
    work_group: &WorkGroup,
    source_message: &ConversationMessage,
    members: &[AgentProfile],
) -> String {
    let members_text = members
        .iter()
        .map(|agent| {
            format!(
                "- id: {}\n  name: {}\n  role: {}\n  objective: {}",
                agent.id,
                agent.name,
                agent.role,
                truncate_text(&agent.objective, 160),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "请作为群主，为一个需要协作推进的目标生成工作流计划。\n\
工作组目标：{}\n\
用户原话：{}\n\
可用成员：\n{}\n\
请只输出一个 JSON 对象，字段为 workflowTitle, ownerAckText, ownerPlanText, stages。\n\
stages 是数组，每个元素包含 title, goal, executionMode(serial|parallel), tasks。\n\
tasks 是数组，每个元素包含 title, goal, assigneeAgentId。\n\
规则：\n\
- stages 数量 1 到 {} 个。\n\
- 每个 stage 的 tasks 数量 1 到 {} 个。\n\
- assigneeAgentId 只能使用可用成员里的 id。\n\
- 拆解必须紧贴用户目标，不要默认套固定 PRD/架构/开发/测试 模板，除非任务确实需要。\n\
- ownerAckText 和 ownerPlanText 必须是群主会在主聊天里说的自然中文。\n\
- 不要输出多余字段，不要带 Markdown、解释文字或代码块围栏。",
        work_group.goal,
        source_message.content,
        members_text,
        MAX_OWNER_STAGES,
        MAX_OWNER_TASKS_PER_STAGE,
    )
}

fn build_owner_stage_dispatch_prompt(
    workflow: &WorkflowRecord,
    stage: &WorkflowStageRecord,
    tasks: &[TaskCard],
    members: &[AgentProfile],
) -> String {
    let task_lines = tasks
        .iter()
        .map(|task| {
            let assignee = task
                .assigned_agent_id
                .as_ref()
                .and_then(|agent_id| members.iter().find(|agent| agent.id == *agent_id))
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| "成员".to_string());
            format!("- {} -> {}: {}", assignee, task.title, task.normalized_goal)
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "请作为群主，为即将启动的阶段生成一条派单消息。\n\
工作流：{}\n\
当前阶段：{} / {}\n\
待启动任务：\n{}\n\
请只输出一个 JSON 对象，字段为 dispatchText。\n\
要求：dispatchText 必须是群主在主聊天里说的自然中文，明确点名负责人和任务，不要输出多余字段。",
        workflow.title, stage.title, stage.goal, task_lines,
    )
}

fn build_owner_stage_transition_prompt(
    workflow: &WorkflowRecord,
    completed_stage: &WorkflowStageRecord,
    next_stage: &WorkflowStageRecord,
    completed_summaries: &[String],
) -> String {
    format!(
        "请作为群主，为阶段切换生成一条自然中文消息。\n\
工作流：{}\n\
已完成阶段：{} / {}\n\
上一阶段交付摘要：\n{}\n\
下一阶段：{} / {}\n\
请只输出一个 JSON 对象，字段为 transitionText。\n\
要求：transitionText 要先简短确认上一阶段完成，再自然引出进入下一阶段。",
        workflow.title,
        completed_stage.title,
        completed_stage.goal,
        if completed_summaries.is_empty() {
            "- 暂无交付摘要".to_string()
        } else {
            completed_summaries
                .iter()
                .map(|line| format!("- {}", truncate_text(line, 220)))
                .collect::<Vec<_>>()
                .join("\n")
        },
        next_stage.title,
        next_stage.goal,
    )
}

fn build_owner_summary_prompt(workflow: &WorkflowRecord, delivered_summaries: &[String]) -> String {
    format!(
        "请作为群主，为整个工作流生成最终汇总消息。\n\
工作流标题：{}\n\
用户目标：{}\n\
主要交付：\n{}\n\
请只输出一个 JSON 对象，字段为 summaryText。\n\
要求：summaryText 是群主在主聊天中的最终总结，必须自然、简洁，并概括关键交付结果。",
        workflow.title,
        workflow.normalized_intent,
        if delivered_summaries.is_empty() {
            "- 暂无交付".to_string()
        } else {
            delivered_summaries
                .iter()
                .map(|line| format!("- {}", truncate_text(line, 220)))
                .collect::<Vec<_>>()
                .join("\n")
        },
    )
}

fn owner_workflow_title(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        "群聊工作流".into()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn narrative_text_or_raw(content: &str) -> String {
    serde_json::from_str::<NarrativeEnvelope>(content)
        .map(|envelope| envelope.text)
        .unwrap_or_else(|_| content.to_string())
}

pub(super) fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (end > start).then_some(&trimmed[start..=end])
}

fn compact_title(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "任务".to_string()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn non_empty_or(value: Option<String>, fallback: String) -> String {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or(fallback)
}

pub(super) fn clean_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(super) fn require_text(value: Option<String>, error_message: &str) -> Result<String> {
    clean_optional_text(value).ok_or_else(|| anyhow!(error_message.to_string()))
}

pub(super) fn truncate_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    format!(
        "{}...",
        value
            .chars()
            .take(limit.saturating_sub(3))
            .collect::<String>()
    )
}

fn mock_owner_completion(decision_kind: &str, prompt: &str) -> String {
    match decision_kind {
        "owner.single_task_assignment" => {
            let fallback_id = extract_prompt_value(prompt, "当前系统默认候选：")
                .filter(|value| value != "none");
            let candidate_ids = extract_candidate_ids(prompt);
            let assignee = fallback_id
                .or_else(|| candidate_ids.first().cloned())
                .unwrap_or_default();
            let task_title = extract_prompt_value(prompt, "任务标题：").unwrap_or_else(|| "当前任务".into());
            json!({
                "assigneeAgentId": if assignee.is_empty() { serde_json::Value::Null } else { json!(assignee) },
                "ownerAckText": "收到，我先看下怎么安排更合适。",
                "ownerDispatchText": format!("@{} 你先接手这个任务，先把“{}”处理起来，有结论及时同步。", extract_agent_name_by_id(prompt, &assignee).unwrap_or_else(|| "成员".into()), task_title),
                "ownerBlockerText": "我看了下当前成员配置，暂时没有特别合适的人能直接接这个任务，请先补充成员或限定范围。"
            })
            .to_string()
        }
        "owner.workflow_plan" => {
            let members = extract_candidates(prompt);
            let stage_count = members.len().clamp(1, 3);
            let stages = (0..stage_count)
                .map(|index| {
                    let member = members.get(index).or_else(|| members.first()).cloned().unwrap_or_default();
                    json!({
                        "title": match index {
                            0 => "需求澄清",
                            1 => "方案推进",
                            _ => "结果收敛",
                        },
                        "goal": match index {
                            0 => "先明确目标边界、关键风险和交付预期。",
                            1 => "基于已确认信息推进核心实现或分析。",
                            _ => "汇总产出并整理最终结论。",
                        },
                        "executionMode": if index == 1 && members.len() > 2 { "parallel" } else { "serial" },
                        "tasks": [{
                            "title": format!("{}负责当前阶段", member.name),
                            "goal": format!("请围绕用户目标推进当前阶段工作，并在群里同步关键结论。"),
                            "assigneeAgentId": member.id,
                        }]
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "workflowTitle": extract_prompt_value(prompt, "用户原话：").unwrap_or_else(|| "群聊工作流".into()),
                "ownerAckText": "收到，这个目标我来统筹推进。",
                "ownerPlanText": "我先拆一下阶段和负责人，按当前目标逐步推进，过程中会根据进展动态调整。",
                "stages": stages,
            })
            .to_string()
        }
        "owner.stage_dispatch" => {
            let assignee = extract_first_task_assignee(prompt).unwrap_or_else(|| "成员".into());
            json!({
                "dispatchText": format!("@{} 先开始这一轮任务，按阶段目标推进，遇到关键结论直接在群里同步。", assignee)
            })
            .to_string()
        }
        "owner.stage_transition" => json!({
            "transitionText": "这一阶段的关键结果已经齐了，我来切到下一阶段继续推进。"
        })
        .to_string(),
        "owner.workflow_summary" => json!({
            "summaryText": "这轮任务已经收敛完成，关键产出我已经汇总，后续可以基于当前结果继续展开。"
        })
        .to_string(),
        "owner.blocker_resolution" => mock_owner_blocker_completion(prompt),
        _ => "{}".to_string(),
    }
}

#[derive(Clone, Default)]
pub(super) struct PromptCandidate {
    pub(super) id: String,
    pub(super) name: String,
}

pub(super) fn extract_candidates(prompt: &str) -> Vec<PromptCandidate> {
    let mut result = Vec::new();
    let mut current_id: Option<String> = None;
    for line in prompt.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("- id: ") {
            current_id = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = trimmed
            .strip_prefix("name: ")
            .or_else(|| trimmed.strip_prefix("name:"))
        {
            if let Some(id) = current_id.take() {
                result.push(PromptCandidate {
                    id,
                    name: value.trim().to_string(),
                });
            }
        }
    }
    result
}

fn extract_candidate_ids(prompt: &str) -> Vec<String> {
    extract_candidates(prompt)
        .into_iter()
        .map(|item| item.id)
        .collect()
}

fn extract_agent_name_by_id(prompt: &str, agent_id: &str) -> Option<String> {
    extract_candidates(prompt)
        .into_iter()
        .find(|candidate| candidate.id == agent_id)
        .map(|candidate| candidate.name)
}

pub(super) fn extract_prompt_value(prompt: &str, prefix: &str) -> Option<String> {
    prompt
        .lines()
        .find_map(|line| {
            line.strip_prefix(prefix)
                .map(|value| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
}

fn extract_first_task_assignee(prompt: &str) -> Option<String> {
    prompt
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("- ") {
                return None;
            }
            let rest = trimmed.trim_start_matches("- ");
            rest.split_once(" -> ")
                .map(|(name, _)| name.trim().to_string())
        })
        .filter(|value| !value.is_empty())
}
