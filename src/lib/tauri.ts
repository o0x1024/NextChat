import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { documentDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AIProviderConfig,
  AIGlobalConfig,
  AgentProfile,
  ChatStreamEvent,
  CreateAgentInput,
  CreateWorkGroupInput,
  DashboardState,
  SendHumanMessageInput,
  SkillDetail,
  UpdateSkillDetailInput,
  SkillPack,
  SystemSettings,
  UpdateAgentInput,
  UpdateWorkGroupInput,
  WorkGroup,
} from "../types";

type RawGlobalConfig = Partial<
  AIGlobalConfig & {
    defaultLlmProvider: string;
    defaultLlmModel: string;
    defaultVlmProvider: string;
    defaultVlmModel: string;
  }
>;

function normalizeGlobalConfig(globalConfig: RawGlobalConfig | undefined): AIGlobalConfig {
  const source = globalConfig ?? {};
  const defaultLLMProvider =
    source.defaultLLMProvider ?? source.defaultLlmProvider ?? "openai";
  const defaultLLMModel = source.defaultLLMModel ?? source.defaultLlmModel ?? "gpt-4o";
  const defaultVLMProvider = source.defaultVLMProvider ?? source.defaultVlmProvider ?? "gemini";
  const defaultVLMModel =
    source.defaultVLMModel ?? source.defaultVlmModel ?? "gemini-2.0-flash";

  return {
    defaultLLMProvider,
    defaultLLMModel,
    defaultVLMProvider,
    defaultVLMModel,
    maskApiKeys: source.maskApiKeys ?? true,
    enableAuditLog: source.enableAuditLog ?? true,
    proxyUrl: source.proxyUrl ?? "",
  };
}

function normalizeSystemSettings<T extends { settings: { globalConfig?: RawGlobalConfig } }>(
  payload: T,
): T {
  return {
    ...payload,
    settings: {
      ...payload.settings,
      globalConfig: normalizeGlobalConfig(payload.settings?.globalConfig),
    },
  };
}

function buildInteropGlobalConfig(globalConfig: AIGlobalConfig): Record<string, unknown> {
  return {
    ...globalConfig,
    defaultLlmProvider: globalConfig.defaultLLMProvider,
    defaultLlmModel: globalConfig.defaultLLMModel,
    defaultVlmProvider: globalConfig.defaultVLMProvider,
    defaultVlmModel: globalConfig.defaultVLMModel,
  };
}

export const dashboardEventNames = [
  "chat:message-created",
  "task:card-created",
  "claim:bid-submitted",
  "lease:granted",
  "lease:preempt-requested",
  "task:status-changed",
  "tool:run-started",
  "tool:run-completed",
  "approval:requested",
  "memory:updated",
  "audit:event-created",
];

export const chatStreamEventNames = [
  "chat:stream-start",
  "chat:stream-delta",
  "chat:stream-done",
] as const;

export async function getDashboardState(): Promise<DashboardState> {
  const state = await invoke<DashboardState>("get_dashboard_state");
  return normalizeSystemSettings(state);
}

export async function createAgentProfile(input: CreateAgentInput): Promise<AgentProfile> {
  return invoke("create_agent_profile", { input });
}

export async function generateAgentProfile(prompt: string): Promise<CreateAgentInput> {
  return invoke("generate_agent_profile", { prompt });
}

export async function updateAgentProfile(input: UpdateAgentInput): Promise<AgentProfile> {
  return invoke("update_agent_profile", { input });
}

export async function deleteAgentProfile(id: string) {
  return invoke("delete_agent_profile", { id });
}

export async function createWorkGroup(input: CreateWorkGroupInput): Promise<WorkGroup> {
  return invoke("create_work_group", { input });
}

export async function deleteWorkGroup(workGroupId: string): Promise<void> {
  return invoke("delete_work_group", { workGroupId });
}

export async function clearWorkGroupHistory(workGroupId: string): Promise<void> {
  return invoke("clear_work_group_history", { workGroupId });
}

export async function updateWorkGroup(input: UpdateWorkGroupInput): Promise<WorkGroup> {
  return invoke("update_work_group", { input });
}

export async function addAgentToWorkGroup(workGroupId: string, agentId: string): Promise<WorkGroup> {
  return invoke("add_agent_to_work_group", { workGroupId, agentId });
}

export async function removeAgentFromWorkGroup(workGroupId: string, agentId: string): Promise<WorkGroup> {
  return invoke("remove_agent_from_work_group", { workGroupId, agentId });
}

export async function sendHumanMessage(input: SendHumanMessageInput) {
  return invoke("send_human_message", { input });
}

export async function approveToolRun(toolRunId: string, approved: boolean) {
  return invoke("approve_tool_run", { toolRunId, approved });
}

export async function cancelTaskCard(taskCardId: string) {
  return invoke("cancel_task_card", { taskCardId });
}

export async function pauseLease(leaseId: string) {
  return invoke("pause_lease", { leaseId });
}

export async function resumeTaskCard(taskCardId: string) {
  return invoke("resume_task_card", { taskCardId });
}

export async function getAuditEvents(limit?: number) {
  return invoke("get_audit_events", { limit });
}

export async function getSettings(): Promise<SystemSettings> {
  const settings = await invoke<SystemSettings>("get_settings");
  return {
    ...settings,
    globalConfig: normalizeGlobalConfig(settings.globalConfig as RawGlobalConfig | undefined),
  };
}

export async function updateSettings(settings: SystemSettings): Promise<void> {
  const normalizedGlobalConfig = normalizeGlobalConfig(
    settings.globalConfig as RawGlobalConfig | undefined,
  );
  const interopSettings = {
    ...settings,
    globalConfig: buildInteropGlobalConfig(normalizedGlobalConfig),
  };
  return invoke("update_settings", { settings: interopSettings });
}

export async function testProviderConnection(config: AIProviderConfig): Promise<void> {
  return invoke("test_provider_connection", { config });
}

export async function refreshProviderModels(
  config: AIProviderConfig,
): Promise<AIProviderConfig> {
  return invoke("refresh_provider_models", { config });
}

export async function installSkillFromLocal(sourcePath: string): Promise<SkillPack[]> {
  return invoke("install_skill_from_local", { sourcePath });
}

export async function installSkillFromGithub(
  source: string,
  skillPath?: string,
): Promise<SkillPack[]> {
  return invoke("install_skill_from_github", { source, skillPath });
}

export async function updateInstalledSkill(
  skillId: string,
  name?: string,
  promptTemplate?: string,
): Promise<SkillPack> {
  return invoke("update_installed_skill", { skillId, name, promptTemplate });
}

export async function setInstalledSkillEnabled(
  skillId: string,
  enabled: boolean,
): Promise<SkillPack> {
  return invoke("set_installed_skill_enabled", { skillId, enabled });
}

export async function deleteInstalledSkill(skillId: string): Promise<void> {
  return invoke("delete_installed_skill", { skillId });
}

export async function getInstalledSkillDetail(skillId: string): Promise<SkillDetail> {
  return invoke("get_installed_skill_detail", { skillId });
}

export async function updateSkillDetail(input: UpdateSkillDetailInput): Promise<SkillDetail> {
  return invoke("update_skill_detail", { input });
}

export async function readInstalledSkillFile(
  skillId: string,
  relativePath: string,
): Promise<string> {
  return invoke("read_installed_skill_file", { skillId, relativePath });
}

export async function upsertInstalledSkillFile(
  skillId: string,
  relativePath: string,
  content: string,
): Promise<void> {
  return invoke("upsert_installed_skill_file", { skillId, relativePath, content });
}

export async function deleteInstalledSkillFile(
  skillId: string,
  relativePath: string,
): Promise<void> {
  return invoke("delete_installed_skill_file", { skillId, relativePath });
}

export async function pickDirectory(defaultPath?: string): Promise<string | null> {
  const parse = (selected: string | string[] | null): string | null => {
    if (!selected) {
      return null;
    }
    if (typeof selected === "string") {
      return selected;
    }
    if (Array.isArray(selected)) {
      const first = selected[0];
      return typeof first === "string" ? first : null;
    }
    return null;
  };

  const isAbsolutePath = (value: string) =>
    value.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(value);

  try {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: defaultPath && isAbsolutePath(defaultPath) ? defaultPath : undefined,
    });
    return parse(selected);
  } catch (error) {
    // Retry once without defaultPath; some platforms fail when defaultPath is invalid.
    console.error("pickDirectory failed with defaultPath, retrying without it", error);
    const selected = await open({
      directory: true,
      multiple: false,
    });
    return parse(selected);
  }
}

export async function getDocumentDirectory(): Promise<string | null> {
  try {
    return await documentDir();
  } catch {
    return null;
  }
}

export function subscribeToEvents(handlers: {
  onDashboardEvent: (eventName: string) => void;
  onChatStreamEvent: (eventName: (typeof chatStreamEventNames)[number], payload: ChatStreamEvent) => void;
}) {
  return Promise.all([
    ...dashboardEventNames.map((eventName) =>
      listen(eventName, () => handlers.onDashboardEvent(eventName)),
    ),
    ...chatStreamEventNames.map((eventName) =>
      listen<ChatStreamEvent>(eventName, (event) => {
        if (event.payload) {
          handlers.onChatStreamEvent(eventName, event.payload);
        }
      }),
    ),
  ]);
}
