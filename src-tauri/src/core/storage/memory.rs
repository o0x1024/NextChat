use anyhow::Result;
use rusqlite::{params, Row};

use super::{bool_to_i64, collect_rows, decode, json, Storage};
use crate::core::domain::MemoryItem;
use crate::core::memory::filter_active_memory;

impl Storage {
    pub fn list_memory_items(&self) -> Result<Vec<MemoryItem>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM memory_items ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], map_memory_item)?;
            Ok(filter_active_memory(collect_rows(rows)?))
        })
    }

    pub fn insert_memory_item(&self, item: &MemoryItem) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO memory_items (
                  id, scope, scope_id, content, tags, embedding_ref, pinned, ttl, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    item.id,
                    json(&item.scope)?,
                    item.scope_id,
                    item.content,
                    json(&item.tags)?,
                    item.embedding_ref,
                    bool_to_i64(item.pinned),
                    item.ttl,
                    item.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn cleanup_expired_memory_items(&self) -> Result<usize> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM memory_items")?;
            let rows = stmt.query_map([], map_memory_item)?;
            let all_items = collect_rows(rows)?;
            let active_ids = filter_active_memory(all_items.clone())
                .into_iter()
                .map(|item| item.id)
                .collect::<std::collections::HashSet<_>>();
            let expired_ids = all_items
                .into_iter()
                .filter(|item| !active_ids.contains(&item.id))
                .map(|item| item.id)
                .collect::<Vec<_>>();

            for id in &expired_ids {
                conn.execute("DELETE FROM memory_items WHERE id = ?1", params![id])?;
            }

            Ok(expired_ids.len())
        })
    }
}

fn map_memory_item(row: &Row<'_>) -> rusqlite::Result<MemoryItem> {
    Ok(MemoryItem {
        id: row.get("id")?,
        scope: decode(row.get("scope")?)?,
        scope_id: row.get("scope_id")?,
        content: row.get("content")?,
        tags: decode(row.get("tags")?)?,
        embedding_ref: row.get("embedding_ref")?,
        pinned: row.get::<_, i64>("pinned")? == 1,
        ttl: row.get("ttl")?,
        created_at: row.get("created_at")?,
    })
}
