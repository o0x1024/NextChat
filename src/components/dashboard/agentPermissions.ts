import type {
  AgentPermissionPolicy,
  AgentProfile,
  MemoryPolicy,
  SkillPack,
  ToolManifest,
} from "../../types";

export const emptyPermissionPolicy: AgentPermissionPolicy = {
  allowToolIds: [],
  denyToolIds: [],
  requireApprovalToolIds: [],
  allowFsRoots: [],
  allowNetworkDomains: [],
};

export const emptyMemoryPolicy: MemoryPolicy = {
  readScope: ["user", "work_group", "agent"],
  writeScope: ["work_group", "agent"],
  pinnedMemoryIds: [],
};

export function splitPolicyList(input: string): string[] {
  return input
    .split(/[\n,]/)
    .map((value) => value.trim())
    .filter(Boolean);
}

export function joinPolicyList(values: string[]): string {
  return values.join(", ");
}

export type ToolExposureReason =
  | "available"
  | "not_bound"
  | "blocked_by_permission";

export function selectedSkillsForAgent(
  agent: AgentProfile,
  skills: SkillPack[],
): SkillPack[] {
  if (!agent.toolIds.includes("Skills")) {
    return [];
  }

  return skills.filter((skill) => skill.enabled);
}

export function allowsToolId(
  policy: AgentPermissionPolicy,
  toolId: string,
): boolean {
  return (
    (policy.allowToolIds.length === 0 || policy.allowToolIds.includes(toolId)) &&
    !policy.denyToolIds.includes(toolId)
  );
}

export function toolExposureReason(
  agent: AgentProfile,
  tool: ToolManifest,
  _skills: SkillPack[],
): ToolExposureReason {
  if (!agent.toolIds.includes(tool.id)) {
    return "not_bound";
  }

  if (!allowsToolId(agent.permissionPolicy, tool.id)) {
    return "blocked_by_permission";
  }

  return "available";
}
