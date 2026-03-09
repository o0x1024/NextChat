use anyhow::Result;
use rusqlite::{params, Row};

use super::{collect_rows, decode, json, Storage};
use crate::core::domain::ClaimBid;

impl Storage {
    pub fn insert_claim_bid(&self, bid: &ClaimBid) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO claim_bids (
                  id, task_card_id, agent_id, rationale, capability_score, score_breakdown, expected_tools, estimated_cost, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    bid.id,
                    bid.task_card_id,
                    bid.agent_id,
                    bid.rationale,
                    bid.capability_score,
                    json(&bid.score_breakdown)?,
                    json(&bid.expected_tools)?,
                    bid.estimated_cost,
                    bid.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_claim_bids(&self) -> Result<Vec<ClaimBid>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM claim_bids ORDER BY created_at ASC")?;
            let rows = stmt.query_map([], map_claim_bid)?;
            collect_rows(rows)
        })
    }
}

fn map_claim_bid(row: &Row<'_>) -> rusqlite::Result<ClaimBid> {
    Ok(ClaimBid {
        id: row.get("id")?,
        task_card_id: row.get("task_card_id")?,
        agent_id: row.get("agent_id")?,
        rationale: row.get("rationale")?,
        capability_score: row.get("capability_score")?,
        score_breakdown: decode(row.get("score_breakdown")?)?,
        expected_tools: decode(row.get("expected_tools")?)?,
        estimated_cost: row.get("estimated_cost")?,
        created_at: row.get("created_at")?,
    })
}
