import type {
  AgentProfile,
  ChatStreamTrack,
  ClaimBid,
  ConversationMessage,
  CreateWorkGroupInput,
  Lease,
  SystemSettings,
  TaskCard,
  ToolManifest,
  ToolRun,
  UpdateWorkGroupInput,
  WorkGroup,
} from "../../types";
import type { Language } from "../../store/preferencesStore";

export interface ChatManagementProps {
  workGroups: WorkGroup[];
  agents: AgentProfile[];
  messages: ConversationMessage[];
  chatStreamTracks: ChatStreamTrack[];
  taskCards: TaskCard[];
  leases: Lease[];
  claimBids: ClaimBid[];
  toolRuns: ToolRun[];
  tools: ToolManifest[];
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
}

export type PanelTarget =
  | { section: "tasks"; taskId?: string }
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
