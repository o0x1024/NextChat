import type { ConversationMessage } from "../../types";

export interface NarrativeStageSummary {
  id: string;
  title: string;
  goal: string;
  executionMode: "serial" | "parallel";
  status: "pending" | "ready" | "running" | "blocked" | "completed" | "needs_review" | "cancelled";
  agents: string[];
}

export interface NarrativeEnvelope {
  marker: string;
  version: number;
  narrativeType:
    | "owner_ack"
    | "owner_plan"
    | "owner_dispatch"
    | "agent_ack"
    | "agent_progress"
    | "agent_delivery"
    | "owner_stage_transition"
    | "blocker_raised"
    | "blocker_resolved"
    | "owner_summary"
    | "direct_assign"
    | "direct_result";
  text: string;
  workflowId?: string | null;
  stageId?: string | null;
  taskId?: string | null;
  blockerId?: string | null;
  stageTitle?: string | null;
  taskTitle?: string | null;
  progressPercent?: number | null;
  blocked?: boolean | null;
  stages?: NarrativeStageSummary[] | null;
}

export function parseNarrativeContent(
  content: string,
  message?: ConversationMessage,
): NarrativeEnvelope | null {
  try {
    const parsed = JSON.parse(content) as NarrativeEnvelope | null;
    if (!parsed || parsed.marker !== "nextchat:narrative") {
      return parseLegacyNarrativeContent(content, message);
    }
    return parsed;
  } catch {
    return parseLegacyNarrativeContent(content, message);
  }
}

function parseLegacyNarrativeContent(
  content: string,
  message?: ConversationMessage,
): NarrativeEnvelope | null {
  const leasedMatch = content.match(
    /^Task card created and leased to (.+?)(?: using (.+?))?\.$/,
  );
  if (leasedMatch) {
    const [, agentName] = leasedMatch;
    return {
      marker: "nextchat:narrative",
      version: 1,
      narrativeType: "owner_dispatch",
      text: `@${agentName} 请先处理当前任务。`,
      taskId: message?.taskCardId ?? null,
      taskTitle: null,
    };
  }

  if (content === "Task card created, but no eligible agent claimed it.") {
    return {
      marker: "nextchat:narrative",
      version: 1,
      narrativeType: "blocker_raised",
      text: "当前还没有合适成员可以接这项任务，请调整目标或补充可执行成员。",
      taskId: message?.taskCardId ?? null,
      taskTitle: null,
      blocked: true,
    };
  }

  const approvalMatch = content.match(/^Approval required for (.+?) before execution\.$/);
  if (approvalMatch) {
    return {
      marker: "nextchat:narrative",
      version: 1,
      narrativeType: "blocker_raised",
      text: `当前任务需要审批后才能继续执行：${approvalMatch[1]}。`,
      taskId: message?.taskCardId ?? null,
      taskTitle: null,
      blocked: true,
    };
  }

  return null;
}

export function narrativeBadgeClass(type: NarrativeEnvelope["narrativeType"]) {
  switch (type) {
    case "owner_ack":
    case "owner_plan":
    case "owner_dispatch":
    case "owner_stage_transition":
    case "owner_summary":
      return "badge-primary";
    case "agent_ack":
    case "agent_progress":
    case "agent_delivery":
    case "direct_assign":
    case "direct_result":
      return "badge-success";
    case "blocker_raised":
      return "badge-warning";
    case "blocker_resolved":
      return "badge-info";
    default:
      return "badge-ghost";
  }
}

export function narrativeLabel(type: NarrativeEnvelope["narrativeType"]) {
  switch (type) {
    case "owner_ack":
      return "群主确认";
    case "owner_plan":
      return "阶段计划";
    case "owner_dispatch":
      return "群主派单";
    case "agent_ack":
      return "接单";
    case "agent_progress":
      return "进展";
    case "agent_delivery":
      return "交付";
    case "owner_stage_transition":
      return "阶段切换";
    case "blocker_raised":
      return "阻塞";
    case "blocker_resolved":
      return "已恢复";
    case "owner_summary":
      return "群主汇总";
    case "direct_assign":
      return "直派";
    case "direct_result":
      return "结果";
    default:
      return type;
  }
}

export function narrativeBubbleClass(envelope: NarrativeEnvelope, message: ConversationMessage) {
  if (envelope.narrativeType === "blocker_raised") {
    return "border border-warning/40 bg-warning/10 text-base-content";
  }
  if (envelope.narrativeType === "blocker_resolved") {
    return "border border-info/40 bg-info/10 text-base-content";
  }
  if (message.senderKind === "human") {
    return "chat-bubble-primary";
  }
  if (
    envelope.narrativeType === "owner_ack" ||
    envelope.narrativeType === "owner_plan" ||
    envelope.narrativeType === "owner_dispatch" ||
    envelope.narrativeType === "owner_stage_transition" ||
    envelope.narrativeType === "owner_summary"
  ) {
    return "border border-primary/20 bg-primary/10 text-base-content";
  }
  return "chat-bubble-secondary";
}
