use anyhow::{Context, Result};
use rusqlite::params;

use super::{bool_to_i64, json, map_work_group, Storage};
use crate::core::domain::{MemoryScope, WorkGroup};

impl Storage {
    pub fn get_work_group_owner_id(&self, work_group_id: &str) -> Result<Option<String>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT owner_agent_id FROM work_groups WHERE id = ?1",
                params![work_group_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .context("work group not found")
            .map_err(Into::into)
        })
    }

    pub fn set_work_group_owner(&self, work_group_id: &str, owner_agent_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE work_groups SET owner_agent_id = ?1 WHERE id = ?2",
                params![owner_agent_id, work_group_id],
            )?;
            if updated == 0 {
                anyhow::bail!("work group not found");
            }
            Ok(())
        })
    }

    pub fn insert_work_group(&self, work_group: &WorkGroup) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO work_groups (
                  id, kind, name, goal, working_directory, member_agent_ids, default_visibility, auto_archive, created_at, archived_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                  kind = excluded.kind,
                  name = excluded.name,
                  goal = excluded.goal,
                  working_directory = excluded.working_directory,
                  member_agent_ids = excluded.member_agent_ids,
                  default_visibility = excluded.default_visibility,
                  auto_archive = excluded.auto_archive,
                  created_at = excluded.created_at,
                  archived_at = excluded.archived_at
                "#,
                params![
                    work_group.id,
                    json(&work_group.kind)?,
                    work_group.name,
                    work_group.goal,
                    work_group.working_directory,
                    json(&work_group.member_agent_ids)?,
                    work_group.default_visibility,
                    bool_to_i64(work_group.auto_archive),
                    work_group.created_at,
                    work_group.archived_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_work_groups(&self) -> Result<Vec<WorkGroup>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM work_groups ORDER BY created_at ASC")?;
            let rows = stmt.query_map([], map_work_group)?;
            super::collect_rows(rows)
        })
    }

    pub fn get_work_group(&self, work_group_id: &str) -> Result<WorkGroup> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM work_groups WHERE id = ?1",
                params![work_group_id],
                map_work_group,
            )
            .context("work group not found")
        })
    }

    pub fn delete_work_group(&self, work_group_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let task_scope = json(&MemoryScope::Task)?;
            let work_group_scope = json(&MemoryScope::WorkGroup)?;
            conn.execute(
                "DELETE FROM memory_items
                 WHERE scope = ?1
                 AND scope_id IN (SELECT id FROM task_cards WHERE work_group_id = ?2)",
                params![task_scope, work_group_id],
            )?;
            conn.execute(
                "DELETE FROM memory_items WHERE scope = ?1 AND scope_id = ?2",
                params![work_group_scope, work_group_id],
            )?;
            conn.execute(
                "DELETE FROM tool_runs
                 WHERE task_card_id IN (SELECT id FROM task_cards WHERE work_group_id = ?1)",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM claim_bids
                 WHERE task_card_id IN (SELECT id FROM task_cards WHERE work_group_id = ?1)",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM leases
                 WHERE task_card_id IN (SELECT id FROM task_cards WHERE work_group_id = ?1)",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM task_cards WHERE work_group_id = ?1",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM messages WHERE work_group_id = ?1",
                params![work_group_id],
            )?;
            let deleted = conn.execute(
                "DELETE FROM work_groups WHERE id = ?1",
                params![work_group_id],
            )?;
            if deleted == 0 {
                anyhow::bail!("work group not found");
            }
            Ok(())
        })
    }

    pub fn clear_work_group_history(&self, work_group_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM work_groups WHERE id = ?1",
                params![work_group_id],
                |row| row.get(0),
            )?;
            if exists == 0 {
                anyhow::bail!("work group not found");
            }
            let task_scope = json(&MemoryScope::Task)?;

            conn.execute(
                "DELETE FROM memory_items
                 WHERE scope = ?1
                 AND scope_id IN (SELECT id FROM task_cards WHERE work_group_id = ?2)",
                params![task_scope, work_group_id],
            )?;
            conn.execute(
                "DELETE FROM tool_runs
                 WHERE task_card_id IN (SELECT id FROM task_cards WHERE work_group_id = ?1)",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM claim_bids
                 WHERE task_card_id IN (SELECT id FROM task_cards WHERE work_group_id = ?1)",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM leases
                 WHERE task_card_id IN (SELECT id FROM task_cards WHERE work_group_id = ?1)",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM task_cards WHERE work_group_id = ?1",
                params![work_group_id],
            )?;
            conn.execute(
                "DELETE FROM messages WHERE work_group_id = ?1",
                params![work_group_id],
            )?;
            Ok(())
        })
    }

    pub fn add_agent_to_work_group(
        &self,
        work_group_id: &str,
        agent_id: &str,
    ) -> Result<WorkGroup> {
        let mut group = self.get_work_group(work_group_id)?;
        if !group.member_agent_ids.contains(&agent_id.to_string()) {
            group.member_agent_ids.push(agent_id.to_string());
            self.insert_work_group(&group)?;
        }
        Ok(group)
    }

    pub fn remove_agent_from_work_group(
        &self,
        work_group_id: &str,
        agent_id: &str,
    ) -> Result<WorkGroup> {
        if self
            .get_work_group_owner_id(work_group_id)?
            .as_deref()
            .is_some_and(|owner_id| owner_id == agent_id)
        {
            anyhow::bail!("cannot remove work group owner");
        }
        let mut group = self.get_work_group(work_group_id)?;
        group.member_agent_ids.retain(|current| current != agent_id);
        self.insert_work_group(&group)?;
        Ok(group)
    }
}
