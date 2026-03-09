use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Serialize};

use crate::core::domain::{
    new_id, now, AgentProfile, AuditEvent, ClaimBid, ConversationMessage, DashboardState, Lease,
    LeaseState, MemoryItem, MemoryPolicy, MemoryScope, MessageKind, ModelPolicy, SenderKind,
    SystemSettings, TaskCard, ToolRun, Visibility, WorkGroup, WorkGroupKind,
};

#[derive(Clone, Debug)]
pub struct Storage {
    db_path: PathBuf,
}

impl Storage {
    pub fn new(app_data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&app_data_dir)?;
        let storage = Self {
            db_path: app_data_dir.join("nextchat.sqlite3"),
        };
        storage.init()?;
        storage.seed_demo_data()?;
        Ok(storage)
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = Connection::open(&self.db_path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        f(&conn)
    }

    fn init(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS agents (
                  id TEXT PRIMARY KEY,
                  name TEXT NOT NULL,
                  avatar TEXT NOT NULL,
                  role TEXT NOT NULL,
                  objective TEXT NOT NULL,
                  model_policy TEXT NOT NULL,
                  skill_ids TEXT NOT NULL,
                  tool_ids TEXT NOT NULL,
                  max_parallel_runs INTEGER NOT NULL,
                  can_spawn_subtasks INTEGER NOT NULL,
                  memory_policy TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS work_groups (
                  id TEXT PRIMARY KEY,
                  kind TEXT NOT NULL,
                  name TEXT NOT NULL,
                  goal TEXT NOT NULL,
                  member_agent_ids TEXT NOT NULL,
                  default_visibility TEXT NOT NULL,
                  auto_archive INTEGER NOT NULL,
                  created_at TEXT NOT NULL,
                  archived_at TEXT
                );

                CREATE TABLE IF NOT EXISTS messages (
                  id TEXT PRIMARY KEY,
                  conversation_id TEXT NOT NULL,
                  work_group_id TEXT NOT NULL,
                  sender_kind TEXT NOT NULL,
                  sender_id TEXT NOT NULL,
                  sender_name TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  visibility TEXT NOT NULL,
                  content TEXT NOT NULL,
                  mentions TEXT NOT NULL,
                  task_card_id TEXT,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS task_cards (
                  id TEXT PRIMARY KEY,
                  parent_id TEXT,
                  source_message_id TEXT NOT NULL,
                  title TEXT NOT NULL,
                  normalized_goal TEXT NOT NULL,
                  input_payload TEXT NOT NULL,
                  priority INTEGER NOT NULL,
                  status TEXT NOT NULL,
                  work_group_id TEXT NOT NULL,
                  created_by TEXT NOT NULL,
                  assigned_agent_id TEXT,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS claim_bids (
                  id TEXT PRIMARY KEY,
                  task_card_id TEXT NOT NULL,
                  agent_id TEXT NOT NULL,
                  rationale TEXT NOT NULL,
                  capability_score REAL NOT NULL,
                  expected_tools TEXT NOT NULL,
                  estimated_cost REAL NOT NULL,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS leases (
                  id TEXT PRIMARY KEY,
                  task_card_id TEXT NOT NULL,
                  owner_agent_id TEXT NOT NULL,
                  state TEXT NOT NULL,
                  granted_at TEXT NOT NULL,
                  expires_at TEXT,
                  preempt_requested_at TEXT,
                  released_at TEXT
                );

                CREATE TABLE IF NOT EXISTS tool_runs (
                  id TEXT PRIMARY KEY,
                  tool_id TEXT NOT NULL,
                  task_card_id TEXT NOT NULL,
                  agent_id TEXT NOT NULL,
                  state TEXT NOT NULL,
                  approval_required INTEGER NOT NULL,
                  started_at TEXT,
                  finished_at TEXT,
                  result_ref TEXT
                );

                CREATE TABLE IF NOT EXISTS memory_items (
                  id TEXT PRIMARY KEY,
                  scope TEXT NOT NULL,
                  scope_id TEXT NOT NULL,
                  content TEXT NOT NULL,
                  tags TEXT NOT NULL,
                  embedding_ref TEXT,
                  pinned INTEGER NOT NULL,
                  ttl INTEGER,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS audit_events (
                  id TEXT PRIMARY KEY,
                  event_type TEXT NOT NULL,
                  entity_type TEXT NOT NULL,
                  entity_id TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settings (
                  id TEXT PRIMARY KEY,
                  payload TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_messages_group ON messages(work_group_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_tasks_group ON task_cards(work_group_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_bids_task ON claim_bids(task_card_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_leases_task ON leases(task_card_id);
                CREATE INDEX IF NOT EXISTS idx_tool_runs_task ON tool_runs(task_card_id);
                "#,
            )?;
            Ok(())
        })
    }

    fn seed_demo_data(&self) -> Result<()> {
        self.with_conn(|conn| {
            let agent_count: i64 = conn.query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))?;
            if agent_count > 0 {
                return Ok(());
            }

            let memory_policy = json(&MemoryPolicy::default())?;
            let agent_specs = vec![
                (
                    "Scout",
                    "SC",
                    "Research Lead",
                    "Map the problem space, gather context, and keep the team aligned on evidence.",
                    vec!["skill.research".to_string()],
                    vec![
                        "project.search".to_string(),
                        "http.request".to_string(),
                        "plan.summarize".to_string(),
                    ],
                ),
                (
                    "Builder",
                    "BD",
                    "Systems Engineer",
                    "Turn task cards into executable changes, plans, and runnable artifacts.",
                    vec!["skill.builder".to_string()],
                    vec![
                        "file.readwrite".to_string(),
                        "project.search".to_string(),
                        "shell.exec".to_string(),
                        "markdown.compose".to_string(),
                        "plan.summarize".to_string(),
                    ],
                ),
                (
                    "Reviewer",
                    "RV",
                    "Quality Reviewer",
                    "Stress test proposals, spot regressions, and keep the bar high.",
                    vec!["skill.reviewer".to_string()],
                    vec![
                        "project.search".to_string(),
                        "markdown.compose".to_string(),
                        "plan.summarize".to_string(),
                    ],
                ),
            ];

            let mut agent_ids = Vec::new();
            for (name, avatar, role, objective, skill_ids, tool_ids) in agent_specs {
                let id = new_id();
                agent_ids.push(id.clone());
                conn.execute(
                    r#"
                    INSERT INTO agents (
                      id, name, avatar, role, objective, model_policy, skill_ids, tool_ids,
                      max_parallel_runs, can_spawn_subtasks, memory_policy
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    "#,
                    params![
                        id,
                        name,
                        avatar,
                        role,
                        objective,
                        json(&ModelPolicy::default())?,
                        json(&skill_ids)?,
                        json(&tool_ids)?,
                        2_i64,
                        1_i64,
                        memory_policy.clone(),
                    ],
                )?;
            }

            let work_group_id = new_id();
            let created_at = now();
            conn.execute(
                r#"
                INSERT INTO work_groups (
                  id, kind, name, goal, member_agent_ids, default_visibility, auto_archive, created_at, archived_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
                "#,
                params![
                    work_group_id,
                    json(&WorkGroupKind::Persistent)?,
                    "Launch Deck",
                    "Coordinate product planning, implementation, and review work in one room.",
                    json(&agent_ids)?,
                    "summary",
                    0_i64,
                    created_at,
                ],
            )?;

            conn.execute(
                r#"
                INSERT INTO messages (
                  id, conversation_id, work_group_id, sender_kind, sender_id, sender_name, kind,
                  visibility, content, mentions, task_card_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11)
                "#,
                params![
                    new_id(),
                    work_group_id,
                    work_group_id,
                    json(&SenderKind::System)?,
                    "coordinator",
                    "Coordinator",
                    json(&MessageKind::Status)?,
                    json(&Visibility::Main)?,
                    "Workspace bootstrapped. Send a human directive to create task cards and start bidding.",
                    json(&Vec::<String>::new())?,
                    now(),
                ],
            )?;

            conn.execute(
                r#"
                INSERT INTO memory_items (
                  id, scope, scope_id, content, tags, embedding_ref, pinned, ttl, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, NULL, ?7)
                "#,
                params![
                    new_id(),
                    json(&MemoryScope::WorkGroup)?,
                    work_group_id,
                    "Default operating mode: summary-first timeline, human directives outrank running work.",
                    json(&vec!["policy".to_string(), "coordination".to_string()])?,
                    1_i64,
                    now(),
                ],
            )?;

            conn.execute(
                r#"
                INSERT INTO settings (id, payload) VALUES ('system', ?1)
                "#,
                params![json(&SystemSettings::default())?],
            )?;

            Ok(())
        })
    }

    pub fn dashboard_state(&self) -> Result<DashboardState> {
        Ok(DashboardState {
            agents: self.list_agents()?,
            work_groups: self.list_work_groups()?,
            messages: self.list_messages()?,
            task_cards: self.list_task_cards(None)?,
            claim_bids: self.list_claim_bids()?,
            leases: self.list_leases()?,
            tool_runs: self.list_tool_runs()?,
            audit_events: self.list_audit_events(None)?,
            skills: vec![],
            tools: vec![],
            memory_items: self.list_memory_items()?,
            settings: self.get_settings()?,
        })
    }

    pub fn insert_agent(&self, agent: &AgentProfile) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO agents (
                  id, name, avatar, role, objective, model_policy, skill_ids, tool_ids,
                  max_parallel_runs, can_spawn_subtasks, memory_policy
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    agent.id,
                    agent.name,
                    agent.avatar,
                    agent.role,
                    agent.objective,
                    json(&agent.model_policy)?,
                    json(&agent.skill_ids)?,
                    json(&agent.tool_ids)?,
                    agent.max_parallel_runs,
                    bool_to_i64(agent.can_spawn_subtasks),
                    json(&agent.memory_policy)?,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_agents(&self) -> Result<Vec<AgentProfile>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM agents ORDER BY name ASC")?;
            let rows = stmt.query_map([], map_agent)?;
            collect_rows(rows)
        })
    }

    pub fn get_agent(&self, agent_id: &str) -> Result<AgentProfile> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM agents WHERE id = ?1",
                params![agent_id],
                map_agent,
            )
            .context("agent not found")
        })
    }

    pub fn delete_agent(&self, agent_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, member_agent_ids FROM work_groups")?;
            let group_rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;

            for group_row in group_rows {
                let (group_id, member_agent_ids) = group_row?;
                let mut members: Vec<String> = decode(member_agent_ids)?;
                let original_len = members.len();
                members.retain(|member_id| member_id != agent_id);
                if members.len() != original_len {
                    conn.execute(
                        "UPDATE work_groups SET member_agent_ids = ?1 WHERE id = ?2",
                        params![json(&members)?, group_id],
                    )?;
                }
            }

            let deleted = conn.execute("DELETE FROM agents WHERE id = ?1", params![agent_id])?;
            if deleted == 0 {
                anyhow::bail!("agent not found");
            }
            Ok(())
        })
    }

    pub fn insert_work_group(&self, work_group: &WorkGroup) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO work_groups (
                  id, kind, name, goal, member_agent_ids, default_visibility, auto_archive, created_at, archived_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    work_group.id,
                    json(&work_group.kind)?,
                    work_group.name,
                    work_group.goal,
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
            collect_rows(rows)
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
        let mut group = self.get_work_group(work_group_id)?;
        group.member_agent_ids.retain(|current| current != agent_id);
        self.insert_work_group(&group)?;
        Ok(group)
    }

    pub fn insert_message(&self, message: &ConversationMessage) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO messages (
                  id, conversation_id, work_group_id, sender_kind, sender_id, sender_name, kind,
                  visibility, content, mentions, task_card_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
                    json(&message.mentions)?,
                    message.task_card_id,
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

    pub fn insert_task_card(&self, task_card: &TaskCard) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO task_cards (
                  id, parent_id, source_message_id, title, normalized_goal, input_payload, priority,
                  status, work_group_id, created_by, assigned_agent_id, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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

    pub fn insert_claim_bid(&self, bid: &ClaimBid) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO claim_bids (
                  id, task_card_id, agent_id, rationale, capability_score, expected_tools, estimated_cost, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    bid.id,
                    bid.task_card_id,
                    bid.agent_id,
                    bid.rationale,
                    bid.capability_score,
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

    pub fn insert_lease(&self, lease: &Lease) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO leases (
                  id, task_card_id, owner_agent_id, state, granted_at, expires_at, preempt_requested_at, released_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    lease.id,
                    lease.task_card_id,
                    lease.owner_agent_id,
                    json(&lease.state)?,
                    lease.granted_at,
                    lease.expires_at,
                    lease.preempt_requested_at,
                    lease.released_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_leases(&self) -> Result<Vec<Lease>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM leases ORDER BY granted_at ASC")?;
            let rows = stmt.query_map([], map_lease)?;
            collect_rows(rows)
        })
    }

    pub fn get_lease_by_task(&self, task_card_id: &str) -> Result<Option<Lease>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM leases WHERE task_card_id = ?1 ORDER BY granted_at DESC LIMIT 1",
                params![task_card_id],
                map_lease,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn update_lease(&self, lease: &Lease) -> Result<()> {
        self.insert_lease(lease)
    }

    pub fn insert_tool_run(&self, tool_run: &ToolRun) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO tool_runs (
                  id, tool_id, task_card_id, agent_id, state, approval_required, started_at, finished_at, result_ref
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    tool_run.id,
                    tool_run.tool_id,
                    tool_run.task_card_id,
                    tool_run.agent_id,
                    json(&tool_run.state)?,
                    bool_to_i64(tool_run.approval_required),
                    tool_run.started_at,
                    tool_run.finished_at,
                    tool_run.result_ref,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_tool_run(&self, tool_run_id: &str) -> Result<ToolRun> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM tool_runs WHERE id = ?1",
                params![tool_run_id],
                map_tool_run,
            )
            .context("tool run not found")
        })
    }

    pub fn list_tool_runs(&self) -> Result<Vec<ToolRun>> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT * FROM tool_runs ORDER BY COALESCE(started_at, '') ASC")?;
            let rows = stmt.query_map([], map_tool_run)?;
            collect_rows(rows)
        })
    }

    pub fn list_memory_items(&self) -> Result<Vec<MemoryItem>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM memory_items ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], map_memory_item)?;
            collect_rows(rows)
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

    pub fn insert_audit_event(&self, event: &AuditEvent) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO audit_events (id, event_type, entity_type, entity_id, payload_json, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    event.id,
                    event.event_type,
                    event.entity_type,
                    event.entity_id,
                    event.payload_json,
                    event.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_audit_events(&self, limit: Option<usize>) -> Result<Vec<AuditEvent>> {
        self.with_conn(|conn| {
            let sql = match limit {
                Some(value) => {
                    format!("SELECT * FROM audit_events ORDER BY created_at DESC LIMIT {value}")
                }
                None => "SELECT * FROM audit_events ORDER BY created_at DESC".to_string(),
            };
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], map_audit_event)?;
            collect_rows(rows)
        })
    }

    pub fn list_active_leases_for_group(&self, work_group_id: &str) -> Result<Vec<Lease>> {
        let tasks = self.list_task_cards(Some(work_group_id))?;
        let task_ids: Vec<String> = tasks.into_iter().map(|task| task.id).collect();
        let mut active = Vec::new();
        for task_id in task_ids {
            if let Some(lease) = self.get_lease_by_task(&task_id)? {
                if matches!(
                    lease.state,
                    LeaseState::Active | LeaseState::PreemptRequested
                ) {
                    active.push(lease);
                }
            }
        }
        Ok(active)
    }

    pub fn counts_for_agents(&self, agent_ids: &[String]) -> Result<Vec<(String, i64)>> {
        let leases = self.list_leases()?;
        let mut counts = Vec::new();
        for agent_id in agent_ids {
            let count = leases
                .iter()
                .filter(|lease| {
                    lease.owner_agent_id == *agent_id
                        && matches!(
                            lease.state,
                            LeaseState::Active | LeaseState::PreemptRequested
                        )
                })
                .count() as i64;
            counts.push((agent_id.clone(), count));
        }
        Ok(counts)
    }

    pub fn get_settings(&self) -> Result<SystemSettings> {
        self.with_conn(|conn| {
            let payload = conn
                .query_row(
                    "SELECT payload FROM settings WHERE id = 'system'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            if let Some(payload) = payload {
                return decode(payload).map_err(Into::into);
            }

            let settings = SystemSettings::default();
            conn.execute(
                "INSERT OR REPLACE INTO settings (id, payload) VALUES ('system', ?1)",
                params![json(&settings)?],
            )?;
            Ok(settings)
        })
    }

    pub fn update_settings(&self, settings: &SystemSettings) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO settings (id, payload) VALUES ('system', ?1)",
                params![json(settings)?],
            )?;
            Ok(())
        })
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn decode<T: DeserializeOwned>(raw: String) -> rusqlite::Result<T> {
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(Into::into)
}

fn collect_rows<T, I>(rows: I) -> Result<Vec<T>>
where
    I: Iterator<Item = rusqlite::Result<T>>,
{
    let mut items = Vec::new();
    for item in rows {
        items.push(item?);
    }
    Ok(items)
}

fn map_agent(row: &Row<'_>) -> rusqlite::Result<AgentProfile> {
    Ok(AgentProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        avatar: row.get("avatar")?,
        role: row.get("role")?,
        objective: row.get("objective")?,
        model_policy: decode(row.get("model_policy")?)?,
        skill_ids: decode(row.get("skill_ids")?)?,
        tool_ids: decode(row.get("tool_ids")?)?,
        max_parallel_runs: row.get("max_parallel_runs")?,
        can_spawn_subtasks: row.get::<_, i64>("can_spawn_subtasks")? == 1,
        memory_policy: decode(row.get("memory_policy")?)?,
    })
}

fn map_work_group(row: &Row<'_>) -> rusqlite::Result<WorkGroup> {
    Ok(WorkGroup {
        id: row.get("id")?,
        kind: decode(row.get("kind")?)?,
        name: row.get("name")?,
        goal: row.get("goal")?,
        member_agent_ids: decode(row.get("member_agent_ids")?)?,
        default_visibility: row.get("default_visibility")?,
        auto_archive: row.get::<_, i64>("auto_archive")? == 1,
        created_at: row.get("created_at")?,
        archived_at: row.get("archived_at")?,
    })
}

fn map_message(row: &Row<'_>) -> rusqlite::Result<ConversationMessage> {
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
        mentions: decode(row.get("mentions")?)?,
        task_card_id: row.get("task_card_id")?,
        created_at: row.get("created_at")?,
    })
}

fn map_task_card(row: &Row<'_>) -> rusqlite::Result<TaskCard> {
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
        created_at: row.get("created_at")?,
    })
}

fn map_claim_bid(row: &Row<'_>) -> rusqlite::Result<ClaimBid> {
    Ok(ClaimBid {
        id: row.get("id")?,
        task_card_id: row.get("task_card_id")?,
        agent_id: row.get("agent_id")?,
        rationale: row.get("rationale")?,
        capability_score: row.get("capability_score")?,
        expected_tools: decode(row.get("expected_tools")?)?,
        estimated_cost: row.get("estimated_cost")?,
        created_at: row.get("created_at")?,
    })
}

fn map_lease(row: &Row<'_>) -> rusqlite::Result<Lease> {
    Ok(Lease {
        id: row.get("id")?,
        task_card_id: row.get("task_card_id")?,
        owner_agent_id: row.get("owner_agent_id")?,
        state: decode(row.get("state")?)?,
        granted_at: row.get("granted_at")?,
        expires_at: row.get("expires_at")?,
        preempt_requested_at: row.get("preempt_requested_at")?,
        released_at: row.get("released_at")?,
    })
}

fn map_tool_run(row: &Row<'_>) -> rusqlite::Result<ToolRun> {
    Ok(ToolRun {
        id: row.get("id")?,
        tool_id: row.get("tool_id")?,
        task_card_id: row.get("task_card_id")?,
        agent_id: row.get("agent_id")?,
        state: decode(row.get("state")?)?,
        approval_required: row.get::<_, i64>("approval_required")? == 1,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        result_ref: row.get("result_ref")?,
    })
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

fn map_audit_event(row: &Row<'_>) -> rusqlite::Result<AuditEvent> {
    Ok(AuditEvent {
        id: row.get("id")?,
        event_type: row.get("event_type")?,
        entity_type: row.get("entity_type")?,
        entity_id: row.get("entity_id")?,
        payload_json: row.get("payload_json")?,
        created_at: row.get("created_at")?,
    })
}
