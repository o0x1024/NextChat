import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  AgentProfile,
  AuditEvent,
  ChatStreamTrack,
  ConversationMessage,
  TaskCard,
  ToolManifest,
  ToolRun,
} from "../../types";
import { ExecutionDecisionFilter, type DecisionFilter } from "./ExecutionDecisionFilter";
import { GroupedDecisionAuditList } from "./GroupedDecisionAuditList";
import {
  collectDecisionTaskIds,
  decisionAuditBadgeClass,
  decisionAuditStatus,
  isDecisionAuditEventType,
  parseDecisionAuditPayload,
} from "./executionDecisionAudit";
import { chainBadgeClass, narrativeChainToken } from "./chainVisual";
import { formatTime } from "./ui";

interface AgentExecutionDetailsPanelProps {
  language: Language;
  focusAgentId: string | null;
  onFocusAgentIdChange: (agentId: string | null) => void;
  onJumpToTask: (taskId: string) => void;
  onJumpToBlocker: (blockerId: string) => void;
  onJumpToNarrative: (target: { taskId?: string; blockerId?: string }) => void;
  currentMembers: AgentProfile[];
  agents: AgentProfile[];
  currentGroupTasks: TaskCard[];
  groupMessages: ConversationMessage[];
  streamTracks: ChatStreamTrack[];
  toolRuns: ToolRun[];
  auditEvents: AuditEvent[];
  tools: ToolManifest[];
}

interface ParsedToolCall {
  toolId: string;
  toolName: string;
  input: string;
  output: string;
  callId?: string;
}

type ExecutionEvent =
  | {
      id: string;
      type: "message";
      timestamp: string;
      taskCardId: string;
      senderId: string;
      senderName: string;
      kind: ConversationMessage["kind"];
      visibility: ConversationMessage["visibility"];
      content: string;
      executionMode?: ConversationMessage["executionMode"];
    }
  | {
      id: string;
      type: "stream";
      timestamp: string;
      taskCardId: string;
      senderId: string;
      senderName: string;
      kind: ChatStreamTrack["kind"];
      visibility: ChatStreamTrack["visibility"];
      content: string;
      status: ChatStreamTrack["status"];
    }
  | {
      id: string;
      type: "tool_call";
      timestamp: string;
      taskCardId: string;
      senderId: string;
      senderName: string;
      toolId: string;
      toolName: string;
      callId: string;
      input: string;
      output: string;
      state: ToolRun["state"];
      approvalRequired: boolean;
    }
  | {
      id: string;
      type: "tool_run";
      timestamp: string;
      taskCardId: string;
      agentId: string;
      agentName: string;
      toolId: string;
      toolName: string;
      state: ToolRun["state"];
      approvalRequired: boolean;
      resultRef?: string | null;
    }
  | {
      id: string;
      type: "decision_audit";
      timestamp: string;
      taskCardId?: string;
      blockerId?: string;
      senderId: string;
      senderName: string;
      eventType: string;
      status: "generated" | "failed" | "applied";
      provider?: string;
      model?: string;
      action?: string;
      prompt?: string;
      raw?: string;
      error?: string;
    };

interface ToolResultMatch {
  output: string;
  timestamp: string;
  messageId?: string;
}
function toolRunBadgeClass(state: ToolRun["state"]) {
  switch (state) {
    case "completed":
      return "badge-success";
    case "cancelled":
      return "badge-error";
    case "pending_approval":
      return "badge-warning";
    case "running":
      return "badge-info";
    default:
      return "badge-ghost";
  }
}

function streamBadgeClass(status: ChatStreamTrack["status"]) {
  return status === "streaming" ? "badge-info" : "badge-success";
}

function messageKindLabel(messageKind: ConversationMessage["kind"] | ChatStreamTrack["kind"]) {
  return messageKind.replace("_", " ");
}

function stringField(value: unknown) {
  if (typeof value === "string") {
    return value;
  }
  if (value === null || value === undefined) {
    return "";
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function prettifyPayload(raw: string) {
  const trimmed = raw.trim();
  if (!trimmed) {
    return raw;
  }
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return raw;
  }
}

const HUMAN_MESSAGE_PREFIXES = [
  "Human:",
  "Human directive:",
  "User:",
  "User directive:",
  "用户:",
  "用户指令:",
];
function stripHumanMessageLines(value: string) {
  const filtered = value
    .split("\n")
    .filter((line) => {
      const normalized = line.trimStart();
      return !HUMAN_MESSAGE_PREFIXES.some((prefix) => normalized.startsWith(prefix));
    })
    .join("\n")
    .replace(/\n{3,}/g, "\n\n");

  return filtered.trim();
}

function sanitizeExecutionPayloadValue(value: unknown): unknown {
  if (typeof value === "string") {
    return stripHumanMessageLines(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeExecutionPayloadValue(item));
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([key, entry]) => [
        key,
        sanitizeExecutionPayloadValue(entry),
      ]),
    );
  }
  return value;
}

function formatExecutionPayload(raw: string, hideHumanMessages = false) {
  const source = hideHumanMessages ? stripHumanMessageLines(raw) : raw;
  const trimmed = source.trim();
  if (!trimmed) {
    return source;
  }
  try {
    const parsed = JSON.parse(trimmed);
    const sanitized = hideHumanMessages ? sanitizeExecutionPayloadValue(parsed) : parsed;
    return JSON.stringify(sanitized, null, 2);
  } catch {
    return source;
  }
}

function toolResultKey(taskCardId: string, senderId: string, toolId: string, callId?: string) {
  if (callId) {
    return `${taskCardId}::${senderId}::${callId}`;
  }
  return `${taskCardId}::${senderId}::${toolId}`;
}

function toolResultFallbackKey(taskCardId: string, senderId: string) {
  return `${taskCardId}::${senderId}`;
}

function parseToolCallRecord(value: unknown): ParsedToolCall | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const record = value as Record<string, unknown>;
  const toolId = stringField(record.toolId ?? record.tool_id);
  if (!toolId) {
    return null;
  }
  const toolName = stringField(record.toolName ?? record.tool_name) || toolId;
  const callId = stringField(record.callId ?? record.call_id) || undefined;
  return {
    toolId,
    toolName,
    callId,
    input: stringField(record.input),
    output: stringField(record.output),
  };
}

function parseToolCallMessage(message: ConversationMessage): ParsedToolCall | null {
  if (message.kind !== "tool_call") {
    return null;
  }
  try {
    return parseToolCallRecord(JSON.parse(message.content));
  } catch {
    return null;
  }
}

function parseToolResultMessage(message: ConversationMessage): ParsedToolCall[] {
  if (message.kind !== "tool_result") {
    return [];
  }
  try {
    const payload = JSON.parse(message.content) as
      | {
          toolCalls?: Array<Record<string, unknown>>;
          tool_calls?: Array<Record<string, unknown>>;
        }
      | Record<string, unknown>
      | null;

    if (!payload || typeof payload !== "object") {
      return [];
    }
    const multiCallPayload = (payload as { toolCalls?: unknown; tool_calls?: unknown }).toolCalls ??
      (payload as { toolCalls?: unknown; tool_calls?: unknown }).tool_calls;
    if (Array.isArray(multiCallPayload)) {
      return multiCallPayload
        .map((call) => parseToolCallRecord(call))
        .filter((call): call is ParsedToolCall => call !== null);
    }
    const singleCall = parseToolCallRecord(payload);
    return singleCall ? [singleCall] : [];
  } catch {
    return [];
  }
}

function isAgentFocused(targetAgentId: string, focusAgentId: string | null) {
  if (!focusAgentId) {
    return true;
  }
  return targetAgentId === focusAgentId;
}

export function AgentExecutionDetailsPanel({
  language,
  focusAgentId,
  onFocusAgentIdChange,
  onJumpToTask,
  onJumpToBlocker,
  onJumpToNarrative,
  currentMembers,
  agents,
  currentGroupTasks,
  groupMessages,
  streamTracks,
  toolRuns,
  auditEvents,
  tools,
}: AgentExecutionDetailsPanelProps) {
  const { t } = useTranslation();
  const eventsContainerRef = useRef<HTMLDivElement | null>(null);
  const shouldAutoScrollRef = useRef(true);
  const [decisionFilter, setDecisionFilter] = useState<DecisionFilter>("all");
  const taskIds = useMemo(() => new Set(currentGroupTasks.map((task) => task.id)), [currentGroupTasks]);
  const taskTitleById = useMemo(
    () => new Map(currentGroupTasks.map((task) => [task.id, task.title])),
    [currentGroupTasks],
  );
  const taskCreatedAtById = useMemo(
    () => new Map(currentGroupTasks.map((task) => [task.id, task.createdAt])),
    [currentGroupTasks],
  );
  const currentWorkGroupId = currentGroupTasks[0]?.workGroupId ?? groupMessages[0]?.workGroupId ?? null;
  const memberById = useMemo(
    () => new Map(currentMembers.map((agent) => [agent.id, agent])),
    [currentMembers],
  );
  const agentById = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents]);
  const toolById = useMemo(() => new Map(tools.map((tool) => [tool.id, tool])), [tools]);

  const runStateByTaskTool = useMemo(() => {
    const timestamp = (run: ToolRun) => run.finishedAt ?? run.startedAt ?? "";
    const sortedRuns = [...toolRuns].sort(
      (left, right) => timestamp(right).localeCompare(timestamp(left)),
    );
    const stateMap = new Map<string, { state: ToolRun["state"]; approvalRequired: boolean }>();
    for (const run of sortedRuns) {
      const key = `${run.taskCardId}::${run.toolId}`;
      if (!stateMap.has(key)) {
        stateMap.set(key, {
          state: run.state,
          approvalRequired: run.approvalRequired,
        });
      }
    }
    return stateMap;
  }, [toolRuns]);

  const events = useMemo(() => {
    const hiddenMessageIds = new Set<string>();
    const toolCallEvents: ExecutionEvent[] = [];
    const consumedResultKeys = new Set<string>();
    const structuredToolCallMessageIds = new Set<string>();
    const structuredToolResultMessageIds = new Set<string>();
    const directToolCalls = new Map<string, ParsedToolCall>();
    const toolResultsByKey = new Map<string, ToolResultMatch>();
    const fallbackToolResults = new Map<string, ToolResultMatch>();

    for (const message of groupMessages) {
      const taskCardId = message.taskCardId;
      if (!taskCardId || !taskIds.has(taskCardId)) {
        continue;
      }
      if (!isAgentFocused(message.senderId, focusAgentId)) {
        continue;
      }

      if (message.kind === "tool_call") {
        const parsedCall = parseToolCallMessage(message);
        if (!parsedCall) {
          continue;
        }
        structuredToolCallMessageIds.add(message.id);
        directToolCalls.set(
          toolResultKey(taskCardId, message.senderId, parsedCall.toolId, parsedCall.callId),
          parsedCall,
        );
        directToolCalls.set(toolResultKey(taskCardId, message.senderId, parsedCall.toolId), parsedCall);
        continue;
      }

      if (message.kind !== "tool_result") {
        continue;
      }

      const parsedCalls = parseToolResultMessage(message);
      if (parsedCalls.length === 0) {
        fallbackToolResults.set(toolResultFallbackKey(taskCardId, message.senderId), {
          output: message.content,
          timestamp: message.createdAt,
          messageId: message.id,
        });
        continue;
      }

      structuredToolResultMessageIds.add(message.id);
      for (const call of parsedCalls) {
        toolResultsByKey.set(
          toolResultKey(taskCardId, message.senderId, call.toolId, call.callId),
          {
            output: call.output,
            timestamp: message.createdAt,
          },
        );
        toolResultsByKey.set(toolResultKey(taskCardId, message.senderId, call.toolId), {
          output: call.output,
          timestamp: message.createdAt,
        });
      }
    }

    for (const message of groupMessages) {
      const taskCardId = message.taskCardId;
      if (!taskCardId || !taskIds.has(taskCardId) || message.kind !== "tool_call") {
        continue;
      }
      if (!isAgentFocused(message.senderId, focusAgentId)) {
        continue;
      }
      const parsedCall = parseToolCallMessage(message);
      if (!parsedCall) {
        continue;
      }
      const primaryResultKey = toolResultKey(taskCardId, message.senderId, parsedCall.toolId, parsedCall.callId);
      const fallbackResultKey = toolResultKey(taskCardId, message.senderId, parsedCall.toolId);
      const matchedResult =
        toolResultsByKey.get(primaryResultKey) ??
        toolResultsByKey.get(fallbackResultKey) ??
        fallbackToolResults.get(toolResultFallbackKey(taskCardId, message.senderId));

      if (toolResultsByKey.has(primaryResultKey)) {
        consumedResultKeys.add(primaryResultKey);
      }
      if (toolResultsByKey.has(fallbackResultKey)) {
        consumedResultKeys.add(fallbackResultKey);
      }
      if (fallbackToolResults.has(toolResultFallbackKey(taskCardId, message.senderId))) {
        consumedResultKeys.add(toolResultFallbackKey(taskCardId, message.senderId));
      }
      if (matchedResult?.messageId) {
        hiddenMessageIds.add(matchedResult.messageId);
      }

      const runState = runStateByTaskTool.get(`${taskCardId}::${parsedCall.toolId}`);
      toolCallEvents.push({
        id: `tool-call-${message.id}`,
        type: "tool_call",
        timestamp: matchedResult?.timestamp ?? message.createdAt,
        taskCardId,
        senderId: message.senderId,
        senderName: message.senderName,
        toolId: parsedCall.toolId,
        toolName: parsedCall.toolName,
        callId: parsedCall.callId || `${parsedCall.toolId}:${message.id}`,
        input: parsedCall.input,
        output: matchedResult?.output ?? "",
        state: runState?.state ?? (matchedResult ? "completed" : "running"),
        approvalRequired: runState?.approvalRequired ?? false,
      });
    }

    for (const message of groupMessages) {
      const taskCardId = message.taskCardId;
      if (!taskCardId || !taskIds.has(taskCardId) || message.kind !== "tool_result") {
        continue;
      }
      if (!isAgentFocused(message.senderId, focusAgentId)) {
        continue;
      }
      const parsedCalls = parseToolResultMessage(message);
      if (parsedCalls.length === 0) {
        continue;
      }
      for (const [index, call] of parsedCalls.entries()) {
        const primaryResultKey = toolResultKey(taskCardId, message.senderId, call.toolId, call.callId);
        const fallbackResultKey = toolResultKey(taskCardId, message.senderId, call.toolId);
        const directCall =
          directToolCalls.get(primaryResultKey) ??
          directToolCalls.get(fallbackResultKey);

        if (directCall || consumedResultKeys.has(primaryResultKey) || consumedResultKeys.has(fallbackResultKey)) {
          continue;
        }

        const runState = runStateByTaskTool.get(`${taskCardId}::${call.toolId}`);
        toolCallEvents.push({
          id: `tool-result-${message.id}-${index}`,
          type: "tool_call",
          timestamp: message.createdAt,
          taskCardId,
          senderId: message.senderId,
          senderName: message.senderName,
          toolId: call.toolId,
          toolName: call.toolName,
          callId: call.callId || `${call.toolId}:${index}`,
          input: call.input,
          output: call.output,
          state: runState?.state ?? "completed",
          approvalRequired: runState?.approvalRequired ?? false,
        });
      }
    }

    for (const messageId of structuredToolCallMessageIds) {
      hiddenMessageIds.add(messageId);
    }
    for (const messageId of structuredToolResultMessageIds) {
      hiddenMessageIds.add(messageId);
    }

    const streamEvents: ExecutionEvent[] = streamTracks
      .filter((track) => {
        if (!track.taskCardId || !taskIds.has(track.taskCardId)) {
          return false;
        }
        return isAgentFocused(track.senderId, focusAgentId);
      })
      .map((track) => ({
        id: `stream-${track.streamId}`,
        type: "stream",
        timestamp: track.updatedAt,
        taskCardId: track.taskCardId as string,
        senderId: track.senderId,
        senderName: track.senderName,
        kind: track.kind,
        visibility: track.visibility,
        content: track.content,
        status: track.status,
      }));

    const messageEvents: ExecutionEvent[] = groupMessages
      .filter((message) => {
        if (!message.taskCardId || !taskIds.has(message.taskCardId)) {
          return false;
        }
        if (message.senderKind === "human") {
          return false;
        }
        if (!isAgentFocused(message.senderId, focusAgentId)) {
          return false;
        }
        return !hiddenMessageIds.has(message.id);
      })
      .map((message) => ({
        id: `msg-${message.id}`,
        type: "message",
        timestamp: message.createdAt,
        taskCardId: message.taskCardId as string,
        senderId: message.senderId,
        senderName: message.senderName,
        kind: message.kind,
        visibility: message.visibility,
        content: message.content,
        executionMode: message.executionMode,
      }));

    const toolRunEvents: ExecutionEvent[] = toolRuns
      .filter((run) => taskIds.has(run.taskCardId) && isAgentFocused(run.agentId, focusAgentId))
      .map((run) => ({
        id: `run-${run.id}-${run.state}`,
        type: "tool_run",
        timestamp:
          run.finishedAt ?? run.startedAt ?? taskCreatedAtById.get(run.taskCardId) ?? "",
        taskCardId: run.taskCardId,
        agentId: run.agentId,
        agentName: agentById.get(run.agentId)?.name ?? run.agentId,
        toolId: run.toolId,
        toolName: toolById.get(run.toolId)?.name ?? run.toolId,
        state: run.state,
        approvalRequired: run.approvalRequired,
        resultRef: run.resultRef,
      }));

    const decisionAuditEvents = auditEvents
      .filter((event) => isDecisionAuditEventType(event.eventType))
      .reduce<ExecutionEvent[]>((result, event) => {
        const payload = parseDecisionAuditPayload(event.payloadJson);
        if (!payload) {
          return result;
        }
        const relatedTaskIds = collectDecisionTaskIds(payload, event.entityId).filter((taskId) =>
          taskIds.has(taskId),
        );
        const sameGroup =
          payload.context?.workGroupId && currentWorkGroupId
            ? payload.context.workGroupId === currentWorkGroupId
            : false;
        if (relatedTaskIds.length === 0 && !sameGroup) {
          return result;
        }
        const senderId = payload.context?.actorId ?? payload.agentId ?? "system";
        if (focusAgentId && senderId !== focusAgentId) {
          return result;
        }
        result.push({
          id: `audit-${event.id}`,
          type: "decision_audit",
          timestamp: event.createdAt,
          taskCardId: relatedTaskIds[0],
          blockerId:
            payload.blockerId ??
            payload.context?.blockerId ??
            (event.eventType.startsWith("owner.blocker_decision.") ? event.entityId : undefined),
          senderId,
          senderName: agentById.get(senderId)?.name ?? memberById.get(senderId)?.name ?? senderId,
          eventType: event.eventType,
          status: decisionAuditStatus(event.eventType),
          provider: payload.provider,
          model: payload.model,
          action: payload.action,
          prompt: payload.prompt,
          raw: payload.raw,
          error: payload.error,
        });
        return result;
      }, []);

    return [...streamEvents, ...toolRunEvents, ...toolCallEvents, ...messageEvents, ...decisionAuditEvents]
      .filter((event) => event.timestamp)
      .sort((left, right) => left.timestamp.localeCompare(right.timestamp))
      .slice(-80);
  }, [
    auditEvents,
    agentById,
    currentWorkGroupId,
    focusAgentId,
    groupMessages,
    memberById,
    runStateByTaskTool,
    streamTracks,
    taskCreatedAtById,
    taskIds,
    toolById,
    toolRuns,
  ]);

  const visibleEvents = useMemo(() => {
    if (decisionFilter === "all") return events;
    return events.filter(
      (event) =>
        event.type === "decision_audit" &&
        (decisionFilter === "failed"
          ? event.status === "failed"
          : event.eventType.startsWith(`${decisionFilter}.`)),
    );
  }, [decisionFilter, events]);
  const groupedDecisionEvents = useMemo(() => visibleEvents.filter((event) => event.type === "decision_audit"), [visibleEvents]);
  const eventFingerprint = useMemo(
    () =>
      visibleEvents
        .map((event) =>
          event.type === "stream"
            ? `${event.id}:${event.timestamp}:${event.status}:${event.content.length}`
            : `${event.id}:${event.timestamp}`,
        )
        .join("|"),
    [visibleEvents],
  );

  useEffect(() => {
    const container = eventsContainerRef.current;
    if (!container || !shouldAutoScrollRef.current) {
      return;
    }
    container.scrollTop = container.scrollHeight;
  }, [eventFingerprint]);

  function handleEventsScroll() {
    const container = eventsContainerRef.current;
    if (!container) {
      return;
    }
    const distanceToBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight;
    shouldAutoScrollRef.current = distanceToBottom < 80;
  }

  return (
    <section className="card card-border bg-base-100">
      <div className="card-body gap-3">
        <div className="flex items-center justify-between gap-2">
          <h3 className="card-title text-base">{t("executionDetailsPanel")}</h3>
          <span className="badge badge-ghost">{t("itemsCount", { count: visibleEvents.length })}</span>
        </div>
        <p className="text-sm text-base-content/60">{t("executionDetailsHint")}</p>
        <ExecutionDecisionFilter language={language} value={decisionFilter} onChange={setDecisionFilter} />
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            className={`btn btn-xs ${focusAgentId ? "btn-ghost" : "btn-primary"}`}
            onClick={() => onFocusAgentIdChange(null)}
          >
            {t("focusAllAgents")}
          </button>
          {currentMembers.map((member) => (
            <button
              type="button"
              key={member.id}
              className={`btn btn-xs ${focusAgentId === member.id ? "btn-primary" : "btn-ghost"}`}
              onClick={() => onFocusAgentIdChange(member.id)}
            >
              {member.name}
            </button>
          ))}
        </div>

        <div
          ref={eventsContainerRef}
          className="max-h-[28rem] space-y-2 overflow-x-hidden overflow-y-auto pr-1"
          onScroll={handleEventsScroll}
        >
          {decisionFilter !== "all" ? (
            <GroupedDecisionAuditList events={groupedDecisionEvents} taskTitleById={taskTitleById} language={language} onJumpToTask={onJumpToTask} onJumpToBlocker={onJumpToBlocker} onJumpToNarrative={onJumpToNarrative} />
          ) : null}
          {decisionFilter === "all" &&
            visibleEvents.map((event) => {
            const taskTitle = event.taskCardId ? taskTitleById.get(event.taskCardId) : undefined;
            const chain = event.type === "decision_audit" ? narrativeChainToken({ taskId: event.taskCardId, blockerId: event.blockerId }) : null;

            if (event.type === "stream") {
              return (
                <article key={event.id} className="min-w-0 overflow-hidden rounded-box bg-base-200 px-3 py-2">
                  <div className="mb-1 flex items-center justify-between gap-2 text-xs">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="font-semibold">{event.senderName}</span>
                      <span className="badge badge-xs badge-info">
                        {messageKindLabel(event.kind)}
                      </span>
                      <span className={`badge badge-xs ${streamBadgeClass(event.status)}`}>
                        {event.status === "streaming"
                          ? t("streamingStatus")
                          : t("streamCompletedStatus")}
                      </span>
                    </div>
                    <time className="text-base-content/60">
                      {formatTime(event.timestamp, language)}
                    </time>
                  </div>
                  <div className="overflow-x-auto whitespace-pre-wrap break-words rounded-box bg-base-100/70 px-2 py-1 font-mono text-sm">
                    {event.content}
                    {event.status === "streaming" ? (
                      <span className="ml-1 inline-block h-4 w-1 animate-pulse bg-current align-middle" />
                    ) : null}
                  </div>
                  <div className="mt-2 flex flex-wrap gap-2 text-xs">
                    {taskTitle ? <span className="badge badge-ghost">{taskTitle}</span> : null}
                    <span className="badge badge-ghost badge-xs">{event.visibility}</span>
                  </div>
                </article>
              );
            }

            if (event.type === "tool_run") {
              return (
                <article key={event.id} className="min-w-0 overflow-hidden rounded-box bg-base-200 px-3 py-2">
                  <div className="mb-1 flex items-center justify-between gap-2 text-xs">
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="font-semibold">{event.agentName}</span>
                      <span className="badge badge-xs badge-info">{event.toolName}</span>
                      <span className={`badge badge-xs ${toolRunBadgeClass(event.state)}`}>
                        {t(`toolRunState.${event.state}`)}
                      </span>
                    </div>
                    <time className="text-base-content/60">
                      {formatTime(event.timestamp, language)}
                    </time>
                  </div>
                  <div className="flex flex-wrap gap-2 text-xs">
                    {taskTitle ? <span className="badge badge-ghost">{taskTitle}</span> : null}
                    {event.approvalRequired ? (
                      <span className="badge badge-warning badge-outline">{t("approval")}</span>
                    ) : null}
                    {event.resultRef ? (
                      <span className="badge badge-ghost">result: {event.resultRef}</span>
                    ) : null}
                  </div>
                </article>
              );
            }

            if (event.type === "tool_call") {
              const tool = toolById.get(event.toolId);
              const displayInput = formatExecutionPayload(event.input, true);
              const displayOutput = formatExecutionPayload(event.output, true);
              return (
                <details key={event.id} className="min-w-0 overflow-hidden rounded-box bg-base-200">
                  <summary className="flex list-none cursor-pointer items-center justify-between gap-3 px-3 py-2 [&::-webkit-details-marker]:hidden">
                    <div className="flex min-w-0 flex-1 items-center gap-2">
                      <span className="truncate text-sm font-semibold">
                        {tool?.name ?? event.toolName}
                      </span>
                      <span className="badge badge-xs badge-ghost">{event.senderName}</span>
                    </div>
                    <span className={`badge badge-xs shrink-0 ${toolRunBadgeClass(event.state)}`}>
                      {t(`toolRunState.${event.state}`)}
                    </span>
                  </summary>
                  <div className="border-t border-base-content/10 px-3 py-2 text-xs">
                    <div className="space-y-3">
                      <div>
                        <div className="mb-1 font-semibold text-base-content/70">
                          {t("executionInputParams")}
                        </div>
                        <pre className="max-h-48 w-full overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-100 px-2 py-1 font-mono text-xs">
                          {displayInput || "-"}
                        </pre>
                      </div>
                      <div>
                        <div className="mb-1 font-semibold text-base-content/70">
                          {t("executionResultLabel")}
                        </div>
                        <pre className="max-h-48 w-full overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-100 px-2 py-1 font-mono text-xs">
                          {displayOutput || "-"}
                        </pre>
                      </div>
                      <div className="flex items-center justify-between text-base-content/60">
                        <span>
                          {t("toolCallIdLabel")}: {event.callId}
                        </span>
                        <time>{formatTime(event.timestamp, language)}</time>
                      </div>
                      {event.approvalRequired ? (
                        <span className="badge badge-warning badge-outline">{t("approval")}</span>
                      ) : null}
                    </div>
                  </div>
                </details>
              );
            }

            if (event.type === "decision_audit") {
              return (
                <details key={event.id} className="min-w-0 overflow-hidden rounded-box bg-base-200">
                  <summary className="flex list-none cursor-pointer items-center justify-between gap-3 px-3 py-2 [&::-webkit-details-marker]:hidden">
                    <div className="flex min-w-0 flex-1 items-center gap-2">
                      <span className="truncate text-sm font-semibold">{event.senderName}</span>
                      <span className="badge badge-xs badge-ghost">{event.eventType}</span>
                      {chain ? <span className={`badge badge-xs ${chainBadgeClass(chain.key)}`}>{chain.label}</span> : null}
                      {taskTitle ? <span className="badge badge-xs badge-ghost">{taskTitle}</span> : null}
                    </div>
                    <span className={`badge badge-xs shrink-0 ${decisionAuditBadgeClass(event.status)}`}>
                      {t("status")}
                    </span>
                  </summary>
                  <div className="border-t border-base-content/10 px-3 py-2 text-xs">
                    <div className="mb-2 flex flex-wrap items-center gap-2">
                      <span className={`badge badge-xs ${decisionAuditBadgeClass(event.status)}`}>
                        {event.status}
                      </span>
                      {event.provider ? <span className="badge badge-xs badge-ghost">{t("provider")}: {event.provider}</span> : null}
                      {event.model ? <span className="badge badge-xs badge-ghost">{t("model")}: {event.model}</span> : null}
                      {event.action ? <span className="badge badge-xs badge-ghost">{t("actions")}: {event.action}</span> : null}
                    </div>
                    {event.prompt ? (
                      <div className="mb-3">
                        <div className="mb-1 font-semibold text-base-content/70">
                          {t("executionInputParams")}
                        </div>
                        <pre className="max-h-48 w-full overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-100 px-2 py-1 font-mono text-xs">
                          {event.prompt}
                        </pre>
                      </div>
                    ) : null}
                    {event.raw ? (
                      <div className="mb-3">
                        <div className="mb-1 font-semibold text-base-content/70">
                          {t("executionResultLabel")}
                        </div>
                        <pre className="max-h-48 w-full overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-100 px-2 py-1 font-mono text-xs">
                          {prettifyPayload(event.raw)}
                        </pre>
                      </div>
                    ) : null}
                    {event.error ? (
                      <div className="mb-3">
                        <div className="mb-1 font-semibold text-base-content/70">error</div>
                        <pre className="max-h-32 w-full overflow-auto whitespace-pre-wrap break-all rounded-box bg-base-100 px-2 py-1 font-mono text-xs">
                          {event.error}
                        </pre>
                      </div>
                    ) : null}
                    <div className="flex items-center justify-between gap-2 text-base-content/60">
                      <div className="flex flex-wrap gap-2">
                        {event.blockerId || event.taskCardId ? (
                          <button
                            type="button"
                            className="btn btn-ghost btn-xs"
                            onClick={() =>
                              onJumpToNarrative({ blockerId: event.blockerId, taskId: event.taskCardId })
                            }
                          >
                            {language === "zh" ? "打开叙事" : "Open narrative"}
                          </button>
                        ) : null}
                        {event.blockerId ? (
                          <button
                            type="button"
                            className="btn btn-ghost btn-xs"
                            onClick={() => onJumpToBlocker(event.blockerId as string)}
                          >
                            {language === "zh" ? "定位阻塞" : "Open blocker"}
                          </button>
                        ) : null}
                        {event.taskCardId ? (
                          <button
                            type="button"
                            className="btn btn-ghost btn-xs"
                            onClick={() => onJumpToTask(event.taskCardId as string)}
                          >
                            {t("openTask")}
                          </button>
                        ) : null}
                      </div>
                      <time>{formatTime(event.timestamp, language)}</time>
                    </div>
                  </div>
                </details>
              );
            }

            return (
              <article key={event.id} className="min-w-0 overflow-hidden rounded-box bg-base-200 px-3 py-2">
                <div className="mb-1 flex items-center justify-between gap-2 text-xs">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="font-semibold">{event.senderName}</span>
                    <span className="badge badge-xs badge-info">
                      {messageKindLabel(event.kind)}
                    </span>
                  </div>
                  <time className="text-base-content/60">
                    {formatTime(event.timestamp, language)}
                  </time>
                </div>
                <div
                  className={
                    event.kind === "tool_result"
                      ? "overflow-x-auto whitespace-pre-wrap break-all rounded-box bg-base-100/70 px-2 py-1 font-mono text-xs"
                      : "whitespace-pre-wrap break-words text-sm"
                  }
                >
                  {event.kind === "tool_result" ? prettifyPayload(event.content) : event.content}
                </div>
                <div className="mt-2 flex flex-wrap gap-2 text-xs">
                  {taskTitle ? <span className="badge badge-ghost">{taskTitle}</span> : null}
                  {event.executionMode ? (
                    <span
                      className={`badge badge-xs ${
                        event.executionMode === "real_model" ? "badge-success" : "badge-warning"
                      }`}
                    >
                      {event.executionMode === "real_model"
                        ? t("realModelReady")
                        : t("fallbackExecution")}
                    </span>
                  ) : null}
                </div>
              </article>
            );
          })}
          {visibleEvents.length === 0 ? (
            <div className="alert alert-soft">
              <span className="text-sm">{t("noExecutionDetails")}</span>
            </div>
          ) : null}
        </div>

        {focusAgentId && memberById.get(focusAgentId) ? (
          <div className="badge badge-primary badge-outline">
            {memberById.get(focusAgentId)?.name}
          </div>
        ) : null}
      </div>
    </section>
  );
}
