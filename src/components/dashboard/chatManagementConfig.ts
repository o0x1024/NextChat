import type {
  AgentProfile,
  AuditEvent,
  ChatStreamTrack,
  ClaimBid,
  ConversationMessage,
  CreateWorkGroupInput,
  Lease,
  OwnerBlockerResolution,
  PendingUserQuestion,
  SystemSettings,
  TaskCard,
  TaskBlockerRecord,
  ToolManifest,
  ToolRun,
  UpdateWorkGroupInput,
  WorkflowCheckpointRecord,
  WorkGroup,
} from "../../types";
import type { Language } from "../../store/preferencesStore";

export interface ChatManagementProps {
  workGroups: WorkGroup[];
  agents: AgentProfile[];
  messages: ConversationMessage[];
  chatStreamTracks: ChatStreamTrack[];
  taskCards: TaskCard[];
  pendingUserQuestions: PendingUserQuestion[];
  taskBlockers: TaskBlockerRecord[];
  workflowCheckpoints: WorkflowCheckpointRecord[];
  leases: Lease[];
  claimBids: ClaimBid[];
  toolRuns: ToolRun[];
  tools: ToolManifest[];
  auditEvents: AuditEvent[];
  settings: SystemSettings;
  selectedWorkGroupId?: string;
  language: Language;
  onSelectWorkGroup: (id: string) => void;
  onCreateGroup: (input: CreateWorkGroupInput) => Promise<void>;
  onDeleteGroup: (workGroupId: string) => Promise<void>;
  onClearGroupHistory: (workGroupId: string) => Promise<void>;
  onUpdateGroup: (input: UpdateWorkGroupInput) => Promise<void>;
  onSendMessage: (workGroupId: string, content: string) => Promise<void>;
  onAddAgent: (workGroupId: string, agentId: string) => Promise<void>;
  onRemoveAgent: (workGroupId: string, agentId: string) => Promise<void>;
  onApproveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  onCancelTask: (taskCardId: string) => Promise<void>;
  onResolveBlocker: (blockerId: string, resolution: OwnerBlockerResolution) => Promise<void>;
}

export type PanelTarget =
  | { section: "tasks"; taskId?: string }
  | { section: "blockers"; blockerId: string }
  | { section: "approvals" };

export const emptyGroupForm: CreateWorkGroupInput = {
  name: "",
  goal: "",
  workingDirectory: ".",
  kind: "persistent",
  defaultVisibility: "summary",
  autoArchive: false,
  memberAgentIds: [],
};

export const emptyEditGroupForm: UpdateWorkGroupInput = {
  id: "",
  ...emptyGroupForm,
};
