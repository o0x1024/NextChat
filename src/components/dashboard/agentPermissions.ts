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
  | "blocked_by_permission"
  | "blocked_by_skill";

export function selectedSkillsForAgent(
  agent: AgentProfile,
  skills: SkillPack[],
): SkillPack[] {
  return skills.filter((skill) => agent.skillIds.includes(skill.id));
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

export function allowedSkillCategories(
  skills: SkillPack[],
): Set<string> | null {
  const categories = new Set(
    skills.flatMap((skill) => skill.allowedToolTags.map((tag) => tag.trim())).filter(Boolean),
  );
  return categories.size > 0 ? categories : null;
}

export function toolExposureReason(
  agent: AgentProfile,
  tool: ToolManifest,
  skills: SkillPack[],
): ToolExposureReason {
  if (!agent.toolIds.includes(tool.id)) {
    return "not_bound";
  }

  if (!allowsToolId(agent.permissionPolicy, tool.id)) {
    return "blocked_by_permission";
  }

  const categories = allowedSkillCategories(skills);
  if (categories && !categories.has(tool.category)) {
    return "blocked_by_skill";
  }

  return "available";
}
