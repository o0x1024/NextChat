use anyhow::{Context, Result};

use super::AppService;
use crate::core::domain::{
    new_id, now, AgentProfile, ConversationMessage, MessageKind, SenderKind,
};
use crate::core::workflow::{
    BlockerResolutionTarget, NarrativeEnvelope, NarrativeMessageType, NarrativeStageSummary,
    WorkflowPlan,
};

impl AppService {
    pub(super) fn owner_plan_message(&self, plan: &WorkflowPlan) -> Result<ConversationMessage> {
        let stages = plan
            .stages
            .iter()
            .map(|item| NarrativeStageSummary {
                id: item.stage.id.clone(),
                title: item.stage.title.clone(),
                goal: item.stage.goal.clone(),
                execution_mode: item.stage.execution_mode.clone(),
                status: item.stage.status.clone(),
                agents: item
                    .tasks
                    .iter()
                    .map(|task| task.assignee_agent_id.clone())
                    .collect(),
            })
            .collect::<Vec<_>>();
        let mut envelope = NarrativeEnvelope::new(
            NarrativeMessageType::OwnerPlan,
            plan.owner_plan_text
                .clone()
                .context("owner workflow plan missing owner plan text")?,
        );
        envelope.workflow_id = Some(plan.workflow.id.clone());
        envelope.stages = Some(stages);
        self.owner_message_from_envelope(
            &plan.workflow.work_group_id,
            envelope,
            MessageKind::Summary,
        )
    }

    pub(super) fn owner_message_from_envelope(
        &self,
        work_group_id: &str,
        envelope: NarrativeEnvelope,
        kind: MessageKind,
    ) -> Result<ConversationMessage> {
        let mut message = ConversationMessage {
            id: new_id(),
            conversation_id: work_group_id.to_string(),
            work_group_id: work_group_id.to_string(),
            sender_kind: SenderKind::System,
            sender_id: "coordinator".into(),
            sender_name: "Coordinator".into(),
            kind,
            visibility: crate::core::domain::Visibility::Main,
            content: serde_json::to_string(&envelope)?,
            mentions: vec![],
            task_card_id: envelope.task_id.clone(),
            execution_mode: None,
            created_at: now(),
        };
        self.assign_group_owner_sender(&mut message)?;
        Ok(message)
    }

    pub(super) fn agent_message_from_envelope(
        &self,
        work_group_id: &str,
        agent: &AgentProfile,
        envelope: NarrativeEnvelope,
        kind: MessageKind,
        task_card_id: Option<String>,
    ) -> Result<ConversationMessage> {
        Ok(ConversationMessage {
            id: new_id(),
            conversation_id: work_group_id.to_string(),
            work_group_id: work_group_id.to_string(),
            sender_kind: SenderKind::Agent,
            sender_id: agent.id.clone(),
            sender_name: agent.name.clone(),
            kind,
            visibility: crate::core::domain::Visibility::Main,
            content: serde_json::to_string(&envelope)?,
            mentions: vec![],
            task_card_id,
            execution_mode: None,
            created_at: now(),
        })
    }
}

pub(super) fn render_question_text(
    question: &str,
    options: &[String],
    context: Option<&str>,
    resolution_target: &BlockerResolutionTarget,
) -> String {
    let mut lines = vec![match resolution_target {
        BlockerResolutionTarget::Owner => format!("当前任务存在阻塞，需要协调：{question}"),
        BlockerResolutionTarget::User => format!("当前任务缺少关键信息，请确认：{question}"),
    }];
    if let Some(context) = context.filter(|value| !value.trim().is_empty()) {
        lines.push(context.to_string());
    }
    if !options.is_empty() {
        lines.push(format!("可选项：{}", options.join(" / ")));
    }
    lines.join("\n")
}
