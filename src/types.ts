export type SenderKind = "human" | "agent" | "system";
export type MessageKind =
  | "text"
  | "summary"
  | "tool_call"
  | "tool_result"
  | "collaboration"
  | "approval"
  | "status";
export type Visibility = "main" | "backstage";
export type ExecutionMode = "real_model" | "fallback";
export type ChatStreamPhase = "start" | "delta" | "done";
export type ChatStreamStatus = "streaming" | "completed";
export type WorkGroupKind = "persistent" | "ephemeral";
export type TaskStatus =
  | "pending"
  | "bidding"
  | "leased"
  | "waiting_children"
  | "waiting_approval"
  | "waiting_user_input"
  | "in_progress"
  | "paused"
  | "cancelled"
  | "completed"
  | "needs_review";
export type LeaseState = "active" | "paused" | "released" | "preempt_requested";
export type ToolRiskLevel = "low" | "medium" | "high";
export type ToolRunState =
  | "pending_approval"
  | "queued"
  | "running"
  | "completed"
  | "cancelled";
export type PendingUserQuestionStatus = "pending" | "answered" | "cancelled";
export type BlockerResolutionTarget = "owner" | "user";
export type BlockerCategory =
  | "missing_dependency"
  | "missing_context"
  | "permission_required"
  | "tool_failure"
  | "design_conflict"
  | "need_user_decision";
export type BlockerStatus = "open" | "resolved" | "cancelled";

export interface ModelPolicy {
  provider: string;
  model: string;
  temperature: number;
}

export interface MemoryPolicy {
  readScope: string[];
  writeScope: string[];
  pinnedMemoryIds: string[];
}

export interface AgentPermissionPolicy {
  allowToolIds: string[];
  denyToolIds: string[];
  requireApprovalToolIds: string[];
  allowFsRoots: string[];
  allowNetworkDomains: string[];
}

export interface AgentProfile {
  id: string;
  name: string;
  avatar: string;
  role: string;
  objective: string;
  modelPolicy: ModelPolicy;
  toolIds: string[];
  maxParallelRuns: number;
  canSpawnSubtasks: boolean;
  memoryPolicy: MemoryPolicy;
  permissionPolicy: AgentPermissionPolicy;
}

export interface WorkGroup {
  id: string;
  kind: WorkGroupKind;
  name: string;
  goal: string;
  workingDirectory: string;
  memberAgentIds: string[];
  defaultVisibility: "verbose" | "summary";
  autoArchive: boolean;
  createdAt: string;
  archivedAt?: string | null;
}

export interface ConversationMessage {
  id: string;
  conversationId: string;
  workGroupId: string;
  senderKind: SenderKind;
  senderId: string;
  senderName: string;
  kind: MessageKind;
  visibility: Visibility;
  content: string;
  mentions: string[];
  taskCardId?: string | null;
  executionMode?: ExecutionMode | null;
  createdAt: string;
}

export interface ChatStreamEvent {
  streamId: string;
  phase: ChatStreamPhase;
  conversationId: string;
  workGroupId: string;
  senderId: string;
  senderName: string;
  kind: MessageKind;
  visibility: Visibility;
  taskCardId?: string | null;
  sequence: number;
  delta?: string | null;
  fullContent?: string | null;
  createdAt: string;
}

export interface ChatStreamTrack {
  streamId: string;
  conversationId: string;
  workGroupId: string;
  senderId: string;
  senderName: string;
  kind: MessageKind;
  visibility: Visibility;
  taskCardId?: string | null;
  status: ChatStreamStatus;
  content: string;
  lastSequence: number;
  replaceOnNextDelta: boolean;
  startedAt: string;
  updatedAt: string;
}

export interface TaskCard {
  id: string;
  parentId?: string | null;
  sourceMessageId: string;
  title: string;
  normalizedGoal: string;
  inputPayload: string;
  priority: number;
  status: TaskStatus;
  workGroupId: string;
  createdBy: string;
  assignedAgentId?: string | null;
  createdAt: string;
}

export interface ClaimBid {
  id: string;
  taskCardId: string;
  agentId: string;
  rationale: string;
  capabilityScore: number;
  scoreBreakdown: ClaimScoreBreakdown;
  expectedTools: string[];
  estimatedCost: number;
  createdAt: string;
}

export interface ClaimScoreBreakdown {
  factors: ClaimScoreFactor[];
}

export type ClaimScoreFactorKind =
  | "base"
  | "mention"
  | "capacity"
  | "over_capacity"
  | "role_match"
  | "tool_coverage"
  | "tool_mismatch"
  | "skill_match"
  | "load_penalty";

export interface ClaimScoreFactor {
  kind: ClaimScoreFactorKind;
  score: number;
  detail: string;
}

export interface Lease {
  id: string;
  taskCardId: string;
  ownerAgentId: string;
  state: LeaseState;
  grantedAt: string;
  expiresAt?: string | null;
  preemptRequestedAt?: string | null;
  releasedAt?: string | null;
}

export interface ToolManifest {
  id: string;
  name: string;
  category: string;
  riskLevel: ToolRiskLevel;
  inputSchema: string;
  outputSchema: string;
  timeoutMs: number;
  concurrencyLimit: number;
  permissions: string[];
  description: string;
}

export interface ToolRun {
  id: string;
  toolId: string;
  taskCardId: string;
  agentId: string;
  state: ToolRunState;
  approvalRequired: boolean;
  startedAt?: string | null;
  finishedAt?: string | null;
  resultRef?: string | null;
}

export interface PendingUserQuestion {
  id: string;
  workGroupId: string;
  taskCardId: string;
  agentId: string;
  toolRunId?: string | null;
  question: string;
  options: string[];
  context?: string | null;
  allowFreeForm: boolean;
  askedMessageId: string;
  answerMessageId?: string | null;
  status: PendingUserQuestionStatus;
  createdAt: string;
  answeredAt?: string | null;
}

export interface TaskBlockerRecord {
  id: string;
  taskId: string;
  workflowId?: string | null;
  raisedByAgentId: string;
  resolutionTarget: BlockerResolutionTarget;
  category: BlockerCategory;
  summary: string;
  details: string;
  status: BlockerStatus;
  createdAt: string;
  resolvedAt?: string | null;
}

export type WorkflowCheckpointStatus =
  | "workflowPlanned"
  | "workflowRunning"
  | "workflowCompleted"
  | "stagePending"
  | "stageRunning"
  | "stageCompleted"
  | "taskReady"
  | "taskRunning"
  | "taskRetryableFailure"
  | "taskRetryScheduled"
  | "taskReassigned"
  | "taskCompleted";

export interface WorkflowRepoSnapshot {
  entryCount: number;
  isEmpty: boolean;
  topLevelEntries: string[];
}

export interface WorkflowCheckpointRecord {
  id: string;
  workflowId?: string | null;
  stageId?: string | null;
  taskId?: string | null;
  stageTitle?: string | null;
  taskTitle?: string | null;
  assigneeAgentId?: string | null;
  assigneeName?: string | null;
  status: WorkflowCheckpointStatus;
  workingDirectory: string;
  repoSnapshot: WorkflowRepoSnapshot;
  artifactSummary: string[];
  todoSnapshot: string[];
  resumeHint?: string | null;
  failureCount: number;
  lastError?: string | null;
  createdAt: string;
  updatedAt: string;
}

export type OwnerBlockerResolution =
  | { action: "provide_context"; message: string }
  | { action: "reassign_task"; targetAgentId: string; message: string }
  | {
      action: "create_dependency_task";
      targetAgentId: string;
      title: string;
      goal: string;
      message: string;
    }
  | {
      action: "request_approval";
      question: string;
      options: string[];
      context?: string | null;
      allowFreeForm?: boolean | null;
    }
  | {
      action: "ask_user";
      question: string;
      options: string[];
      context?: string | null;
      allowFreeForm?: boolean | null;
    }
  | { action: "pause_task"; message: string };

export interface SkillPack {
  id: string;
  name: string;
  promptTemplate: string;
  planningRules: string[];
  allowedToolTags: string[];
  doneCriteria: string[];
  enabled: boolean;
  editable: boolean;
  source: string;
  installPath?: string | null;
}

export interface SkillFileEntry {
  path: string;
  size: number;
  isBinary: boolean;
}

export interface SkillDetail {
  skillId: string;
  enabled: boolean;
  source: string;
  installPath: string;
  name: string;
  description: string;
  argumentHint?: string | null;
  userInvocable: boolean;
  disableModelInvocation: boolean;
  allowedTools?: string | null;
  model?: string | null;
  context?: string | null;
  agent?: string | null;
  hooksJson?: string | null;
  summary?: string | null;
  content: string;
  files: SkillFileEntry[];
}

export interface UpdateSkillDetailInput {
  skillId: string;
  enabled: boolean;
  name: string;
  description: string;
  argumentHint?: string | null;
  userInvocable: boolean;
  disableModelInvocation: boolean;
  allowedTools?: string | null;
  model?: string | null;
  context?: string | null;
  agent?: string | null;
  hooksJson?: string | null;
  summary?: string | null;
  content: string;
}

export interface MemoryItem {
  id: string;
  scope: "user" | "work_group" | "agent" | "task";
  scopeId: string;
  content: string;
  tags: string[];
  pinned: boolean;
  ttl?: number | null;
  createdAt: string;
}

export interface AuditEvent {
  id: string;
  eventType: string;
  entityType: string;
  entityId: string;
  payloadJson: string;
  createdAt: string;
}

export interface AIProviderConfig {
  id: string;
  name: string;
  icon: string;
  enabled: boolean;
  rigProviderType: string;
  apiKey: string;
  baseUrl: string;
  models: string[];
  defaultModel: string;
  maxContextLength: number;
  customHeaders: string;
  temperature: number;
  maxTokens: number;
  outputTokenLimit: number;
  maxDialogRounds: number;
}

export interface AIGlobalConfig {
  defaultLLMProvider: string;
  defaultLLMModel: string;
  defaultVLMProvider: string;
  defaultVLMModel: string;
  maskApiKeys: boolean;
  enableAuditLog: boolean;
  proxyUrl: string;
}

export interface SystemSettings {
  providers: AIProviderConfig[];
  globalConfig: AIGlobalConfig;
}

export interface DashboardState {
  agents: AgentProfile[];
  workGroups: WorkGroup[];
  messages: ConversationMessage[];
  taskCards: TaskCard[];
  pendingUserQuestions: PendingUserQuestion[];
  taskBlockers: TaskBlockerRecord[];
  workflowCheckpoints: WorkflowCheckpointRecord[];
  claimBids: ClaimBid[];
  leases: Lease[];
  toolRuns: ToolRun[];
  auditEvents: AuditEvent[];
  skills: SkillPack[];
  tools: ToolManifest[];
  memoryItems: MemoryItem[];
  settings: SystemSettings;
}

export interface CreateAgentInput {
  name: string;
  avatar: string;
  role: string;
  objective: string;
  provider: string;
  model: string;
  temperature: number;
  toolIds: string[];
  maxParallelRuns: number;
  canSpawnSubtasks: boolean;
  memoryPolicy: MemoryPolicy;
  permissionPolicy: AgentPermissionPolicy;
}

export interface UpdateAgentInput extends CreateAgentInput {
  id: string;
}

export interface CreateWorkGroupInput {
  name: string;
  goal: string;
  workingDirectory: string;
  kind: WorkGroupKind;
  defaultVisibility: "verbose" | "summary";
  autoArchive: boolean;
  memberAgentIds?: string[];
}

export interface UpdateWorkGroupInput extends CreateWorkGroupInput {
  id: string;
}

export interface SendHumanMessageInput {
  workGroupId: string;
  content: string;
}
