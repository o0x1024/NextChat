use anyhow::{anyhow, Context, Result};
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::{emit, AppService};
use crate::core::domain::{
    now, LeaseState, MessageKind, TaskStatus,
};
use crate::core::workflow::{
    BlockerStatus, NarrativeEnvelope, NarrativeMessageType, StageStatus, WorkflowRecord,
    WorkflowStageRecord, WorkflowStatus,
};

impl AppService {
    /// Cancel an entire workflow: marks all non-completed tasks as cancelled,
    /// releases their leases, and sets the workflow + remaining stages to Cancelled.
    pub fn cancel_workflow<R: Runtime>(
        &self,
        app: AppHandle<R>,
        workflow_id: &str,
    ) -> Result<WorkflowRecord> {
        let mut workflow = self.storage.get_workflow(workflow_id)?;
        if matches!(
            workflow.status,
            WorkflowStatus::Completed | WorkflowStatus::Cancelled
        ) {
            return Err(anyhow!(
                "workflow is already {:?} and cannot be cancelled",
                workflow.status
            ));
        }

        // Cancel all non-completed stages and their tasks
        let stages = self.storage.list_workflow_stages(workflow_id)?;
        for mut stage in stages {
            if !matches!(
                stage.status,
                StageStatus::Completed | StageStatus::Cancelled
            ) {
                self.cancel_stage_tasks(&app, &stage.id)?;
                stage.status = StageStatus::Cancelled;
                self.storage.insert_workflow_stage(&stage)?;
            }
        }

        // Cancel open blockers
        self.cancel_workflow_open_blockers(workflow_id)?;

        workflow.status = WorkflowStatus::Cancelled;
        workflow.current_stage_id = None;
        self.storage.insert_workflow(&workflow)?;

        // Emit narrative message
        let mut envelope = NarrativeEnvelope::new(
            NarrativeMessageType::OwnerSummary,
            "工作流已被用户取消。".to_string(),
        );
        envelope.workflow_id = Some(workflow.id.clone());
        let message = self.owner_message_from_envelope(
            &workflow.work_group_id,
            envelope,
            MessageKind::Status,
        )?;
        self.storage.insert_message(&message)?;
        emit(&app, "chat:message-created", &message)?;

        self.record_audit(
            "workflow.cancelled",
            "workflow",
            &workflow.id,
            json!({ "title": workflow.title }),
        )?;

        Ok(workflow)
    }

    /// Pause a running workflow: pauses all active tasks and their leases,
    /// sets the workflow status to NeedsUserInput (paused by user).
    pub fn pause_workflow<R: Runtime>(
        &self,
        app: AppHandle<R>,
        workflow_id: &str,
    ) -> Result<WorkflowRecord> {
        let mut workflow = self.storage.get_workflow(workflow_id)?;
        if !matches!(workflow.status, WorkflowStatus::Running) {
            return Err(anyhow!(
                "only running workflows can be paused, current status: {:?}",
                workflow.status
            ));
        }

        // Pause all active tasks in the current stage
        if let Some(stage_id) = workflow.current_stage_id.as_deref() {
            let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
            for dispatch in &dispatches {
                if let Ok(mut task) = self.storage.get_task_card(&dispatch.task_id) {
                    if matches!(
                        task.status,
                        TaskStatus::Leased | TaskStatus::InProgress | TaskStatus::Bidding
                    ) {
                        task.status = TaskStatus::Paused;
                        self.storage.update_task_card(&task)?;
                        emit(&app, "task:status-changed", &task)?;

                        if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                            if matches!(lease.state, LeaseState::Active) {
                                lease.state = LeaseState::Paused;
                                self.storage.update_lease(&lease)?;
                            }
                        }
                    }
                }
            }
        }

        workflow.status = WorkflowStatus::NeedsUserInput;
        self.storage.insert_workflow(&workflow)?;

        let mut envelope = NarrativeEnvelope::new(
            NarrativeMessageType::OwnerSummary,
            "工作流已暂停，等待用户继续。".to_string(),
        );
        envelope.workflow_id = Some(workflow.id.clone());
        let message = self.owner_message_from_envelope(
            &workflow.work_group_id,
            envelope,
            MessageKind::Status,
        )?;
        self.storage.insert_message(&message)?;
        emit(&app, "chat:message-created", &message)?;

        self.record_audit(
            "workflow.paused",
            "workflow",
            &workflow.id,
            json!({ "title": workflow.title }),
        )?;

        Ok(workflow)
    }

    /// Resume a paused/blocked workflow: resumes paused tasks and sets workflow back to Running.
    pub fn resume_workflow<R: Runtime>(
        &self,
        app: AppHandle<R>,
        workflow_id: &str,
    ) -> Result<WorkflowRecord> {
        let mut workflow = self.storage.get_workflow(workflow_id)?;
        if !matches!(
            workflow.status,
            WorkflowStatus::NeedsUserInput | WorkflowStatus::Blocked
        ) {
            return Err(anyhow!(
                "only paused or blocked workflows can be resumed, current status: {:?}",
                workflow.status
            ));
        }

        // Resume paused tasks in the current stage
        if let Some(stage_id) = workflow.current_stage_id.as_deref() {
            let mut stage = self.storage.get_workflow_stage(stage_id)?;
            let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
            for dispatch in &dispatches {
                if let Ok(mut task) = self.storage.get_task_card(&dispatch.task_id) {
                    if matches!(task.status, TaskStatus::Paused) {
                        task.status = TaskStatus::Leased;
                        self.storage.update_task_card(&task)?;
                        emit(&app, "task:status-changed", &task)?;

                        if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                            lease.state = LeaseState::Active;
                            lease.preempt_requested_at = None;
                            self.storage.update_lease(&lease)?;
                            emit(&app, "lease:granted", &lease)?;
                        }

                        self.spawn_task_execution(app.clone(), task.id.clone(), None);
                    }
                }
            }

            if matches!(stage.status, StageStatus::Blocked) {
                stage.status = StageStatus::Running;
                self.storage.insert_workflow_stage(&stage)?;
            }
        }

        workflow.status = WorkflowStatus::Running;
        self.storage.insert_workflow(&workflow)?;

        let mut envelope = NarrativeEnvelope::new(
            NarrativeMessageType::OwnerStageTransition,
            "工作流已恢复执行。".to_string(),
        );
        envelope.workflow_id = Some(workflow.id.clone());
        let message = self.owner_message_from_envelope(
            &workflow.work_group_id,
            envelope,
            MessageKind::Status,
        )?;
        self.storage.insert_message(&message)?;
        emit(&app, "chat:message-created", &message)?;

        self.record_audit(
            "workflow.resumed",
            "workflow",
            &workflow.id,
            json!({ "title": workflow.title }),
        )?;

        Ok(workflow)
    }

    /// Skip a stage: marks it as completed and advances to the next stage.
    pub fn skip_workflow_stage<R: Runtime>(
        &self,
        app: AppHandle<R>,
        workflow_id: &str,
        stage_id: &str,
    ) -> Result<WorkflowStageRecord> {
        let mut workflow = self.storage.get_workflow(workflow_id)?;
        let mut stage = self.storage.get_workflow_stage(stage_id)?;

        if stage.workflow_id != workflow_id {
            return Err(anyhow!("stage does not belong to this workflow"));
        }
        if matches!(stage.status, StageStatus::Completed | StageStatus::Cancelled) {
            return Err(anyhow!("stage is already completed or cancelled"));
        }

        // Cancel all non-completed tasks in this stage
        self.cancel_stage_tasks(&app, stage_id)?;

        stage.status = StageStatus::Completed;
        self.storage.insert_workflow_stage(&stage)?;

        // Emit skip narrative
        let mut envelope = NarrativeEnvelope::new(
            NarrativeMessageType::OwnerStageTransition,
            format!("用户跳过了阶段「{}」。", stage.title),
        );
        envelope.workflow_id = Some(workflow.id.clone());
        envelope.stage_id = Some(stage.id.clone());
        envelope.stage_title = Some(stage.title.clone());
        let message = self.owner_message_from_envelope(
            &workflow.work_group_id,
            envelope,
            MessageKind::Status,
        )?;
        self.storage.insert_message(&message)?;
        emit(&app, "chat:message-created", &message)?;

        // Advance to next stage
        let stages = self.storage.list_workflow_stages(workflow_id)?;
        let next_stage = stages
            .iter()
            .find(|candidate| candidate.order_index > stage.order_index)
            .cloned();

        if let Some(next_stage) = next_stage {
            workflow.current_stage_id = Some(next_stage.id.clone());
            workflow.status = WorkflowStatus::Running;
            self.storage.insert_workflow(&workflow)?;

            self.activate_next_stage(&app, workflow_id, &next_stage.id)?;
        } else {
            // No more stages: workflow is complete
            workflow.status = WorkflowStatus::Completed;
            workflow.current_stage_id = None;
            self.storage.insert_workflow(&workflow)?;

            let mut summary = NarrativeEnvelope::new(
                NarrativeMessageType::OwnerSummary,
                self.build_owner_workflow_summary_text(&workflow)?,
            );
            summary.workflow_id = Some(workflow.id.clone());
            let msg = self.owner_message_from_envelope(
                &workflow.work_group_id,
                summary,
                MessageKind::Summary,
            )?;
            self.storage.insert_message(&msg)?;
            emit(&app, "chat:message-created", &msg)?;
        }

        self.record_audit(
            "workflow.stage_skipped",
            "workflow_stage",
            &stage.id,
            json!({ "workflowId": workflow_id, "stageTitle": stage.title }),
        )?;

        Ok(stage)
    }

    // --- Helpers ---

    fn cancel_stage_tasks<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        stage_id: &str,
    ) -> Result<()> {
        let dispatches = self.storage.list_stage_task_dispatches(stage_id)?;
        for dispatch in &dispatches {
            if let Ok(mut task) = self.storage.get_task_card(&dispatch.task_id) {
                if !matches!(
                    task.status,
                    TaskStatus::Completed | TaskStatus::Cancelled
                ) {
                    task.status = TaskStatus::Cancelled;
                    self.storage.update_task_card(&task)?;
                    emit(app, "task:status-changed", &task)?;

                    if let Some(mut lease) = self.storage.get_lease_by_task(&task.id)? {
                        if !matches!(lease.state, LeaseState::Released) {
                            lease.state = LeaseState::Released;
                            lease.released_at = Some(now());
                            self.storage.update_lease(&lease)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn cancel_workflow_open_blockers(&self, workflow_id: &str) -> Result<()> {
        let blockers = self.storage.list_task_blockers()?;
        for mut blocker in blockers {
            if blocker.workflow_id.as_deref() == Some(workflow_id)
                && matches!(blocker.status, BlockerStatus::Open)
            {
                blocker.status = BlockerStatus::Cancelled;
                blocker.resolved_at = Some(now());
                self.storage.insert_task_blocker(&blocker)?;
            }
        }
        Ok(())
    }
}
