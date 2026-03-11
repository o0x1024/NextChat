use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::json;

use super::{
    block_on_service_future,
    owner_orchestration::{extract_json_object, extract_prompt_value, require_text, truncate_text},
    AppService,
};
use crate::core::domain::{AgentProfile, ModelProviderAdapter, TaskCard};
use crate::core::llm_rig::RigModelAdapter;
use crate::core::workflow::{AgentNarrativeDecision, RequestRouteMode};

impl AppService {
    pub(super) fn build_agent_ack_text(
        &self,
        task: &TaskCard,
        agent: &AgentProfile,
    ) -> Result<String> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let decision =
            block_on_service_future(self.complete_agent_json::<AgentNarrativeDecision>(
                agent,
                "agent.task_ack",
                build_agent_narrative_prompt(task, agent, dispatch.as_ref(), "ack", None),
                json!({
                    "workGroupId": task.work_group_id,
                    "taskId": task.id,
                    "routeMode": dispatch.as_ref().map(|item| item.route_mode.clone()),
                    "actorId": agent.id,
                }),
            ))?;
        require_text(decision.text, "agent ack text missing")
    }

    pub(super) async fn build_agent_progress_decision_async(
        &self,
        task: &TaskCard,
        agent: &AgentProfile,
    ) -> Result<AgentNarrativeDecision> {
        let dispatch = self.storage.get_task_dispatch(&task.id)?;
        let decision = self
            .complete_agent_json::<AgentNarrativeDecision>(
                agent,
                "agent.task_progress",
                build_agent_narrative_prompt(task, agent, dispatch.as_ref(), "progress", None),
                json!({
                    "workGroupId": task.work_group_id,
                    "taskId": task.id,
                    "routeMode": dispatch.as_ref().map(|item| item.route_mode.clone()),
                    "actorId": agent.id,
                }),
            )
            .await?;
        let text = require_text(decision.text, "agent progress text missing")?;
        let progress_percent = decision.progress_percent.map(|value| value.clamp(5, 95));
        Ok(AgentNarrativeDecision {
            text: Some(text),
            progress_percent: progress_percent.or(Some(35)),
        })
    }

    async fn complete_agent_json<T>(
        &self,
        agent: &AgentProfile,
        decision_kind: &str,
        prompt: String,
        audit_context: serde_json::Value,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let settings = self.storage.get_settings()?;
        let preamble = format!(
            "你是工作群里的执行成员。你的职责是围绕当前任务，用自然中文给出真实的群聊叙事消息。\
你必须只返回 JSON，不要输出 Markdown、代码块或解释。\
不要编造不存在的成员、工具结果或完成状态。\
成员身份信息：{} / {} / {}。",
            agent.name, agent.role, agent.objective
        );

        let raw =
            if agent.model_policy.provider == "mock" || agent.model_policy.model == "simulation" {
                mock_agent_narrative_completion(decision_kind, &prompt)
            } else {
                RigModelAdapter
                    .complete(&agent.model_policy, &settings, &preamble, &prompt)
                    .await?
                    .ok_or_else(|| anyhow!("agent model is unavailable or not configured"))?
            };

        let Some(payload) = extract_json_object(&raw) else {
            self.record_audit(
                "agent.narrative.parse_failed",
                "agent_narrative",
                decision_kind,
                json!({
                    "agentId": agent.id,
                    "provider": agent.model_policy.provider,
                    "model": agent.model_policy.model,
                    "prompt": truncate_text(&prompt, 4_000),
                    "raw": truncate_text(&raw, 8_000),
                    "context": audit_context,
                }),
            )?;
            return Err(anyhow!("agent model did not return valid JSON"));
        };

        match serde_json::from_str::<T>(payload) {
            Ok(parsed) => {
                self.record_audit(
                    "agent.narrative.generated",
                    "agent_narrative",
                    decision_kind,
                    json!({
                        "agentId": agent.id,
                        "provider": agent.model_policy.provider,
                        "model": agent.model_policy.model,
                        "prompt": truncate_text(&prompt, 4_000),
                        "raw": truncate_text(payload, 8_000),
                        "context": audit_context,
                    }),
                )?;
                Ok(parsed)
            }
            Err(error) => {
                self.record_audit(
                    "agent.narrative.parse_failed",
                    "agent_narrative",
                    decision_kind,
                    json!({
                        "agentId": agent.id,
                        "provider": agent.model_policy.provider,
                        "model": agent.model_policy.model,
                        "error": error.to_string(),
                        "prompt": truncate_text(&prompt, 4_000),
                        "raw": truncate_text(payload, 8_000),
                        "context": audit_context,
                    }),
                )?;
                Err(error).context("failed to parse agent narrative JSON")
            }
        }
    }
}

fn build_agent_narrative_prompt(
    task: &TaskCard,
    agent: &AgentProfile,
    dispatch: Option<&crate::core::workflow::TaskDispatchRecord>,
    message_kind: &str,
    current_summary: Option<&str>,
) -> String {
    let route_mode = dispatch
        .as_ref()
        .map(|item| match item.route_mode {
            RequestRouteMode::OwnerOrchestrated => "owner_orchestrated",
            RequestRouteMode::DirectAgentAssign => "direct_agent_assign",
            RequestRouteMode::DirectAnswer => "direct_answer",
        })
        .unwrap_or("unknown");
    format!(
        "请以执行成员身份生成一条群聊消息。\n\
只输出 JSON，对象字段为 text, progressPercent。\n\
规则：\n\
- text 必填，必须是自然中文，第一人称口吻。\n\
- 如果 messageKind=ack，表示刚接单，应该说明先做什么；progressPercent 返回 null。\n\
- 如果 messageKind=progress，表示任务已开始处理中，应该说明当前关注点；progressPercent 返回 5 到 95 的整数。\n\
- 不要假装任务已经完成，不要杜撰结果。\n\
agentName：{}\n\
agentRole：{}\n\
taskTitle：{}\n\
taskGoal：{}\n\
messageKind：{}\n\
routeMode：{}\n\
workflowId：{}\n\
stageId：{}\n\
stageTitle：{}\n\
taskLabel：{}\n\
currentSummary：{}\n",
        agent.name,
        agent.role,
        task.title,
        task.normalized_goal,
        message_kind,
        route_mode,
        dispatch
            .and_then(|item| item.workflow_id.clone())
            .unwrap_or_else(|| "none".into()),
        dispatch
            .and_then(|item| item.stage_id.clone())
            .unwrap_or_else(|| "none".into()),
        dispatch
            .and_then(|item| item.narrative_stage_label.clone())
            .unwrap_or_else(|| "none".into()),
        dispatch
            .and_then(|item| item.narrative_task_label.clone())
            .unwrap_or_else(|| task.title.clone()),
        current_summary.unwrap_or("none"),
    )
}

fn mock_agent_narrative_completion(decision_kind: &str, prompt: &str) -> String {
    let task_title =
        extract_prompt_value(prompt, "taskTitle：").unwrap_or_else(|| "当前任务".into());
    let route_mode =
        extract_prompt_value(prompt, "routeMode：").unwrap_or_else(|| "owner_orchestrated".into());
    match decision_kind {
        "agent.task_ack" => {
            let text = if route_mode == "owner_orchestrated" {
                format!(
                    "收到，我先处理{}，先把关键点梳理清楚，再同步给群主。",
                    task_title
                )
            } else {
                format!("收到，我来处理{}，先从关键问题开始推进。", task_title)
            };
            json!({ "text": text, "progressPercent": serde_json::Value::Null }).to_string()
        }
        "agent.task_progress" => {
            let text = if route_mode == "owner_orchestrated" {
                format!(
                    "我正在处理{}，先聚焦关键问题，进展会同步给群主。",
                    task_title
                )
            } else {
                format!(
                    "我正在处理{}，先推进关键部分，有进展会直接同步。",
                    task_title
                )
            };
            json!({ "text": text, "progressPercent": 35 }).to_string()
        }
        _ => "{}".to_string(),
    }
}
