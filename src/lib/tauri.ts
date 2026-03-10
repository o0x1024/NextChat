import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AIProviderConfig,
  AgentProfile,
  CreateAgentInput,
  CreateWorkGroupInput,
  DashboardState,
  SendHumanMessageInput,
  SkillPack,
  SystemSettings,
  UpdateAgentInput,
  UpdateWorkGroupInput,
  WorkGroup,
} from "../types";

export const eventNames = [
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

export async function getDashboardState(): Promise<DashboardState> {
  return invoke("get_dashboard_state");
}

export async function createAgentProfile(input: CreateAgentInput): Promise<AgentProfile> {
  return invoke("create_agent_profile", { input });
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
  return invoke("get_settings");
}

export async function updateSettings(settings: SystemSettings): Promise<void> {
  return invoke("update_settings", { settings });
}

export async function testProviderConnection(config: AIProviderConfig): Promise<void> {
  return invoke("test_provider_connection", { config });
}

export async function refreshProviderModels(
  config: AIProviderConfig,
): Promise<AIProviderConfig> {
  return invoke("refresh_provider_models", { config });
}

export async function installSkillFromLocal(sourcePath: string): Promise<SkillPack> {
  return invoke("install_skill_from_local", { sourcePath });
}

export async function installSkillFromGithub(
  source: string,
  skillPath?: string,
): Promise<SkillPack> {
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

export function subscribeToEvents(onEvent: () => void) {
  return Promise.all(eventNames.map((eventName) => listen(eventName, onEvent)));
}
