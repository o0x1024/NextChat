use anyhow::{anyhow, Result};
use serde_json::json;

use super::AppService;
use crate::core::domain::{new_id, now, TaskStatus};
use crate::core::workflow::{StageStatus, WorkflowExecutionMode, WorkflowStageRecord, WorkflowStatus};

/// Input for adding a new stage to a running workflow.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddWorkflowStageInput {
    pub workflow_id: String,
    pub title: String,
    pub goal: String,
    /// Insert after the stage with this ID. None = append at end.
    pub after_stage_id: Option<String>,
    pub execution_mode: WorkflowExecutionMode,
}

/// Input for updating an existing pending/ready stage.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkflowStageInput {
    pub stage_id: String,
    pub title: Option<String>,
    pub goal: Option<String>,
    pub execution_mode: Option<WorkflowExecutionMode>,
}

impl AppService {
    /// Add a new stage to an existing workflow at a specified position.
    /// Can only be called on a workflow that is not yet completed or cancelled.
    pub fn add_workflow_stage(&self, input: AddWorkflowStageInput) -> Result<WorkflowStageRecord> {
        let workflow = self.storage.get_workflow(&input.workflow_id)?;
        if matches!(
            workflow.status,
            WorkflowStatus::Completed | WorkflowStatus::Cancelled
        ) {
            return Err(anyhow!(
                "Cannot add stage to a {:?} workflow",
                workflow.status
            ));
        }

        let mut stages = self.storage.list_workflow_stages(&input.workflow_id)?;

        // Determine insertion position (order_index of the "after" stage)
        let insert_after_index = if let Some(ref after_id) = input.after_stage_id {
            stages
                .iter()
                .find(|s| &s.id == after_id)
                .map(|s| s.order_index)
                .ok_or_else(|| anyhow!("Stage '{}' not found in workflow", after_id))?
        } else {
            // Append after the last stage
            stages.iter().map(|s| s.order_index).max().unwrap_or(0)
        };

        // Shift all stages that come after the insertion point by +1
        for stage in stages.iter_mut() {
            if stage.order_index > insert_after_index {
                stage.order_index += 1;
                self.storage.insert_workflow_stage(stage)?;
            }
        }

        let new_stage = WorkflowStageRecord {
            id: new_id(),
            workflow_id: input.workflow_id.clone(),
            title: input.title,
            goal: input.goal,
            order_index: insert_after_index + 1,
            execution_mode: input.execution_mode,
            status: StageStatus::Pending,
            entry_message_id: None,
            completion_message_id: None,
            deliverables_json: None,
            quality_gate_json: None,
            created_at: now(),
        };
        self.storage.insert_workflow_stage(&new_stage)?;

        self.record_audit(
            "workflow.stage_added",
            "workflow_stage",
            &new_stage.id,
            json!({
                "workflowId": input.workflow_id,
                "title": new_stage.title,
                "orderIndex": new_stage.order_index,
            }),
        )?;

        Ok(new_stage)
    }

    /// Update properties of a stage that has not yet started running.
    pub fn update_workflow_stage(
        &self,
        input: UpdateWorkflowStageInput,
    ) -> Result<WorkflowStageRecord> {
        let mut stage = self.storage.get_workflow_stage(&input.stage_id)?;

        if matches!(
            stage.status,
            StageStatus::Running | StageStatus::Completed | StageStatus::Cancelled
        ) {
            return Err(anyhow!(
                "Cannot update a stage with status {:?}",
                stage.status
            ));
        }

        if let Some(title) = input.title {
            stage.title = title;
        }
        if let Some(goal) = input.goal {
            stage.goal = goal;
        }
        if let Some(mode) = input.execution_mode {
            stage.execution_mode = mode;
        }

        self.storage.insert_workflow_stage(&stage)?;

        self.record_audit(
            "workflow.stage_updated",
            "workflow_stage",
            &stage.id,
            json!({
                "workflowId": stage.workflow_id,
                "title": stage.title,
            }),
        )?;

        Ok(stage)
    }

    /// Remove a pending or ready stage (and its pending tasks) from a workflow.
    pub fn remove_workflow_stage(&self, stage_id: &str) -> Result<()> {
        let stage = self.storage.get_workflow_stage(stage_id)?;

        if matches!(stage.status, StageStatus::Running | StageStatus::Completed) {
            return Err(anyhow!(
                "Cannot remove a {:?} stage — only pending/ready stages can be removed",
                stage.status
            ));
        }

        // Cancel any pending tasks associated with this stage
        let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
        for dispatch in &dispatches {
            if let Ok(mut task) = self.storage.get_task_card(&dispatch.task_id) {
                if !matches!(task.status, TaskStatus::Completed) {
                    task.status = TaskStatus::Cancelled;
                    self.storage.update_task_card(&task)?;
                }
            }
        }

        // Soft-delete: mark as Cancelled (preserves audit trail)
        let mut cancelled = stage;
        cancelled.status = StageStatus::Cancelled;
        self.storage.insert_workflow_stage(&cancelled)?;

        self.record_audit(
            "workflow.stage_removed",
            "workflow_stage",
            stage_id,
            json!({ "workflowId": cancelled.workflow_id }),
        )?;

        Ok(())
    }
}
