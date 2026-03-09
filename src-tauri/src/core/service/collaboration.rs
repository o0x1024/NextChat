use anyhow::Result;
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::domain::{
    new_id, now, AgentProfile, ConversationMessage, ExecutionMode, MessageKind, SenderKind,
    TaskCard, TaskStatus, Visibility,
};

impl AppService {
    pub(super) fn emit_collaboration_request<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        parent_task: &TaskCard,
        child_task: &TaskCard,
        requester: &AgentProfile,
        collaborator: Option<&AgentProfile>,
    ) -> Result<Option<ConversationMessage>> {
        let message = ConversationMessage {
            id: new_id(),
            conversation_id: child_task.work_group_id.clone(),
            work_group_id: child_task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: requester.id.clone(),
            sender_name: requester.name.clone(),
            kind: MessageKind::Collaboration,
            visibility: Visibility::Backstage,
            content: format!(
                "Collaboration request\nParent task: {}\nChild task: {}\nRequester: {}\nCollaborator: {}\nGoal: {}",
                parent_task.title,
                child_task.title,
                requester.name,
                collaborator
                    .map(|agent| agent.name.as_str())
                    .unwrap_or("Pending assignment"),
                child_task.normalized_goal
            ),
            mentions: collaborator
                .map(|agent| vec![agent.id.clone()])
                .unwrap_or_default(),
            task_card_id: Some(child_task.id.clone()),
            execution_mode: None,
            created_at: now(),
        };
        self.storage.insert_message(&message)?;
        emit(app, "chat:message-created", &message)?;
        self.record_audit(
            "task.collaboration_requested",
            "task_card",
            &child_task.id,
            json!({
                "parentTaskId": parent_task.id,
                "requesterAgentId": requester.id,
                "collaboratorAgentId": collaborator.map(|agent| agent.id.clone()),
            }),
        )?;
        Ok(Some(message))
    }

    pub(super) fn emit_collaboration_result<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        child_task: &TaskCard,
        collaborator: &AgentProfile,
        status: TaskStatus,
        summary: &str,
        execution_mode: Option<ExecutionMode>,
    ) -> Result<Option<ConversationMessage>> {
        let Some(parent_id) = child_task.parent_id.as_deref() else {
            return Ok(None);
        };
        let parent_task = self.storage.get_task_card(parent_id)?;
        let Some(requester) = self.parent_owner_agent(&parent_task)? else {
            return Ok(None);
        };

        let message = ConversationMessage {
            id: new_id(),
            conversation_id: child_task.work_group_id.clone(),
            work_group_id: child_task.work_group_id.clone(),
            sender_kind: SenderKind::Agent,
            sender_id: collaborator.id.clone(),
            sender_name: collaborator.name.clone(),
            kind: MessageKind::Collaboration,
            visibility: Visibility::Backstage,
            content: format!(
                "Collaboration result\nParent task: {}\nChild task: {}\nRequester: {}\nCollaborator: {}\nStatus: {}\nResult: {}",
                parent_task.title,
                child_task.title,
                requester.name,
                collaborator.name,
                collaboration_status_label(&status),
                summary.trim()
            ),
            mentions: vec![requester.id.clone()],
            task_card_id: Some(child_task.id.clone()),
            execution_mode,
            created_at: now(),
        };
        self.storage.insert_message(&message)?;
        emit(app, "chat:message-created", &message)?;
        self.record_audit(
            "task.collaboration_reported",
            "task_card",
            &child_task.id,
            json!({
                "parentTaskId": parent_task.id,
                "requesterAgentId": requester.id,
                "collaboratorAgentId": collaborator.id,
                "status": collaboration_status_label(&status),
            }),
        )?;
        Ok(Some(message))
    }

    fn parent_owner_agent(&self, parent_task: &TaskCard) -> Result<Option<AgentProfile>> {
        if let Some(lease) = self.storage.get_lease_by_task(&parent_task.id)? {
            return Ok(Some(self.storage.get_agent(&lease.owner_agent_id)?));
        }
        if let Some(agent_id) = parent_task.assigned_agent_id.as_deref() {
            return Ok(Some(self.storage.get_agent(agent_id)?));
        }
        Ok(None)
    }
}

fn collaboration_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::NeedsReview => "needs_review",
        TaskStatus::WaitingApproval => "waiting_approval",
        _ => "in_progress",
    }
}
