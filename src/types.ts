export type SenderKind = "human" | "agent" | "system";
export type MessageKind =
  | "text"
  | "summary"
  | "tool_call"
  | "tool_result"
  | "approval"
  | "status";
export type Visibility = "main" | "backstage";
export type ExecutionMode = "real_model" | "fallback";
export type WorkGroupKind = "persistent" | "ephemeral";
export type TaskStatus =
  | "pending"
  | "bidding"
  | "leased"
  | "waiting_children"
  | "waiting_approval"
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

export interface AgentProfile {
  id: string;
  name: string;
  avatar: string;
  role: string;
  objective: string;
  modelPolicy: ModelPolicy;
  skillIds: string[];
  toolIds: string[];
  maxParallelRuns: number;
  canSpawnSubtasks: boolean;
  memoryPolicy: MemoryPolicy;
}

export interface WorkGroup {
  id: string;
  kind: WorkGroupKind;
  name: string;
  goal: string;
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
  expectedTools: string[];
  estimatedCost: number;
  createdAt: string;
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

export interface SkillPack {
  id: string;
  name: string;
  promptTemplate: string;
  planningRules: string[];
  allowedToolTags: string[];
  doneCriteria: string[];
}

export interface MemoryItem {
  id: string;
  scope: "user" | "work_group" | "agent";
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
  skillIds: string[];
  toolIds: string[];
  maxParallelRuns: number;
  canSpawnSubtasks: boolean;
}

export interface UpdateAgentInput extends CreateAgentInput {
  id: string;
}

export interface CreateWorkGroupInput {
  name: string;
  goal: string;
  kind: WorkGroupKind;
  defaultVisibility: "verbose" | "summary";
  autoArchive: boolean;
}

export interface SendHumanMessageInput {
  workGroupId: string;
  content: string;
}
