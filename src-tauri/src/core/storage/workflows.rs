use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};

use super::{collect_rows, decode, json, Storage};
use crate::core::workflow::{
    BlockerStatus, TaskBlockerRecord, TaskDispatchRecord, WorkflowCheckpointRecord, WorkflowRecord,
    WorkflowStageRecord,
};

impl Storage {
    pub fn insert_workflow(&self, workflow: &WorkflowRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO workflows (
                  id, work_group_id, source_message_id, route_mode, title, normalized_intent,
                  status, owner_agent_id, current_stage_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    workflow.id,
                    workflow.work_group_id,
                    workflow.source_message_id,
                    json(&workflow.route_mode)?,
                    workflow.title,
                    workflow.normalized_intent,
                    json(&workflow.status)?,
                    workflow.owner_agent_id,
                    workflow.current_stage_id,
                    workflow.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_workflow(&self, workflow_id: &str) -> Result<WorkflowRecord> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM workflows WHERE id = ?1",
                params![workflow_id],
                map_workflow,
            )
            .context("workflow not found")
        })
    }

    pub fn insert_workflow_stage(&self, stage: &WorkflowStageRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO workflow_stages (
                  id, workflow_id, title, goal, order_index, execution_mode, status,
                  entry_message_id, completion_message_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    stage.id,
                    stage.workflow_id,
                    stage.title,
                    stage.goal,
                    stage.order_index,
                    json(&stage.execution_mode)?,
                    json(&stage.status)?,
                    stage.entry_message_id,
                    stage.completion_message_id,
                    stage.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_workflow_stages(&self, workflow_id: &str) -> Result<Vec<WorkflowStageRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM workflow_stages WHERE workflow_id = ?1 ORDER BY order_index ASC",
            )?;
            let rows = stmt.query_map(params![workflow_id], map_workflow_stage)?;
            collect_rows(rows)
        })
    }

    pub fn get_workflow_stage(&self, stage_id: &str) -> Result<WorkflowStageRecord> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM workflow_stages WHERE id = ?1",
                params![stage_id],
                map_workflow_stage,
            )
            .context("workflow stage not found")
        })
    }

    pub fn insert_task_dispatch(&self, dispatch: &TaskDispatchRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO task_dispatches (
                  task_id, workflow_id, stage_id, dispatch_source, depends_on_task_ids,
                  acknowledged_at, result_message_id, locked_by_user_mention, target_agent_id,
                  route_mode, narrative_stage_label, narrative_task_label
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
                params![
                    dispatch.task_id,
                    dispatch.workflow_id,
                    dispatch.stage_id,
                    json(&dispatch.dispatch_source)?,
                    json(&dispatch.depends_on_task_ids)?,
                    dispatch.acknowledged_at,
                    dispatch.result_message_id,
                    if dispatch.locked_by_user_mention {
                        1_i64
                    } else {
                        0_i64
                    },
                    dispatch.target_agent_id,
                    json(&dispatch.route_mode)?,
                    dispatch.narrative_stage_label,
                    dispatch.narrative_task_label,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_task_dispatch(&self, task_id: &str) -> Result<Option<TaskDispatchRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM task_dispatches WHERE task_id = ?1",
                params![task_id],
                map_task_dispatch,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn list_stage_task_dispatches(&self, stage_id: &str) -> Result<Vec<TaskDispatchRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT * FROM task_dispatches WHERE stage_id = ?1 ORDER BY rowid ASC")?;
            let rows = stmt.query_map(params![stage_id], map_task_dispatch)?;
            collect_rows(rows)
        })
    }

    pub fn list_workflow_task_dispatches(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<TaskDispatchRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM task_dispatches WHERE workflow_id = ?1 ORDER BY rowid ASC",
            )?;
            let rows = stmt.query_map(params![workflow_id], map_task_dispatch)?;
            collect_rows(rows)
        })
    }

    pub fn insert_task_blocker(&self, blocker: &TaskBlockerRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO task_blockers (
                  id, task_id, workflow_id, raised_by_agent_id, resolution_target, category,
                  summary, details, status, created_at, resolved_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    blocker.id,
                    blocker.task_id,
                    blocker.workflow_id,
                    blocker.raised_by_agent_id,
                    json(&blocker.resolution_target)?,
                    json(&blocker.category)?,
                    blocker.summary,
                    blocker.details,
                    json(&blocker.status)?,
                    blocker.created_at,
                    blocker.resolved_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn insert_workflow_checkpoint(&self, checkpoint: &WorkflowCheckpointRecord) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO workflow_checkpoints (
                  id, workflow_id, stage_id, task_id, stage_title, task_title,
                  assignee_agent_id, assignee_name, status, working_directory,
                  repo_snapshot_json, artifact_summary_json, todo_snapshot_json,
                  resume_hint, failure_count, last_error, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                "#,
                params![
                    checkpoint.id,
                    checkpoint.workflow_id,
                    checkpoint.stage_id,
                    checkpoint.task_id,
                    checkpoint.stage_title,
                    checkpoint.task_title,
                    checkpoint.assignee_agent_id,
                    checkpoint.assignee_name,
                    json(&checkpoint.status)?,
                    checkpoint.working_directory,
                    json(&checkpoint.repo_snapshot)?,
                    json(&checkpoint.artifact_summary)?,
                    json(&checkpoint.todo_snapshot)?,
                    checkpoint.resume_hint,
                    checkpoint.failure_count,
                    checkpoint.last_error,
                    checkpoint.created_at,
                    checkpoint.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn latest_workflow_checkpoint_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<WorkflowCheckpointRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM workflow_checkpoints WHERE task_id = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT 1",
                params![task_id],
                map_workflow_checkpoint,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn latest_workflow_checkpoint_for_stage(
        &self,
        stage_id: &str,
    ) -> Result<Option<WorkflowCheckpointRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM workflow_checkpoints WHERE stage_id = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT 1",
                params![stage_id],
                map_workflow_checkpoint,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn latest_workflow_checkpoint(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowCheckpointRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM workflow_checkpoints WHERE workflow_id = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT 1",
                params![workflow_id],
                map_workflow_checkpoint,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn list_workflow_checkpoints(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<WorkflowCheckpointRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM workflow_checkpoints WHERE workflow_id = ?1 ORDER BY updated_at ASC, rowid ASC",
            )?;
            let rows = stmt.query_map(params![workflow_id], map_workflow_checkpoint)?;
            collect_rows(rows)
        })
    }

    pub fn list_all_workflow_checkpoints(&self) -> Result<Vec<WorkflowCheckpointRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM workflow_checkpoints ORDER BY updated_at DESC, rowid DESC",
            )?;
            let rows = stmt.query_map([], map_workflow_checkpoint)?;
            collect_rows(rows)
        })
    }

    pub fn latest_open_blocker_for_task(&self, task_id: &str) -> Result<Option<TaskBlockerRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM task_blockers WHERE task_id = ?1 AND status = ?2 ORDER BY created_at DESC LIMIT 1",
                params![task_id, json(&BlockerStatus::Open)?],
                map_task_blocker,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn get_task_blocker(&self, blocker_id: &str) -> Result<TaskBlockerRecord> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM task_blockers WHERE id = ?1",
                params![blocker_id],
                map_task_blocker,
            )
            .context("task blocker not found")
        })
    }

    pub fn list_task_blockers(&self) -> Result<Vec<TaskBlockerRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM task_blockers ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], map_task_blocker)?;
            collect_rows(rows)
        })
    }
}

fn map_workflow(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRecord> {
    Ok(WorkflowRecord {
        id: row.get("id")?,
        work_group_id: row.get("work_group_id")?,
        source_message_id: row.get("source_message_id")?,
        route_mode: decode(row.get("route_mode")?)?,
        title: row.get("title")?,
        normalized_intent: row.get("normalized_intent")?,
        status: decode(row.get("status")?)?,
        owner_agent_id: row.get("owner_agent_id")?,
        current_stage_id: row.get("current_stage_id")?,
        created_at: row.get("created_at")?,
    })
}

fn map_workflow_stage(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowStageRecord> {
    Ok(WorkflowStageRecord {
        id: row.get("id")?,
        workflow_id: row.get("workflow_id")?,
        title: row.get("title")?,
        goal: row.get("goal")?,
        order_index: row.get("order_index")?,
        execution_mode: decode(row.get("execution_mode")?)?,
        status: decode(row.get("status")?)?,
        entry_message_id: row.get("entry_message_id")?,
        completion_message_id: row.get("completion_message_id")?,
        created_at: row.get("created_at")?,
    })
}

fn map_task_dispatch(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskDispatchRecord> {
    Ok(TaskDispatchRecord {
        task_id: row.get("task_id")?,
        workflow_id: row.get("workflow_id")?,
        stage_id: row.get("stage_id")?,
        dispatch_source: decode(row.get("dispatch_source")?)?,
        depends_on_task_ids: decode(row.get("depends_on_task_ids")?)?,
        acknowledged_at: row.get("acknowledged_at")?,
        result_message_id: row.get("result_message_id")?,
        locked_by_user_mention: row.get::<_, i64>("locked_by_user_mention")? == 1,
        target_agent_id: row.get("target_agent_id")?,
        route_mode: decode(row.get("route_mode")?)?,
        narrative_stage_label: row.get("narrative_stage_label")?,
        narrative_task_label: row.get("narrative_task_label")?,
    })
}

fn map_task_blocker(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskBlockerRecord> {
    Ok(TaskBlockerRecord {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        workflow_id: row.get("workflow_id")?,
        raised_by_agent_id: row.get("raised_by_agent_id")?,
        resolution_target: decode(row.get("resolution_target")?)?,
        category: decode(row.get("category")?)?,
        summary: row.get("summary")?,
        details: row.get("details")?,
        status: decode(row.get("status")?)?,
        created_at: row.get("created_at")?,
        resolved_at: row.get("resolved_at")?,
    })
}

fn map_workflow_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowCheckpointRecord> {
    Ok(WorkflowCheckpointRecord {
        id: row.get("id")?,
        workflow_id: row.get("workflow_id")?,
        stage_id: row.get("stage_id")?,
        task_id: row.get("task_id")?,
        stage_title: row.get("stage_title")?,
        task_title: row.get("task_title")?,
        assignee_agent_id: row.get("assignee_agent_id")?,
        assignee_name: row.get("assignee_name")?,
        status: decode(row.get("status")?)?,
        working_directory: row.get("working_directory")?,
        repo_snapshot: decode(row.get("repo_snapshot_json")?)?,
        artifact_summary: decode(row.get("artifact_summary_json")?)?,
        todo_snapshot: decode(row.get("todo_snapshot_json")?)?,
        resume_hint: row.get("resume_hint")?,
        failure_count: row.get("failure_count")?,
        last_error: row.get("last_error")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}
