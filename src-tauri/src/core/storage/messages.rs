use anyhow::Result;
use rusqlite::params;

use super::{collect_rows, decode, json, Storage};
use crate::core::domain::{ConversationMessage, ExecutionMode};

impl Storage {
    pub fn insert_message(&self, message: &ConversationMessage) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO messages (
                  id, conversation_id, work_group_id, sender_kind, sender_id, sender_name, kind,
                  visibility, content, narrative_meta, mentions, task_card_id, execution_mode, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#,
                params![
                    message.id,
                    message.conversation_id,
                    message.work_group_id,
                    json(&message.sender_kind)?,
                    message.sender_id,
                    message.sender_name,
                    json(&message.kind)?,
                    json(&message.visibility)?,
                    message.content,
                    message.narrative_meta,
                    json(&message.mentions)?,
                    message.task_card_id,
                    message.execution_mode.as_ref().map(json).transpose()?,
                    message.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_messages(&self) -> Result<Vec<ConversationMessage>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM messages ORDER BY created_at ASC")?;
            let rows = stmt.query_map([], map_message)?;
            collect_rows(rows)
        })
    }

    pub fn list_messages_for_group(&self, work_group_id: &str) -> Result<Vec<ConversationMessage>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM messages WHERE work_group_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map(params![work_group_id], map_message)?;
            collect_rows(rows)
        })
    }
}

fn map_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationMessage> {
    Ok(ConversationMessage {
        id: row.get("id")?,
        conversation_id: row.get("conversation_id")?,
        work_group_id: row.get("work_group_id")?,
        sender_kind: decode(row.get("sender_kind")?)?,
        sender_id: row.get("sender_id")?,
        sender_name: row.get("sender_name")?,
        kind: decode(row.get("kind")?)?,
        visibility: decode(row.get("visibility")?)?,
        content: row.get("content")?,
        narrative_meta: row.get("narrative_meta").unwrap_or(None),
        mentions: decode(row.get("mentions")?)?,
        task_card_id: row.get("task_card_id")?,
        execution_mode: row
            .get::<_, Option<String>>("execution_mode")?
            .map(decode::<ExecutionMode>)
            .transpose()?,
        created_at: row.get("created_at")?,
    })
}
