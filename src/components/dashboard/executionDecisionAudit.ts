export interface DecisionAuditContext {
  workGroupId?: string;
  taskId?: string;
  taskIds?: string[];
  blockerId?: string;
  actorId?: string;
}

export interface DecisionAuditPayload {
  agentId?: string;
  provider?: string;
  model?: string;
  prompt?: string;
  raw?: string;
  error?: string;
  action?: string;
  taskId?: string;
  blockerId?: string;
  context?: DecisionAuditContext;
}

export function isDecisionAuditEventType(eventType: string) {
  return [
    "owner.decision.generated",
    "owner.decision.parse_failed",
    "owner.blocker_decision.applied",
    "owner.blocker_decision.failed",
    "agent.narrative.generated",
    "agent.narrative.parse_failed",
  ].includes(eventType);
}

export function parseDecisionAuditPayload(payloadJson: string): DecisionAuditPayload | null {
  try {
    const payload = JSON.parse(payloadJson) as DecisionAuditPayload;
    return payload && typeof payload === "object" ? payload : null;
  } catch {
    return null;
  }
}

export function decisionAuditStatus(eventType: string): "generated" | "failed" | "applied" {
  if (eventType.endsWith(".generated")) return "generated";
  if (eventType.endsWith(".applied")) return "applied";
  return "failed";
}

export function decisionAuditBadgeClass(status: "generated" | "failed" | "applied") {
  if (status === "generated") return "badge-success";
  if (status === "applied") return "badge-info";
  return "badge-error";
}

export function collectDecisionTaskIds(payload: DecisionAuditPayload, entityId: string) {
  const ids = new Set<string>();
  if (payload.taskId) ids.add(payload.taskId);
  if (payload.context?.taskId) ids.add(payload.context.taskId);
  for (const taskId of payload.context?.taskIds ?? []) ids.add(taskId);
  if (entityId) ids.add(entityId);
  return [...ids];
}
