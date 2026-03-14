use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestRouteMode {
    OwnerOrchestrated,
    DirectAgentAssign,
    DirectAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeMessageType {
    OwnerAck,
    OwnerPlan,
    OwnerDispatch,
    AgentAck,
    AgentProgress,
    AgentDelivery,
    OwnerStageTransition,
    BlockerRaised,
    BlockerResolved,
    OwnerSummary,
    DirectAssign,
    DirectResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Planning,
    Running,
    Blocked,
    NeedsUserInput,
    Completed,
    NeedsReview,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Ready,
    Running,
    Blocked,
    Completed,
    NeedsReview,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowExecutionMode {
    Serial,
    Parallel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskDispatchSource {
    OwnerAssign,
    UserDirect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerResolutionTarget {
    Owner,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerCategory {
    MissingDependency,
    MissingContext,
    PermissionRequired,
    ToolFailure,
    DesignConflict,
    NeedUserDecision,
    PeerInputRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockerStatus {
    Open,
    Resolved,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaiseTaskBlockerInput {
    pub raised_by_agent_id: String,
    pub resolution_target: BlockerResolutionTarget,
    pub category: BlockerCategory,
    pub summary: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum OwnerBlockerResolution {
    ProvideContext {
        message: String,
    },
    ReassignTask {
        target_agent_id: String,
        message: String,
    },
    CreateDependencyTask {
        target_agent_id: String,
        title: String,
        goal: String,
        message: String,
    },
    RequestApproval {
        question: String,
        options: Vec<String>,
        context: Option<String>,
        allow_free_form: Option<bool>,
    },
    AskUser {
        question: String,
        options: Vec<String>,
        context: Option<String>,
        allow_free_form: Option<bool>,
    },
    PauseTask {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecord {
    pub id: String,
    pub work_group_id: String,
    pub source_message_id: String,
    pub route_mode: RequestRouteMode,
    pub title: String,
    pub normalized_intent: String,
    pub status: WorkflowStatus,
    pub owner_agent_id: String,
    pub current_stage_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStageRecord {
    pub id: String,
    pub workflow_id: String,
    pub title: String,
    pub goal: String,
    pub order_index: i64,
    pub execution_mode: WorkflowExecutionMode,
    pub status: StageStatus,
    pub entry_message_id: Option<String>,
    pub completion_message_id: Option<String>,
    pub deliverables_json: Option<String>,
    pub quality_gate_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDispatchRecord {
    pub task_id: String,
    pub workflow_id: Option<String>,
    pub stage_id: Option<String>,
    pub dispatch_source: TaskDispatchSource,
    pub depends_on_task_ids: Vec<String>,
    pub acknowledged_at: Option<String>,
    pub result_message_id: Option<String>,
    pub locked_by_user_mention: bool,
    pub target_agent_id: String,
    pub route_mode: RequestRouteMode,
    pub narrative_stage_label: Option<String>,
    pub narrative_task_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBlockerRecord {
    pub id: String,
    pub task_id: String,
    pub workflow_id: Option<String>,
    pub raised_by_agent_id: String,
    pub resolution_target: BlockerResolutionTarget,
    pub category: BlockerCategory,
    pub summary: String,
    pub details: String,
    pub status: BlockerStatus,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCheckpointStatus {
    WorkflowPlanned,
    WorkflowRunning,
    WorkflowCompleted,
    StagePending,
    StageRunning,
    StageCompleted,
    TaskReady,
    TaskRunning,
    TaskRetryableFailure,
    TaskRetryScheduled,
    TaskReassigned,
    TaskCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRepoSnapshot {
    pub entry_count: i64,
    pub is_empty: bool,
    pub top_level_entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointRecord {
    pub id: String,
    pub workflow_id: Option<String>,
    pub stage_id: Option<String>,
    pub task_id: Option<String>,
    pub stage_title: Option<String>,
    pub task_title: Option<String>,
    pub assignee_agent_id: Option<String>,
    pub assignee_name: Option<String>,
    pub status: WorkflowCheckpointStatus,
    pub working_directory: String,
    pub repo_snapshot: WorkflowRepoSnapshot,
    pub artifact_summary: Vec<String>,
    pub todo_snapshot: Vec<String>,
    pub resume_hint: Option<String>,
    pub failure_count: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedTask {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub assignee_agent_id: String,
    pub locked_by_user_mention: bool,
    pub depends_on_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStage {
    pub stage: WorkflowStageRecord,
    pub tasks: Vec<PlannedTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPlan {
    pub workflow: WorkflowRecord,
    pub stages: Vec<PlannedStage>,
    pub owner_ack_text: Option<String>,
    pub owner_plan_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerTaskAssignmentDecision {
    pub assignee_agent_id: Option<String>,
    pub owner_ack_text: Option<String>,
    pub owner_dispatch_text: Option<String>,
    pub owner_blocker_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerPlannedTaskDraft {
    pub title: String,
    pub goal: String,
    pub assignee_agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerPlannedStageDraft {
    pub title: String,
    pub goal: String,
    pub execution_mode: WorkflowExecutionMode,
    pub tasks: Vec<OwnerPlannedTaskDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerWorkflowPlanDecision {
    pub workflow_title: Option<String>,
    pub owner_ack_text: Option<String>,
    pub owner_plan_text: Option<String>,
    pub stages: Vec<OwnerPlannedStageDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerStageNarrativeDecision {
    pub transition_text: Option<String>,
    pub dispatch_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerWorkflowSummaryDecision {
    pub summary_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentNarrativeDecision {
    pub text: Option<String>,
    pub progress_percent: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerBlockerDecision {
    pub owner_narrative_text: Option<String>,
    pub resolution: OwnerBlockerResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeStageSummary {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub execution_mode: WorkflowExecutionMode,
    pub status: StageStatus,
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NarrativeEnvelope {
    pub marker: String,
    pub version: i64,
    pub narrative_type: NarrativeMessageType,
    pub text: String,
    pub workflow_id: Option<String>,
    pub stage_id: Option<String>,
    pub task_id: Option<String>,
    pub blocker_id: Option<String>,
    pub stage_title: Option<String>,
    pub task_title: Option<String>,
    pub progress_percent: Option<i64>,
    pub blocked: Option<bool>,
    pub stages: Option<Vec<NarrativeStageSummary>>,
}

impl NarrativeEnvelope {
    pub fn new(narrative_type: NarrativeMessageType, text: impl Into<String>) -> Self {
        Self {
            marker: "nextchat:narrative".into(),
            version: 1,
            narrative_type,
            text: text.into(),
            workflow_id: None,
            stage_id: None,
            task_id: None,
            blocker_id: None,
            stage_title: None,
            task_title: None,
            progress_percent: None,
            blocked: None,
            stages: None,
        }
    }
}
