use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderKind {
    Human,
    Agent,
    System,
}

impl Default for SenderKind {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Summary,
    ToolCall,
    ToolResult,
    Approval,
    Status,
}

impl Default for MessageKind {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Main,
    Backstage,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Main
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkGroupKind {
    Persistent,
    Ephemeral,
}

impl Default for WorkGroupKind {
    fn default() -> Self {
        Self::Persistent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Bidding,
    Leased,
    WaitingChildren,
    WaitingApproval,
    InProgress,
    Paused,
    Cancelled,
    Completed,
    NeedsReview,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LeaseState {
    Active,
    Paused,
    Released,
    PreemptRequested,
}

impl Default for LeaseState {
    fn default() -> Self {
        Self::Active
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskLevel {
    Low,
    Medium,
    High,
}

impl Default for ToolRiskLevel {
    fn default() -> Self {
        Self::Low
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRunState {
    PendingApproval,
    Queued,
    Running,
    Completed,
    Cancelled,
}

impl Default for ToolRunState {
    fn default() -> Self {
        Self::Queued
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    WorkGroup,
    Agent,
}

impl Default for MemoryScope {
    fn default() -> Self {
        Self::WorkGroup
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPolicy {
    pub provider: String,
    pub model: String,
    pub temperature: f64,
}

impl Default for ModelPolicy {
    fn default() -> Self {
        Self {
            provider: "mock".into(),
            model: "simulation".into(),
            temperature: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPolicy {
    pub read_scope: Vec<String>,
    pub write_scope: Vec<String>,
    pub pinned_memory_ids: Vec<String>,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            read_scope: vec!["user".into(), "work_group".into(), "agent".into()],
            write_scope: vec!["work_group".into(), "agent".into()],
            pinned_memory_ids: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub objective: String,
    pub model_policy: ModelPolicy,
    pub skill_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_parallel_runs: i64,
    pub can_spawn_subtasks: bool,
    pub memory_policy: MemoryPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkGroup {
    pub id: String,
    pub kind: WorkGroupKind,
    pub name: String,
    pub goal: String,
    pub member_agent_ids: Vec<String>,
    pub default_visibility: String,
    pub auto_archive: bool,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub work_group_id: String,
    pub sender_kind: SenderKind,
    pub sender_id: String,
    pub sender_name: String,
    pub kind: MessageKind,
    pub visibility: Visibility,
    pub content: String,
    pub mentions: Vec<String>,
    pub task_card_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCard {
    pub id: String,
    pub parent_id: Option<String>,
    pub source_message_id: String,
    pub title: String,
    pub normalized_goal: String,
    pub input_payload: String,
    pub priority: i64,
    pub status: TaskStatus,
    pub work_group_id: String,
    pub created_by: String,
    pub assigned_agent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimBid {
    pub id: String,
    pub task_card_id: String,
    pub agent_id: String,
    pub rationale: String,
    pub capability_score: f64,
    pub expected_tools: Vec<String>,
    pub estimated_cost: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub id: String,
    pub task_card_id: String,
    pub owner_agent_id: String,
    pub state: LeaseState,
    pub granted_at: String,
    pub expires_at: Option<String>,
    pub preempt_requested_at: Option<String>,
    pub released_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolManifest {
    pub id: String,
    pub name: String,
    pub category: String,
    pub risk_level: ToolRiskLevel,
    pub input_schema: String,
    pub output_schema: String,
    pub timeout_ms: i64,
    pub concurrency_limit: i64,
    pub permissions: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRun {
    pub id: String,
    pub tool_id: String,
    pub task_card_id: String,
    pub agent_id: String,
    pub state: ToolRunState,
    pub approval_required: bool,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub result_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPack {
    pub id: String,
    pub name: String,
    pub prompt_template: String,
    pub planning_rules: Vec<String>,
    pub allowed_tool_tags: Vec<String>,
    pub done_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub embedding_ref: Option<String>,
    pub pinned: bool,
    pub ttl: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
    pub payload_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProviderConfig {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub enabled: bool,
    pub rig_provider_type: String,
    pub api_key: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub temperature: f64,
    pub max_tokens: i64,
    pub output_token_limit: i64,
    pub max_dialog_rounds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIGlobalConfig {
    pub default_llm_provider: String,
    pub default_llm_model: String,
    pub default_vlm_provider: String,
    pub default_vlm_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub providers: Vec<AIProviderConfig>,
    pub global_config: AIGlobalConfig,
}

impl Default for AIProviderConfig {
    fn default() -> Self {
        Self {
            id: "openai".into(),
            name: "OpenAI".into(),
            icon: "fab fa-openai".into(),
            enabled: true,
            rig_provider_type: "OpenAI".into(),
            api_key: "".into(),
            base_url: "https://api.openai.com/v1".into(),
            models: vec![
                "gpt-4o".into(),
                "gpt-4o-mini".into(),
                "gpt-4-turbo".into(),
                "gpt-3.5-turbo".into(),
                "o1".into(),
                "o1-mini".into(),
                "o3-mini".into(),
            ],
            default_model: "gpt-4o".into(),
            temperature: 0.7,
            max_tokens: 2000,
            output_token_limit: 16384,
            max_dialog_rounds: 540,
        }
    }
}

impl Default for AIGlobalConfig {
    fn default() -> Self {
        Self {
            default_llm_provider: "openai".into(),
            default_llm_model: "gpt-4o".into(),
            default_vlm_provider: "gemini".into(),
            default_vlm_model: "gemini-2.0-flash".into(),
        }
    }
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            providers: vec![
                AIProviderConfig::default(),
                AIProviderConfig {
                    id: "anthropic".into(),
                    name: "Anthropic".into(),
                    icon: "fas fa-comment-dots".into(),
                    enabled: false,
                    rig_provider_type: "Anthropic".into(),
                    api_key: "".into(),
                    base_url: "https://api.anthropic.com".into(),
                    models: vec![
                        "claude-3-5-sonnet-20241022".into(),
                        "claude-3-5-haiku-20241022".into(),
                        "claude-3-opus-20240229".into(),
                    ],
                    default_model: "claude-3-5-sonnet-20241022".into(),
                    ..AIProviderConfig::default()
                },
                AIProviderConfig {
                    id: "gemini".into(),
                    name: "Gemini".into(),
                    icon: "fab fa-google".into(),
                    enabled: false,
                    rig_provider_type: "Gemini".into(),
                    api_key: "".into(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
                    models: vec![
                        "gemini-2.0-flash".into(),
                        "gemini-2.0-pro-exp-02-05".into(),
                        "gemini-1.5-pro".into(),
                    ],
                    default_model: "gemini-2.0-flash".into(),
                    ..AIProviderConfig::default()
                },
                AIProviderConfig {
                    id: "deepseek".into(),
                    name: "DeepSeek".into(),
                    icon: "fas fa-water".into(),
                    enabled: false,
                    rig_provider_type: "DeepSeek".into(),
                    api_key: "".into(),
                    base_url: "https://api.deepseek.com/v1".into(),
                    models: vec!["deepseek-chat".into(), "deepseek-coder".into(), "deepseek-reasoner".into()],
                    default_model: "deepseek-chat".into(),
                    ..AIProviderConfig::default()
                },
            ],
            global_config: AIGlobalConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardState {
    pub agents: Vec<AgentProfile>,
    pub work_groups: Vec<WorkGroup>,
    pub messages: Vec<ConversationMessage>,
    pub task_cards: Vec<TaskCard>,
    pub claim_bids: Vec<ClaimBid>,
    pub leases: Vec<Lease>,
    pub tool_runs: Vec<ToolRun>,
    pub audit_events: Vec<AuditEvent>,
    pub skills: Vec<SkillPack>,
    pub tools: Vec<ToolManifest>,
    pub memory_items: Vec<MemoryItem>,
    pub settings: SystemSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentInput {
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub objective: String,
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub skill_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_parallel_runs: i64,
    pub can_spawn_subtasks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentInput {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub objective: String,
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub skill_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_parallel_runs: i64,
    pub can_spawn_subtasks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkGroupInput {
    pub name: String,
    pub goal: String,
    pub kind: WorkGroupKind,
    pub default_visibility: String,
    pub auto_archive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendHumanMessageInput {
    pub work_group_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionContext {
    pub agent: AgentProfile,
    pub work_group: WorkGroup,
    pub work_group_members: Vec<AgentProfile>,
    pub task_card: TaskCard,
    pub conversation_window: Vec<ConversationMessage>,
    pub available_tools: Vec<ToolManifest>,
    pub available_skills: Vec<SkillPack>,
    pub approved_tool: Option<ToolManifest>,
    pub settings: SystemSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionRequest {
    pub tool: ToolManifest,
    pub input: String,
    pub task_card_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionResult {
    pub output: String,
    pub result_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecution {
    pub summary: String,
    pub backstage_notes: String,
    pub suggested_subtasks: Vec<String>,
    pub tool_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimContext {
    pub task_card: TaskCard,
    pub work_group: WorkGroup,
    pub candidates: Vec<AgentProfile>,
    pub content: String,
    pub mentioned_agent_ids: Vec<String>,
    pub active_loads: Vec<(String, i64)>,
    pub requested_tool: Option<ToolManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimPlan {
    pub task_card: TaskCard,
    pub bids: Vec<ClaimBid>,
    pub lease: Option<Lease>,
    pub coordinator_messages: Vec<ConversationMessage>,
    pub requested_tool: Option<ToolManifest>,
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute_task(&self, context: TaskExecutionContext) -> Result<AgentExecution>;
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, request: ToolExecutionRequest) -> Result<ToolExecutionResult>;
}

#[allow(dead_code)]
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn write_memory(&self, item: MemoryItem) -> Result<()>;
}

pub trait ClaimScorer: Send + Sync {
    fn score(&self, context: ClaimContext) -> Result<ClaimPlan>;
}

#[async_trait]
pub trait ModelProviderAdapter: Send + Sync {
    async fn complete(
        &self,
        policy: &ModelPolicy,
        settings: &SystemSettings,
        preamble: &str,
        prompt: &str,
    ) -> Result<Option<String>>;
}
