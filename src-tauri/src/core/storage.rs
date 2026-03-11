mod bids;
mod memory;
mod messages;
mod tasks;
mod work_groups;
mod workflows;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Serialize};

use crate::core::domain::{
    new_id, now, AgentPermissionPolicy, AgentProfile, AuditEvent, DashboardState, Lease,
    LeaseState, MemoryPolicy, MemoryScope, MessageKind, ModelPolicy, PendingUserQuestion,
    PendingUserQuestionStatus, SenderKind, SystemSettings, ToolRun, Visibility, WorkGroup,
    WorkGroupKind,
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
                  memory_policy TEXT NOT NULL,
                  permission_policy TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS work_groups (
                  id TEXT PRIMARY KEY,
                  kind TEXT NOT NULL,
                  name TEXT NOT NULL,
                  goal TEXT NOT NULL,
                  working_directory TEXT NOT NULL,
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
                  execution_mode TEXT,
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

                CREATE TABLE IF NOT EXISTS workflows (
                  id TEXT PRIMARY KEY,
                  work_group_id TEXT NOT NULL,
                  source_message_id TEXT NOT NULL,
                  route_mode TEXT NOT NULL,
                  title TEXT NOT NULL,
                  normalized_intent TEXT NOT NULL,
                  status TEXT NOT NULL,
                  owner_agent_id TEXT NOT NULL,
                  current_stage_id TEXT,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS workflow_stages (
                  id TEXT PRIMARY KEY,
                  workflow_id TEXT NOT NULL,
                  title TEXT NOT NULL,
                  goal TEXT NOT NULL,
                  order_index INTEGER NOT NULL,
                  execution_mode TEXT NOT NULL,
                  status TEXT NOT NULL,
                  entry_message_id TEXT,
                  completion_message_id TEXT,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS task_dispatches (
                  task_id TEXT PRIMARY KEY,
                  workflow_id TEXT,
                  stage_id TEXT,
                  dispatch_source TEXT NOT NULL,
                  depends_on_task_ids TEXT NOT NULL,
                  acknowledged_at TEXT,
                  result_message_id TEXT,
                  locked_by_user_mention INTEGER NOT NULL DEFAULT 0,
                  target_agent_id TEXT NOT NULL,
                  route_mode TEXT NOT NULL,
                  narrative_stage_label TEXT,
                  narrative_task_label TEXT
                );

                CREATE TABLE IF NOT EXISTS task_blockers (
                  id TEXT PRIMARY KEY,
                  task_id TEXT NOT NULL,
                  workflow_id TEXT,
                  raised_by_agent_id TEXT NOT NULL,
                  resolution_target TEXT NOT NULL,
                  category TEXT NOT NULL,
                  summary TEXT NOT NULL,
                  details TEXT NOT NULL,
                  status TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  resolved_at TEXT
                );

                CREATE TABLE IF NOT EXISTS workflow_checkpoints (
                  id TEXT PRIMARY KEY,
                  workflow_id TEXT,
                  stage_id TEXT,
                  task_id TEXT,
                  stage_title TEXT,
                  task_title TEXT,
                  assignee_agent_id TEXT,
                  assignee_name TEXT,
                  status TEXT NOT NULL,
                  working_directory TEXT NOT NULL,
                  repo_snapshot_json TEXT NOT NULL,
                  artifact_summary_json TEXT NOT NULL,
                  todo_snapshot_json TEXT NOT NULL,
                  resume_hint TEXT,
                  failure_count INTEGER NOT NULL DEFAULT 0,
                  last_error TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS claim_bids (
                  id TEXT PRIMARY KEY,
                  task_card_id TEXT NOT NULL,
                  agent_id TEXT NOT NULL,
                  rationale TEXT NOT NULL,
                  capability_score REAL NOT NULL,
                  score_breakdown TEXT NOT NULL,
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

                CREATE TABLE IF NOT EXISTS pending_user_questions (
                  id TEXT PRIMARY KEY,
                  work_group_id TEXT NOT NULL,
                  task_card_id TEXT NOT NULL,
                  agent_id TEXT NOT NULL,
                  tool_run_id TEXT,
                  question TEXT NOT NULL,
                  options TEXT NOT NULL,
                  context TEXT,
                  allow_free_form INTEGER NOT NULL,
                  asked_message_id TEXT NOT NULL,
                  answer_message_id TEXT,
                  status TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  answered_at TEXT
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
                CREATE INDEX IF NOT EXISTS idx_workflows_group ON workflows(work_group_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_workflow_stages_workflow ON workflow_stages(workflow_id, order_index);
                CREATE INDEX IF NOT EXISTS idx_task_dispatches_workflow ON task_dispatches(workflow_id, stage_id);
                CREATE INDEX IF NOT EXISTS idx_task_blockers_task ON task_blockers(task_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_workflow_checkpoints_workflow ON workflow_checkpoints(workflow_id, updated_at);
                CREATE INDEX IF NOT EXISTS idx_workflow_checkpoints_stage ON workflow_checkpoints(stage_id, updated_at);
                CREATE INDEX IF NOT EXISTS idx_workflow_checkpoints_task ON workflow_checkpoints(task_id, updated_at);
                CREATE INDEX IF NOT EXISTS idx_bids_task ON claim_bids(task_card_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_leases_task ON leases(task_card_id);
                CREATE INDEX IF NOT EXISTS idx_tool_runs_task ON tool_runs(task_card_id);
                CREATE INDEX IF NOT EXISTS idx_pending_user_questions_group ON pending_user_questions(work_group_id, created_at);
                "#,
            )?;
            ensure_column(conn, "messages", "execution_mode", "TEXT")?;
            ensure_column(conn, "agents", "permission_policy", "TEXT")?;
            ensure_column(conn, "claim_bids", "score_breakdown", "TEXT")?;
            ensure_column(
                conn,
                "work_groups",
                "working_directory",
                "TEXT NOT NULL DEFAULT '.'",
            )?;
            ensure_column(conn, "work_groups", "owner_agent_id", "TEXT")?;
            conn.execute(
                "UPDATE agents SET permission_policy = ?1 WHERE permission_policy IS NULL",
                params![json(&AgentPermissionPolicy::default())?],
            )?;
            conn.execute(
                "UPDATE claim_bids SET score_breakdown = ?1 WHERE score_breakdown IS NULL",
                params![json(&crate::core::domain::ClaimScoreBreakdown::default())?],
            )?;
            conn.execute(
                "UPDATE work_groups SET working_directory = '.' WHERE working_directory IS NULL OR trim(working_directory) = ''",
                [],
            )?;
            {
                let mut stmt =
                    conn.prepare("SELECT id, owner_agent_id, member_agent_ids FROM work_groups")?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                for row in rows {
                    let (group_id, owner_agent_id, member_agent_ids) = row?;
                    if owner_agent_id
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|value| !value.is_empty())
                    {
                        continue;
                    }
                    let members: Vec<String> = decode(member_agent_ids)?;
                    let Some(first_member) = members.first() else {
                        continue;
                    };
                    conn.execute(
                        "UPDATE work_groups SET owner_agent_id = ?1 WHERE id = ?2",
                        params![first_member, group_id],
                    )?;
                }
            }
            {
                let mut stmt = conn.prepare("SELECT id, tool_ids FROM agents")?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                for row in rows {
                    let (agent_id, tool_ids_raw) = row?;
                    let mut tool_ids: Vec<String> = decode(tool_ids_raw)?;
                    if tool_ids.iter().any(|tool_id| tool_id == "AskUserQuestion") {
                        continue;
                    }
                    tool_ids.push("AskUserQuestion".to_string());
                    conn.execute(
                        "UPDATE agents SET tool_ids = ?1 WHERE id = ?2",
                        params![json(&tool_ids)?, agent_id],
                    )?;
                }
            }
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
            let permission_policy = json(&AgentPermissionPolicy::default())?;
            let agent_specs = vec![
                (
                    "Scout",
                    "SC",
                    "Research Lead",
                    "Map the problem space, gather context, and keep the team aligned on evidence.",
                    vec![
                        "Skills".to_string(),
                        "Grep".to_string(),
                        "WebSearch".to_string(),
                        "WebFetch".to_string(),
                        "Read".to_string(),
                        "AskUserQuestion".to_string(),
                        "Task".to_string(),
                    ],
                ),
                (
                    "Builder",
                    "BD",
                    "Systems Engineer",
                    "Turn task cards into executable changes, plans, and runnable artifacts.",
                    vec![
                        "Skills".to_string(),
                        "Read".to_string(),
                        "Edit".to_string(),
                        "Write".to_string(),
                        "MultiEdit".to_string(),
                        "Grep".to_string(),
                        "Glob".to_string(),
                        "Bash".to_string(),
                        "shell.exec".to_string(),
                        "AskUserQuestion".to_string(),
                        "TodoWrite".to_string(),
                        "ExitPlanMode".to_string(),
                    ],
                ),
                (
                    "Reviewer",
                    "RV",
                    "Quality Reviewer",
                    "Stress test proposals, spot regressions, and keep the bar high.",
                    vec![
                        "Skills".to_string(),
                        "Read".to_string(),
                        "Grep".to_string(),
                        "LS".to_string(),
                        "AskUserQuestion".to_string(),
                        "TodoWrite".to_string(),
                    ],
                ),
            ];

            let mut agent_ids = Vec::new();
            for (name, avatar, role, objective, tool_ids) in agent_specs {
                let id = new_id();
                agent_ids.push(id.clone());
                conn.execute(
                    r#"
                    INSERT INTO agents (
                      id, name, avatar, role, objective, model_policy, skill_ids, tool_ids,
                      max_parallel_runs, can_spawn_subtasks, memory_policy, permission_policy
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                    "#,
                    params![
                        id,
                        name,
                        avatar,
                        role,
                        objective,
                        json(&ModelPolicy::default())?,
                        json(&Vec::<String>::new())?,
                        json(&tool_ids)?,
                        2_i64,
                        1_i64,
                        memory_policy.clone(),
                        permission_policy.clone(),
                    ],
                )?;
            }

            let work_group_id = new_id();
            let created_at = now();
            conn.execute(
                r#"
                INSERT INTO work_groups (
                  id, kind, name, goal, working_directory, member_agent_ids, owner_agent_id, default_visibility, auto_archive, created_at, archived_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)
                "#,
                params![
                    work_group_id,
                    json(&WorkGroupKind::Persistent)?,
                    "Launch Deck",
                    "Coordinate product planning, implementation, and review work in one room.",
                    ".",
                    json(&agent_ids)?,
                    agent_ids.first().cloned(),
                    "summary",
                    0_i64,
                    created_at,
                ],
            )?;

            conn.execute(
                r#"
                INSERT INTO messages (
                  id, conversation_id, work_group_id, sender_kind, sender_id, sender_name, kind,
                  visibility, content, mentions, task_card_id, execution_mode, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, ?11)
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
            pending_user_questions: self.list_pending_user_questions()?,
            task_blockers: self.list_task_blockers()?,
            workflow_checkpoints: self.list_all_workflow_checkpoints()?,
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
                  max_parallel_runs, can_spawn_subtasks, memory_policy, permission_policy
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
                    json(&agent.permission_policy)?,
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
            let mut stmt =
                conn.prepare("SELECT id, owner_agent_id, member_agent_ids FROM work_groups")?;
            let group_rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;

            for group_row in group_rows {
                let (group_id, owner_agent_id, member_agent_ids) = group_row?;
                let mut members: Vec<String> = decode(member_agent_ids)?;
                let original_len = members.len();
                members.retain(|member_id| member_id != agent_id);
                let mut next_owner = owner_agent_id;
                let mut owner_changed = false;
                if next_owner.as_deref() == Some(agent_id) {
                    next_owner = members.first().cloned();
                    owner_changed = true;
                }
                if members.len() != original_len || owner_changed {
                    conn.execute(
                        "UPDATE work_groups SET member_agent_ids = ?1, owner_agent_id = ?2 WHERE id = ?3",
                        params![json(&members)?, next_owner, group_id],
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

    pub fn insert_pending_user_question(&self, question: &PendingUserQuestion) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO pending_user_questions (
                  id, work_group_id, task_card_id, agent_id, tool_run_id, question, options,
                  context, allow_free_form, asked_message_id, answer_message_id, status, created_at, answered_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                ON CONFLICT(id) DO UPDATE SET
                  work_group_id = excluded.work_group_id,
                  task_card_id = excluded.task_card_id,
                  agent_id = excluded.agent_id,
                  tool_run_id = excluded.tool_run_id,
                  question = excluded.question,
                  options = excluded.options,
                  context = excluded.context,
                  allow_free_form = excluded.allow_free_form,
                  asked_message_id = excluded.asked_message_id,
                  answer_message_id = excluded.answer_message_id,
                  status = excluded.status,
                  created_at = excluded.created_at,
                  answered_at = excluded.answered_at
                "#,
                params![
                    question.id,
                    question.work_group_id,
                    question.task_card_id,
                    question.agent_id,
                    question.tool_run_id,
                    question.question,
                    json(&question.options)?,
                    question.context,
                    if question.allow_free_form { 1_i64 } else { 0_i64 },
                    question.asked_message_id,
                    question.answer_message_id,
                    json(&question.status)?,
                    question.created_at,
                    question.answered_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn latest_pending_user_question_for_group(
        &self,
        work_group_id: &str,
    ) -> Result<Option<PendingUserQuestion>> {
        self.with_conn(|conn| {
            conn.query_row(
                r#"
                SELECT
                  id, work_group_id, task_card_id, agent_id, tool_run_id, question, options,
                  context, allow_free_form, asked_message_id, answer_message_id, status, created_at, answered_at
                FROM pending_user_questions
                WHERE work_group_id = ?1 AND status = ?2
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                params![work_group_id, json(&PendingUserQuestionStatus::Pending)?],
                map_pending_user_question,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub fn list_pending_user_questions(&self) -> Result<Vec<PendingUserQuestion>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                  id, work_group_id, task_card_id, agent_id, tool_run_id, question, options,
                  context, allow_free_form, asked_message_id, answer_message_id, status, created_at, answered_at
                FROM pending_user_questions
                WHERE status = ?1
                ORDER BY created_at DESC
                "#,
            )?;
            let rows =
                stmt.query_map(params![json(&PendingUserQuestionStatus::Pending)?], map_pending_user_question)?;
            collect_rows(rows)
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

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns = collect_rows(rows)?;

    if !columns.iter().any(|existing| existing == column) {
        if let Err(error) = conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        ) {
            let duplicate_column = matches!(
                &error,
                rusqlite::Error::SqliteFailure(_, Some(message))
                    if message.contains("duplicate column name")
            );
            if !duplicate_column {
                return Err(error.into());
            }
        }
    }

    Ok(())
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
        permission_policy: row
            .get::<_, Option<String>>("permission_policy")?
            .map(decode::<AgentPermissionPolicy>)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn map_work_group(row: &Row<'_>) -> rusqlite::Result<WorkGroup> {
    Ok(WorkGroup {
        id: row.get("id")?,
        kind: decode(row.get("kind")?)?,
        name: row.get("name")?,
        goal: row.get("goal")?,
        working_directory: row.get("working_directory")?,
        member_agent_ids: decode(row.get("member_agent_ids")?)?,
        default_visibility: row.get("default_visibility")?,
        auto_archive: row.get::<_, i64>("auto_archive")? == 1,
        created_at: row.get("created_at")?,
        archived_at: row.get("archived_at")?,
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

fn map_pending_user_question(row: &Row<'_>) -> rusqlite::Result<PendingUserQuestion> {
    Ok(PendingUserQuestion {
        id: row.get("id")?,
        work_group_id: row.get("work_group_id")?,
        task_card_id: row.get("task_card_id")?,
        agent_id: row.get("agent_id")?,
        tool_run_id: row.get("tool_run_id")?,
        question: row.get("question")?,
        options: decode(row.get("options")?)?,
        context: row.get("context")?,
        allow_free_form: row.get::<_, i64>("allow_free_form")? == 1,
        asked_message_id: row.get("asked_message_id")?,
        answer_message_id: row.get("answer_message_id")?,
        status: decode(row.get("status")?)?,
        created_at: row.get("created_at")?,
        answered_at: row.get("answered_at")?,
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
