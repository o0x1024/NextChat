use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::core::workflow::{
    TaskBlockerRecord, WorkflowCheckpointRecord, WorkflowRecord, WorkflowStageRecord,
};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Summary,
    ToolCall,
    ToolResult,
    Collaboration,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    RealModel,
    Fallback,
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
    WaitingUserInput,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    WorkGroup,
    Agent,
    Task,
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
pub struct AgentPermissionPolicy {
    pub allow_tool_ids: Vec<String>,
    pub deny_tool_ids: Vec<String>,
    pub require_approval_tool_ids: Vec<String>,
    pub allow_fs_roots: Vec<String>,
    pub allow_network_domains: Vec<String>,
}

impl Default for AgentPermissionPolicy {
    fn default() -> Self {
        Self {
            allow_tool_ids: vec![],
            deny_tool_ids: vec![],
            require_approval_tool_ids: vec![],
            allow_fs_roots: vec![],
            allow_network_domains: vec![],
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
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_parallel_runs: i64,
    pub can_spawn_subtasks: bool,
    pub memory_policy: MemoryPolicy,
    pub permission_policy: AgentPermissionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkGroup {
    pub id: String,
    pub kind: WorkGroupKind,
    pub name: String,
    pub goal: String,
    pub working_directory: String,
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
    pub narrative_meta: Option<String>,
    pub mentions: Vec<String>,
    pub task_card_id: Option<String>,
    pub execution_mode: Option<ExecutionMode>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatStreamPhase {
    Start,
    Delta,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub stream_id: String,
    pub phase: ChatStreamPhase,
    pub conversation_id: String,
    pub work_group_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub kind: MessageKind,
    pub visibility: Visibility,
    pub task_card_id: Option<String>,
    pub sequence: i64,
    pub delta: Option<String>,
    pub full_content: Option<String>,
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
    pub output_summary: Option<String>,
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
    pub score_breakdown: ClaimScoreBreakdown,
    pub expected_tools: Vec<String>,
    pub estimated_cost: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaimScoreBreakdown {
    pub factors: Vec<ClaimScoreFactor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimScoreFactor {
    pub kind: ClaimScoreFactorKind,
    pub score: f64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimScoreFactorKind {
    Base,
    Mention,
    Capacity,
    OverCapacity,
    RoleMatch,
    ToolCoverage,
    ToolMismatch,
    SkillMatch,
    LoadPenalty,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingUserQuestionStatus {
    Pending,
    Answered,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUserQuestion {
    pub id: String,
    pub work_group_id: String,
    pub task_card_id: String,
    pub agent_id: String,
    pub tool_run_id: Option<String>,
    pub question: String,
    pub options: Vec<String>,
    pub context: Option<String>,
    pub allow_free_form: bool,
    pub asked_message_id: String,
    pub answer_message_id: Option<String>,
    pub status: PendingUserQuestionStatus,
    pub created_at: String,
    pub answered_at: Option<String>,
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
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub editable: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub install_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileEntry {
    pub path: String,
    pub size: i64,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub skill_id: String,
    pub enabled: bool,
    pub source: String,
    pub install_path: String,
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub user_invocable: bool,
    pub disable_model_invocation: bool,
    pub allowed_tools: Option<String>,
    pub model: Option<String>,
    pub context: Option<String>,
    pub agent: Option<String>,
    pub hooks_json: Option<String>,
    pub summary: Option<String>,
    pub content: String,
    pub files: Vec<SkillFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillDetailInput {
    pub skill_id: String,
    pub enabled: bool,
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub user_invocable: bool,
    pub disable_model_invocation: bool,
    pub allowed_tools: Option<String>,
    pub model: Option<String>,
    pub context: Option<String>,
    pub agent: Option<String>,
    pub hooks_json: Option<String>,
    pub summary: Option<String>,
    pub content: String,
}

fn default_true() -> bool {
    true
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
    #[serde(default = "default_max_context_length")]
    pub max_context_length: i64,
    #[serde(default = "default_custom_headers")]
    pub custom_headers: String,
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
    pub mask_api_keys: bool,
    pub enable_audit_log: bool,
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub providers: Vec<AIProviderConfig>,
    pub global_config: AIGlobalConfig,
}

fn default_max_context_length() -> i64 {
    128_000
}

fn default_custom_headers() -> String {
    "{}".to_string()
}

fn provider_config(
    id: &str,
    name: &str,
    icon: &str,
    enabled: bool,
    rig_provider_type: &str,
    base_url: &str,
    models: &[&str],
    default_model: &str,
    max_context_length: i64,
    api_key: &str,
) -> AIProviderConfig {
    AIProviderConfig {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        enabled,
        rig_provider_type: rig_provider_type.into(),
        api_key: api_key.into(),
        base_url: base_url.into(),
        models: models.iter().map(|model| (*model).into()).collect(),
        default_model: default_model.into(),
        max_context_length,
        custom_headers: default_custom_headers(),
        ..AIProviderConfig::default()
    }
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
            models: vec![],
            default_model: "gpt-4o".into(),
            max_context_length: default_max_context_length(),
            custom_headers: default_custom_headers(),
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
            mask_api_keys: true,
            enable_audit_log: true,
            proxy_url: "".into(),
        }
    }
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            providers: vec![
                AIProviderConfig::default(),
                provider_config(
                    "anthropic",
                    "Anthropic",
                    "fas fa-comment-dots",
                    false,
                    "Anthropic",
                    "https://api.anthropic.com",
                    &[],
                    "",
                    200_000,
                    "",
                ),
                provider_config(
                    "gemini",
                    "Gemini",
                    "fab fa-google",
                    false,
                    "Gemini",
                    "https://generativelanguage.googleapis.com/v1beta",
                    &[],
                    "",
                    1_000_000,
                    "",
                ),
                provider_config(
                    "deepseek",
                    "DeepSeek",
                    "fas fa-water",
                    false,
                    "DeepSeek",
                    "https://api.deepseek.com",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "groq",
                    "Groq",
                    "fas fa-bolt",
                    false,
                    "Groq",
                    "https://api.groq.com/openai/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "cohere",
                    "Cohere",
                    "fas fa-circle",
                    false,
                    "Cohere",
                    "https://api.cohere.ai",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "xai",
                    "xAI",
                    "fas fa-robot",
                    false,
                    "xAI",
                    "https://api.x.ai/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "mistral",
                    "Mistral",
                    "fas fa-wind",
                    false,
                    "Mistral",
                    "https://api.mistral.ai/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "moonshot",
                    "Moonshot",
                    "fas fa-moon",
                    false,
                    "Moonshot",
                    "https://api.moonshot.ai/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "hyperbolic",
                    "Hyperbolic",
                    "fas fa-infinity",
                    false,
                    "Hyperbolic",
                    "https://api.hyperbolic.xyz/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "mira",
                    "Mira",
                    "fas fa-sparkles",
                    false,
                    "Mira",
                    "https://api.mira.network/v1",
                    &["mira-chat"],
                    "mira-chat",
                    128_000,
                    "",
                ),
                provider_config(
                    "openrouter",
                    "OpenRouter",
                    "fas fa-route",
                    false,
                    "OpenRouter",
                    "https://openrouter.ai/api/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "perplexity",
                    "Perplexity",
                    "fas fa-magnifying-glass",
                    false,
                    "Perplexity",
                    "https://api.perplexity.ai",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "together",
                    "Together",
                    "fas fa-link",
                    false,
                    "Together",
                    "https://api.together.xyz/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "huggingface",
                    "HuggingFace",
                    "fas fa-face-smile",
                    false,
                    "HuggingFace",
                    "https://router.huggingface.co/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "ollama",
                    "Ollama",
                    "fas fa-server",
                    false,
                    "Ollama",
                    "http://localhost:11434",
                    &[],
                    "",
                    32_000,
                    "ollama",
                ),
                provider_config(
                    "azure",
                    "Azure OpenAI",
                    "fab fa-microsoft",
                    false,
                    "Azure",
                    "https://<your-resource>.openai.azure.com",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "galadriel",
                    "Galadriel",
                    "fas fa-ring",
                    false,
                    "Galadriel",
                    "https://api.galadriel.com/v1/verified",
                    &[],
                    "",
                    128_000,
                    "",
                ),
                provider_config(
                    "voyageai",
                    "VoyageAI",
                    "fas fa-compass",
                    false,
                    "VoyageAI",
                    "https://api.voyageai.com/v1",
                    &[],
                    "",
                    128_000,
                    "",
                ),
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
    pub pending_user_questions: Vec<PendingUserQuestion>,
    pub task_blockers: Vec<TaskBlockerRecord>,
    pub workflow_checkpoints: Vec<WorkflowCheckpointRecord>,
    pub workflows: Vec<WorkflowRecord>,
    pub workflow_stages: Vec<WorkflowStageRecord>,
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
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_parallel_runs: i64,
    pub can_spawn_subtasks: bool,
    pub memory_policy: MemoryPolicy,
    pub permission_policy: AgentPermissionPolicy,
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
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub tool_ids: Vec<String>,
    pub max_parallel_runs: i64,
    pub can_spawn_subtasks: bool,
    pub memory_policy: MemoryPolicy,
    pub permission_policy: AgentPermissionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkGroupInput {
    pub name: String,
    pub goal: String,
    pub working_directory: String,
    pub kind: WorkGroupKind,
    pub default_visibility: String,
    pub auto_archive: bool,
    #[serde(default)]
    pub member_agent_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkGroupInput {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub working_directory: String,
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
    pub memory_context: Vec<MemoryItem>,
    pub available_tools: Vec<ToolManifest>,
    pub available_skills: Vec<SkillPack>,
    pub approved_tool: Option<ToolManifest>,
    pub approved_tool_input: Option<String>,
    pub upstream_context: Option<String>,
    pub settings: SystemSettings,
    #[serde(skip)]
    pub summary_stream: Option<UnboundedSender<SummaryStreamSignal>>,
    #[serde(skip)]
    pub tool_stream: Option<UnboundedSender<ToolStreamChunk>>,
    #[serde(skip)]
    pub tool_call_stream: Option<UnboundedSender<ToolCallProgressEvent>>,
}

#[derive(Debug, Clone)]
pub enum SummaryStreamSignal {
    Delta(String),
    Reset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionRequest {
    pub tool: ToolManifest,
    pub input: String,
    pub task_card_id: String,
    pub agent_id: String,
    pub agent: AgentProfile,
    pub approval_granted: bool,
    pub working_directory: String,
    #[serde(skip)]
    pub tool_stream: Option<UnboundedSender<ToolStreamChunk>>,
}

#[derive(Debug, Clone)]
pub struct ToolStreamChunk {
    pub tool_id: String,
    pub channel: String,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallProgressPhase {
    Started,
    Completed,
}

#[derive(Debug, Clone)]
pub struct ToolCallProgressEvent {
    pub tool_id: String,
    pub tool_name: String,
    pub call_id: String,
    pub input: String,
    pub output: String,
    pub phase: ToolCallProgressPhase,
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
    pub execution_mode: ExecutionMode,
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
