import { create } from "zustand";
import {
  addAgentToWorkGroup,
  approveToolRun,
  cancelTaskCard,
  cancelWorkflow,
  clearWorkGroupHistory,
  createAgentProfile,
  createWorkGroup,
  deleteWorkGroup,
  deleteAgentProfile,
  deleteInstalledSkill,
  deleteInstalledSkillFile,
  getInstalledSkillDetail,
  getDashboardState,
  pauseLease,
  pauseWorkflow,
  readInstalledSkillFile,
  removeAgentFromWorkGroup,
  resolveOwnerBlocker,
  resumeTaskCard,
  resumeWorkflow,
  sendHumanMessage,
  skipWorkflowStage,
  addWorkflowStage,
  updateWorkflowStage,
  removeWorkflowStage,
  installSkillFromGithub,
  installSkillFromLocal,
  setInstalledSkillEnabled,
  type DashboardEventName,
  type DashboardEventPayloadMap,
  subscribeToEvents,
  updateSkillDetail,
  updateWorkGroup,
  updateInstalledSkill,
  updateAgentProfile,
  updateSettings as updateSettingsCommand,
  upsertInstalledSkillFile,
} from "../lib/tauri";
import type {
  AuditEvent,
  ChatStreamEvent,
  ChatStreamTrack,
  ClaimBid,
  ConversationMessage,
  CreateAgentInput,
  CreateWorkGroupInput,
  DashboardState,
  Lease,
  OwnerBlockerResolution,
  PendingUserQuestion,
  SendHumanMessageInput,
  SkillDetail,
  SystemSettings,
  TaskCard,
  TaskBlockerRecord,
  ToolRun,
  UpdateAgentInput,
  UpdateSkillDetailInput,
  UpdateWorkGroupInput,
  AddWorkflowStageInput,
  UpdateWorkflowStageInput,
} from "../types";

type Unlisten = () => void | Promise<void>;

export interface ToastState {
  message: string;
  type: 'success' | 'error' | 'info' | 'warning';
}

interface AppStore extends DashboardState {
  chatStreamTracks: ChatStreamTrack[];
  selectedWorkGroupId?: string;
  selectedAgentId?: string;
  selectedTaskId?: string;
  selectedSettingsProviderId: string;
  backstageOpen: boolean;
  loading: boolean;
  error?: string;
  toast?: ToastState;
  init: () => Promise<void>;
  refresh: (withLoading?: boolean) => Promise<void>;
  deleteAgent: (id: string) => Promise<void>;
  setSelectedWorkGroupId: (id?: string) => void;
  setSelectedAgentId: (id?: string) => void;
  setSelectedTaskId: (id?: string) => void;
  setSelectedSettingsProviderId: (id: string) => void;
  toggleBackstage: () => void;
  showToast: (message: string, type?: 'success' | 'error' | 'info' | 'warning') => void;
  clearToast: () => void;
  createAgent: (input: CreateAgentInput) => Promise<void>;
  updateAgent: (input: UpdateAgentInput) => Promise<void>;
  createGroup: (input: CreateWorkGroupInput) => Promise<void>;
  deleteGroup: (workGroupId: string) => Promise<void>;
  clearGroupHistory: (workGroupId: string) => Promise<void>;
  updateGroup: (input: UpdateWorkGroupInput) => Promise<void>;
  addAgent: (workGroupId: string, agentId: string) => Promise<void>;
  removeAgent: (workGroupId: string, agentId: string) => Promise<void>;
  sendMessage: (input: SendHumanMessageInput) => Promise<void>;
  approveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  cancelTask: (taskCardId: string) => Promise<void>;
  resolveBlocker: (blockerId: string, resolution: OwnerBlockerResolution) => Promise<void>;
  pauseLeaseById: (leaseId: string) => Promise<void>;
  resumeTask: (taskCardId: string) => Promise<void>;
  cancelWorkflow: (workflowId: string) => Promise<void>;
  pauseWorkflow: (workflowId: string) => Promise<void>;
  resumeWorkflow: (workflowId: string) => Promise<void>;
  skipWorkflowStage: (workflowId: string, stageId: string) => Promise<void>;
  addWorkflowStage: (input: AddWorkflowStageInput) => Promise<void>;
  updateWorkflowStage: (input: UpdateWorkflowStageInput) => Promise<void>;
  removeWorkflowStage: (stageId: string) => Promise<void>;
  updateSettings: (settings: SystemSettings) => Promise<void>;
  installSkillFromGithub: (source: string, skillPath?: string) => Promise<number>;
  installSkillFromLocal: (sourcePath: string) => Promise<number>;
  updateInstalledSkill: (skillId: string, name?: string, promptTemplate?: string) => Promise<void>;
  setInstalledSkillEnabled: (skillId: string, enabled: boolean) => Promise<void>;
  deleteInstalledSkill: (skillId: string) => Promise<void>;
  getInstalledSkillDetail: (skillId: string) => Promise<SkillDetail>;
  updateSkillDetail: (input: UpdateSkillDetailInput) => Promise<SkillDetail>;
  readInstalledSkillFile: (skillId: string, relativePath: string) => Promise<string>;
  upsertInstalledSkillFile: (skillId: string, relativePath: string, content: string) => Promise<void>;
  deleteInstalledSkillFile: (skillId: string, relativePath: string) => Promise<void>;
}

let unlisteners: Unlisten[] = [];
let subscriptionsStarted = false;
let subscriptionsPromise: Promise<void> | null = null;
let settingsUpdateQueue: Promise<void> = Promise.resolve();

const emptyState: DashboardState = {
  agents: [],
  workGroups: [],
  messages: [],
  taskCards: [],
  pendingUserQuestions: [],
  claimBids: [],
  leases: [],
  toolRuns: [],
  taskBlockers: [],
  workflowCheckpoints: [],
  auditEvents: [],
  skills: [],
  tools: [],
  memoryItems: [],
  workflows: [],
  workflowStages: [],
  settings: {
    providers: [],
    globalConfig: {
      defaultLLMProvider: "openai",
      defaultLLMModel: "gpt-4o",
      defaultVLMProvider: "gemini",
      defaultVLMModel: "gemini-2.0-flash",
      maskApiKeys: true,
      enableAuditLog: true,
      proxyUrl: "",
    },
  },
};

function upsertChatStreamTrack(
  tracks: ChatStreamTrack[],
  event: ChatStreamEvent,
): ChatStreamTrack[] {
  const existing = tracks.find((track) => track.streamId === event.streamId);
  const baseTrack: ChatStreamTrack = existing ?? {
    streamId: event.streamId,
    conversationId: event.conversationId,
    workGroupId: event.workGroupId,
    senderId: event.senderId,
    senderName: event.senderName,
    kind: event.kind,
    visibility: event.visibility,
    taskCardId: event.taskCardId,
    status: "streaming",
    content: "",
    lastSequence: -1,
    replaceOnNextDelta: false,
    startedAt: event.createdAt,
    updatedAt: event.createdAt,
  };

  if (existing && event.sequence <= existing.lastSequence) {
    return tracks;
  }

  let nextTrack = {
    ...baseTrack,
    conversationId: event.conversationId,
    workGroupId: event.workGroupId,
    senderId: event.senderId,
    senderName: event.senderName,
    kind: event.kind,
    visibility: event.visibility,
    taskCardId: event.taskCardId,
    lastSequence: event.sequence,
    updatedAt: event.createdAt,
  };

  if (event.phase === "delta") {
    nextTrack = {
      ...nextTrack,
      content: baseTrack.replaceOnNextDelta
        ? (event.delta ?? "")
        : `${baseTrack.content}${event.delta ?? ""}`,
      replaceOnNextDelta: false,
      status: "streaming",
    };
  } else if (event.phase === "done") {
    nextTrack = {
      ...nextTrack,
      content: event.fullContent ?? `${baseTrack.content}${event.delta ?? ""}`,
      replaceOnNextDelta: false,
      status: "completed",
    };
  } else {
    nextTrack = {
      ...nextTrack,
      content: existing ? baseTrack.content : "",
      replaceOnNextDelta: Boolean(existing),
      lastSequence: event.sequence,
      status: "streaming",
      startedAt: event.createdAt,
    };
  }

  const nextTracks = tracks.filter((track) => track.streamId !== event.streamId);
  return [nextTrack, ...nextTracks].sort((left, right) =>
    right.updatedAt.localeCompare(left.updatedAt),
  );
}

function pruneChatStreamTracks(tracks: ChatStreamTrack[], messageIds: Set<string>) {
  return tracks.filter((track) => !messageIds.has(track.streamId));
}

function upsertById<T extends { id: string }>(
  items: T[],
  nextItem: T,
  compare: (left: T, right: T) => number,
) {
  return [...items.filter((item) => item.id !== nextItem.id), nextItem].sort(compare);
}

function sortMessages(left: ConversationMessage, right: ConversationMessage) {
  return left.createdAt.localeCompare(right.createdAt);
}

function sortTaskCards(left: TaskCard, right: TaskCard) {
  return right.createdAt.localeCompare(left.createdAt);
}

function sortClaimBids(left: ClaimBid, right: ClaimBid) {
  return left.createdAt.localeCompare(right.createdAt);
}

function sortLeases(left: Lease, right: Lease) {
  return left.grantedAt.localeCompare(right.grantedAt);
}

function sortToolRuns(left: ToolRun, right: ToolRun) {
  return (left.startedAt ?? "").localeCompare(right.startedAt ?? "");
}

function sortAuditEvents(left: AuditEvent, right: AuditEvent) {
  return right.createdAt.localeCompare(left.createdAt);
}

function sortPendingUserQuestions(left: PendingUserQuestion, right: PendingUserQuestion) {
  return right.createdAt.localeCompare(left.createdAt);
}

function reduceDashboardEventState(
  state: AppStore,
  eventName: DashboardEventName,
  payload: DashboardEventPayloadMap[DashboardEventName],
) {
  switch (eventName) {
    case "chat:message-created": {
      const nextMessage = payload as ConversationMessage;
      const messages = upsertById(state.messages, nextMessage, sortMessages);
      return {
        messages,
        chatStreamTracks: pruneChatStreamTracks(
          state.chatStreamTracks,
          new Set(messages.map((message) => message.id)),
        ),
      };
    }
    case "task:card-created":
    case "task:status-changed":
      return {
        taskCards: upsertById(state.taskCards, payload as TaskCard, sortTaskCards),
      };
    case "pending-user-question:updated": {
      const nextQuestion = payload as PendingUserQuestion;
      if (nextQuestion.status !== "pending") {
        return {
          pendingUserQuestions: state.pendingUserQuestions.filter(
            (question) => question.id !== nextQuestion.id,
          ),
        };
      }
      return {
        pendingUserQuestions: upsertById(
          state.pendingUserQuestions,
          nextQuestion,
          sortPendingUserQuestions,
        ),
      };
    }
    case "claim:bid-submitted":
      return {
        claimBids: upsertById(state.claimBids, payload as ClaimBid, sortClaimBids),
      };
    case "lease:granted":
    case "lease:preempt-requested":
      return {
        leases: upsertById(state.leases, payload as Lease, sortLeases),
      };
    case "tool:run-started":
    case "tool:run-completed":
    case "approval:requested":
      return {
        toolRuns: upsertById(state.toolRuns, payload as ToolRun, sortToolRuns),
      };
    case "audit:event-created":
      return {
        auditEvents: upsertById(state.auditEvents, payload as AuditEvent, sortAuditEvents),
      };
    default:
      return null;
  }
}

async function withRefresh<T>(fn: () => Promise<T>, refresh: () => Promise<void>) {
  await fn();
  await refresh();
}

export const useAppStore = create<AppStore>((set, get) => ({
  ...emptyState,
  chatStreamTracks: [],
  selectedWorkGroupId: undefined,
  selectedAgentId: undefined,
  selectedTaskId: undefined,
  selectedSettingsProviderId: "openai",
  backstageOpen: false,
  loading: true,
  error: undefined,
  async init() {
    await get().refresh();
    if (!subscriptionsStarted) {
      if (!subscriptionsPromise) {
        subscriptionsPromise = (async () => {
          const listeners = await subscribeToEvents({
            onDashboardEvent: (eventName, payload) => {
              let handled = false;
              set((state) => {
                const nextState = reduceDashboardEventState(state, eventName, payload);
                handled = nextState !== null;
                return nextState ? { ...state, ...nextState } : state;
              });
              if (!handled) {
                void get().refresh(false);
              }
            },
            onChatStreamEvent: (_eventName, payload) => {
              set((state) => ({
                chatStreamTracks: upsertChatStreamTrack(
                  pruneChatStreamTracks(
                    state.chatStreamTracks,
                    new Set(state.messages.map((message) => message.id)),
                  ),
                  payload,
                ),
              }));
            },
          });
          unlisteners = listeners;
          subscriptionsStarted = true;
        })()
          .catch((error) => {
            subscriptionsStarted = false;
            throw error;
          })
          .finally(() => {
            subscriptionsPromise = null;
          });
      }
      await subscriptionsPromise;
    }
  },
  async refresh(withLoading = true) {
    try {
      if (withLoading) {
        set({ loading: true, error: undefined });
      } else {
        set({ error: undefined });
      }
      const nextState = await getDashboardState();
      const selectedWorkGroupId =
        get().selectedWorkGroupId ?? nextState.workGroups[0]?.id;
      const persistedMessageIds = new Set(nextState.messages.map((message) => message.id));
      set({
        ...nextState,
        chatStreamTracks: pruneChatStreamTracks(get().chatStreamTracks, persistedMessageIds),
        selectedWorkGroupId,
        selectedAgentId: get().selectedAgentId,
        selectedTaskId: get().selectedTaskId,
        backstageOpen: get().backstageOpen,
        loading: false,
      });
    } catch (error) {
      set({
        loading: false,
        error: error instanceof Error ? error.message : "Unknown error",
      });
    }
  },
  setSelectedWorkGroupId(id) {
    set({ selectedWorkGroupId: id, selectedTaskId: undefined });
  },
  setSelectedAgentId(id) {
    set({ selectedAgentId: id });
  },
  setSelectedTaskId(id) {
    set({ selectedTaskId: id });
  },
  setSelectedSettingsProviderId(id) {
    set({ selectedSettingsProviderId: id });
  },
  toggleBackstage() {
    set((state) => ({ backstageOpen: !state.backstageOpen }));
  },
  showToast(message, type = 'success') {
    set({ toast: { message, type } });
    setTimeout(() => {
      set((state) => (state.toast?.message === message ? { toast: undefined } : state));
    }, 3000);
  },
  clearToast() {
    set({ toast: undefined });
  },
  async createAgent(input) {
    const agent = await createAgentProfile(input);
    await get().refresh();
    set({ selectedAgentId: agent.id });
  },
  async updateAgent(input) {
    const agent = await updateAgentProfile(input);
    await get().refresh();
    set({ selectedAgentId: agent.id });
  },
  async deleteAgent(id) {
    await deleteAgentProfile(id);
    await get().refresh();
    set({ selectedAgentId: undefined });
  },
  async createGroup(input) {
    const group = await createWorkGroup(input);
    await get().refresh();
    set({ selectedWorkGroupId: group.id, selectedTaskId: undefined });
  },
  async deleteGroup(workGroupId) {
    await deleteWorkGroup(workGroupId);
    await get().refresh();
    const fallbackGroupId = get().workGroups[0]?.id;
    set({ selectedWorkGroupId: fallbackGroupId, selectedTaskId: undefined });
  },
  async clearGroupHistory(workGroupId) {
    await clearWorkGroupHistory(workGroupId);
    await get().refresh();
    set({ selectedWorkGroupId: workGroupId, selectedTaskId: undefined });
  },
  async updateGroup(input) {
    const group = await updateWorkGroup(input);
    await get().refresh();
    set({ selectedWorkGroupId: group.id, selectedTaskId: undefined });
  },
  async addAgent(workGroupId, agentId) {
    await withRefresh(() => addAgentToWorkGroup(workGroupId, agentId), get().refresh);
  },
  async removeAgent(workGroupId, agentId) {
    await withRefresh(() => removeAgentFromWorkGroup(workGroupId, agentId), get().refresh);
  },
  async sendMessage(input) {
    await withRefresh(() => sendHumanMessage(input), get().refresh);
  },
  async approveRun(toolRunId, approved) {
    await withRefresh(() => approveToolRun(toolRunId, approved), get().refresh);
  },
  async cancelTask(taskCardId) {
    await withRefresh(() => cancelTaskCard(taskCardId), get().refresh);
  },
  async resolveBlocker(blockerId, resolution) {
    await withRefresh(() => resolveOwnerBlocker(blockerId, resolution), get().refresh);
  },
  async pauseLeaseById(leaseId) {
    await withRefresh(() => pauseLease(leaseId), get().refresh);
  },
  async resumeTask(taskCardId) {
    await withRefresh(() => resumeTaskCard(taskCardId), get().refresh);
  },
  async cancelWorkflow(workflowId) {
    await withRefresh(() => cancelWorkflow(workflowId), get().refresh);
  },
  async pauseWorkflow(workflowId) {
    await withRefresh(() => pauseWorkflow(workflowId), get().refresh);
  },
  async resumeWorkflow(workflowId) {
    await withRefresh(() => resumeWorkflow(workflowId), get().refresh);
  },
  async skipWorkflowStage(workflowId, stageId) {
    await withRefresh(() => skipWorkflowStage(workflowId, stageId), get().refresh);
  },
  async addWorkflowStage(input) {
    await withRefresh(() => addWorkflowStage(input), get().refresh);
  },
  async updateWorkflowStage(input) {
    await withRefresh(() => updateWorkflowStage(input), get().refresh);
  },
  async removeWorkflowStage(stageId) {
    await withRefresh(() => removeWorkflowStage(stageId), get().refresh);
  },
  async updateSettings(settings) {
    // Optimistically reflect settings changes in UI to avoid stale reads between rapid edits.
    set((state) => ({
      ...state,
      settings,
      error: undefined,
    }));

    settingsUpdateQueue = settingsUpdateQueue
      .catch(() => undefined)
      .then(async () => {
        await updateSettingsCommand(settings);
        await get().refresh(false);
      });

    return settingsUpdateQueue;
  },
  async installSkillFromGithub(source, skillPath) {
    const skills = await installSkillFromGithub(source, skillPath);
    await get().refresh();
    return skills.length;
  },
  async installSkillFromLocal(sourcePath) {
    const skills = await installSkillFromLocal(sourcePath);
    await get().refresh();
    return skills.length;
  },
  async updateInstalledSkill(skillId, name, promptTemplate) {
    await withRefresh(
      () => updateInstalledSkill(skillId, name, promptTemplate),
      get().refresh,
    );
  },
  async setInstalledSkillEnabled(skillId, enabled) {
    await withRefresh(() => setInstalledSkillEnabled(skillId, enabled), get().refresh);
  },
  async deleteInstalledSkill(skillId) {
    await withRefresh(() => deleteInstalledSkill(skillId), get().refresh);
  },
  async getInstalledSkillDetail(skillId) {
    return getInstalledSkillDetail(skillId);
  },
  async updateSkillDetail(input) {
    const detail = await updateSkillDetail(input);
    await get().refresh();
    return detail;
  },
  async readInstalledSkillFile(skillId, relativePath) {
    return readInstalledSkillFile(skillId, relativePath);
  },
  async upsertInstalledSkillFile(skillId, relativePath, content) {
    await withRefresh(
      () => upsertInstalledSkillFile(skillId, relativePath, content),
      get().refresh,
    );
  },
  async deleteInstalledSkillFile(skillId, relativePath) {
    await withRefresh(
      () => deleteInstalledSkillFile(skillId, relativePath),
      get().refresh,
    );
  },
}));

export function disposeAppStore() {
  unlisteners.forEach((stop) => stop());
  unlisteners = [];
  subscriptionsStarted = false;
}
