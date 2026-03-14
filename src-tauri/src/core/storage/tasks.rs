use anyhow::{Context, Result};
use rusqlite::params;

use super::{collect_rows, decode, json, Storage};
use crate::core::domain::TaskCard;

impl Storage {
    pub fn insert_task_card(&self, task_card: &TaskCard) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO task_cards (
                  id, parent_id, source_message_id, title, normalized_goal, input_payload, priority,
                  status, work_group_id, created_by, assigned_agent_id, output_summary, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
                params![
                    task_card.id,
                    task_card.parent_id,
                    task_card.source_message_id,
                    task_card.title,
                    task_card.normalized_goal,
                    task_card.input_payload,
                    task_card.priority,
                    json(&task_card.status)?,
                    task_card.work_group_id,
                    task_card.created_by,
                    task_card.assigned_agent_id,
                    task_card.output_summary,
                    task_card.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_task_cards(&self, work_group_id: Option<&str>) -> Result<Vec<TaskCard>> {
        self.with_conn(|conn| {
            let sql = if work_group_id.is_some() {
                "SELECT * FROM task_cards WHERE work_group_id = ?1 ORDER BY created_at DESC"
            } else {
                "SELECT * FROM task_cards ORDER BY created_at DESC"
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = if let Some(id) = work_group_id {
                stmt.query_map(params![id], map_task_card)?
            } else {
                stmt.query_map([], map_task_card)?
            };
            collect_rows(rows)
        })
    }

    pub fn get_task_card(&self, task_card_id: &str) -> Result<TaskCard> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM task_cards WHERE id = ?1",
                params![task_card_id],
                map_task_card,
            )
            .context("task card not found")
        })
    }

    pub fn list_child_tasks(&self, parent_id: &str) -> Result<Vec<TaskCard>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT * FROM task_cards WHERE parent_id = ?1 ORDER BY created_at ASC")?;
            let rows = stmt.query_map(params![parent_id], map_task_card)?;
            collect_rows(rows)
        })
    }

    pub fn update_task_card(&self, task_card: &TaskCard) -> Result<()> {
        self.insert_task_card(task_card)
    }
}

fn map_task_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskCard> {
    Ok(TaskCard {
        id: row.get("id")?,
        parent_id: row.get("parent_id")?,
        source_message_id: row.get("source_message_id")?,
        title: row.get("title")?,
        normalized_goal: row.get("normalized_goal")?,
        input_payload: row.get("input_payload")?,
        priority: row.get("priority")?,
        status: decode(row.get("status")?)?,
        work_group_id: row.get("work_group_id")?,
        created_by: row.get("created_by")?,
        assigned_agent_id: row.get("assigned_agent_id")?,
        output_summary: row.get::<_, Option<String>>("output_summary").unwrap_or(None),
        created_at: row.get("created_at")?,
    })
}
