import { create } from "zustand";
import {
  addAgentToWorkGroup,
  approveToolRun,
  cancelTaskCard,
  createAgentProfile,
  createWorkGroup,
  deleteAgentProfile,
  getDashboardState,
  pauseLease,
  removeAgentFromWorkGroup,
  resumeTaskCard,
  sendHumanMessage,
  subscribeToEvents,
  updateAgentProfile,
  updateSettings,
} from "../lib/tauri";
import type {
  CreateAgentInput,
  CreateWorkGroupInput,
  DashboardState,
  SendHumanMessageInput,
  SystemSettings,
  UpdateAgentInput,
} from "../types";

type Unlisten = () => void | Promise<void>;

interface AppStore extends DashboardState {
  selectedWorkGroupId?: string;
  selectedAgentId?: string;
  selectedTaskId?: string;
  selectedSettingsProviderId: string;
  backstageOpen: boolean;
  loading: boolean;
  error?: string;
  init: () => Promise<void>;
  refresh: () => Promise<void>;
  deleteAgent: (id: string) => Promise<void>;
  setSelectedWorkGroupId: (id?: string) => void;
  setSelectedAgentId: (id?: string) => void;
  setSelectedTaskId: (id?: string) => void;
  setSelectedSettingsProviderId: (id: string) => void;
  toggleBackstage: () => void;
  createAgent: (input: CreateAgentInput) => Promise<void>;
  updateAgent: (input: UpdateAgentInput) => Promise<void>;
  createGroup: (input: CreateWorkGroupInput) => Promise<void>;
  addAgent: (workGroupId: string, agentId: string) => Promise<void>;
  removeAgent: (workGroupId: string, agentId: string) => Promise<void>;
  sendMessage: (input: SendHumanMessageInput) => Promise<void>;
  approveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  cancelTask: (taskCardId: string) => Promise<void>;
  pauseLeaseById: (leaseId: string) => Promise<void>;
  resumeTask: (taskCardId: string) => Promise<void>;
  updateSettings: (settings: SystemSettings) => Promise<void>;
}

let unlisteners: Unlisten[] = [];
let subscriptionsStarted = false;

const emptyState: DashboardState = {
  agents: [],
  workGroups: [],
  messages: [],
  taskCards: [],
  claimBids: [],
  leases: [],
  toolRuns: [],
  auditEvents: [],
  skills: [],
  tools: [],
  memoryItems: [],
  settings: {
    providers: [],
    globalConfig: {
      defaultLLMProvider: "openai",
      defaultLLMModel: "gpt-4o",
      defaultVLMProvider: "gemini",
      defaultVLMModel: "gemini-2.0-flash",
    },
  },
};

async function withRefresh<T>(fn: () => Promise<T>, refresh: () => Promise<void>) {
  await fn();
  await refresh();
}

export const useAppStore = create<AppStore>((set, get) => ({
  ...emptyState,
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
      const listeners = await subscribeToEvents(() => {
        void get().refresh();
      });
      unlisteners = listeners;
      subscriptionsStarted = true;
    }
  },
  async refresh() {
    try {
      set({ loading: true, error: undefined });
      const nextState = await getDashboardState();
      const selectedWorkGroupId =
        get().selectedWorkGroupId ?? nextState.workGroups[0]?.id;
      set({
        ...nextState,
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
  async pauseLeaseById(leaseId) {
    await withRefresh(() => pauseLease(leaseId), get().refresh);
  },
  async resumeTask(taskCardId) {
    await withRefresh(() => resumeTaskCard(taskCardId), get().refresh);
  },
  async updateSettings(settings) {
    await withRefresh(() => updateSettings(settings), get().refresh);
  },
}));

export function disposeAppStore() {
  unlisteners.forEach((stop) => stop());
  unlisteners = [];
  subscriptionsStarted = false;
}
