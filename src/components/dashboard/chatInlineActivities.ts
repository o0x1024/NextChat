import type {
  ChatStreamTrack,
  ConversationMessage,
  ToolManifest,
  ToolRun,
} from "../../types";

interface ParsedToolCall {
  toolId: string;
  toolName: string;
  input: string;
  output: string;
  callId?: string;
}

export type ChatInlineActivity =
  | {
      id: string;
      type: "stream";
      timestamp: string;
      taskCardId?: string | null;
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
    };

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

function stripHumanMessageLines(value: string) {
  return value
    .split("\n")
    .filter((line) => {
      const normalized = line.trimStart();
      return ![
        "Human:",
        "Human directive:",
        "User:",
        "User directive:",
        "用户:",
        "用户指令:",
      ].some((prefix) => normalized.startsWith(prefix));
    })
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function formatInlineExecutionPayload(raw: string, hideHumanMessages = false) {
  const source = hideHumanMessages ? stripHumanMessageLines(raw) : raw;
  const trimmed = source.trim();
  if (!trimmed) {
    return source;
  }
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
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
    const multiCallPayload =
      (payload as { toolCalls?: unknown; tool_calls?: unknown }).toolCalls ??
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

function latestTimestamp(run: ToolRun) {
  return run.finishedAt ?? run.startedAt ?? "";
}

export function buildChatInlineActivities({
  currentMessages,
  streamTracks,
  toolRuns,
  tools,
  currentTaskIds,
}: {
  currentMessages: ConversationMessage[];
  streamTracks: ChatStreamTrack[];
  toolRuns: ToolRun[];
  tools: ToolManifest[];
  currentTaskIds: Set<string>;
}) {
  const hiddenMessageIds = new Set<string>();
  const persistedMessageIds = new Set(currentMessages.map((message) => message.id));
  const toolResultsByKey = new Map<string, { output: string; timestamp: string; messageId?: string }>();
  const fallbackToolResults = new Map<string, { output: string; timestamp: string; messageId?: string }>();
  const directToolCalls = new Map<string, ParsedToolCall>();
  const structuredToolCallMessageIds = new Set<string>();
  const structuredToolResultMessageIds = new Set<string>();
  const consumedResultKeys = new Set<string>();
  const toolById = new Map(tools.map((tool) => [tool.id, tool.name]));
  const latestRunByTaskTool = new Map<string, ToolRun>();
  const toolCallActivities: ChatInlineActivity[] = [];

  for (const run of [...toolRuns].sort((left, right) => latestTimestamp(right).localeCompare(latestTimestamp(left)))) {
    if (!currentTaskIds.has(run.taskCardId)) {
      continue;
    }
    const key = `${run.taskCardId}::${run.toolId}`;
    if (!latestRunByTaskTool.has(key)) {
      latestRunByTaskTool.set(key, run);
    }
  }

  for (const message of currentMessages) {
    const taskCardId = message.taskCardId;
    if (!taskCardId || !currentTaskIds.has(taskCardId)) {
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
          messageId: message.id,
        },
      );
      toolResultsByKey.set(toolResultKey(taskCardId, message.senderId, call.toolId), {
        output: call.output,
        timestamp: message.createdAt,
        messageId: message.id,
      });
    }
  }

  for (const message of currentMessages) {
    const taskCardId = message.taskCardId;
    if (!taskCardId || !currentTaskIds.has(taskCardId) || message.kind !== "tool_call") {
      continue;
    }
    const parsedCall = parseToolCallMessage(message);
    if (!parsedCall) {
      continue;
    }
    const primaryResultKey = toolResultKey(
      taskCardId,
      message.senderId,
      parsedCall.toolId,
      parsedCall.callId,
    );
    const fallbackResultKey = toolResultKey(taskCardId, message.senderId, parsedCall.toolId);
    const matchedResult =
      toolResultsByKey.get(primaryResultKey) ??
      toolResultsByKey.get(fallbackResultKey) ??
      fallbackToolResults.get(toolResultFallbackKey(taskCardId, message.senderId));
    const runState = latestRunByTaskTool.get(`${taskCardId}::${parsedCall.toolId}`);

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

    toolCallActivities.push({
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

  for (const message of currentMessages) {
    const taskCardId = message.taskCardId;
    if (!taskCardId || !currentTaskIds.has(taskCardId) || message.kind !== "tool_result") {
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
      if (
        directCall ||
        consumedResultKeys.has(primaryResultKey) ||
        consumedResultKeys.has(fallbackResultKey)
      ) {
        continue;
      }
      const runState = latestRunByTaskTool.get(`${taskCardId}::${call.toolId}`);
      toolCallActivities.push({
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

  const toolCallKeys = new Set(
    toolCallActivities
      .filter((activity): activity is Extract<ChatInlineActivity, { type: "tool_call" }> => activity.type === "tool_call")
      .map((activity) => `${activity.taskCardId}::${activity.toolId}`),
  );

  const streamActivities: ChatInlineActivity[] = streamTracks
    .filter((track) => track.visibility === "main" && !persistedMessageIds.has(track.streamId))
    .map((track) => ({
      id: `stream-${track.streamId}`,
      type: "stream",
      timestamp: track.updatedAt,
      taskCardId: track.taskCardId,
      senderId: track.senderId,
      senderName: track.senderName,
      kind: track.kind,
      visibility: track.visibility,
      content: track.content,
      status: track.status,
    }));

  const toolRunActivities: ChatInlineActivity[] = Array.from(latestRunByTaskTool.values())
    .filter((run) => run.state !== "completed" || !toolCallKeys.has(`${run.taskCardId}::${run.toolId}`))
    .map((run) => ({
      id: `tool-run-${run.id}-${run.state}`,
      type: "tool_run",
      timestamp: latestTimestamp(run),
      taskCardId: run.taskCardId,
      agentId: run.agentId,
      agentName: currentMessages.find((message) => message.senderId === run.agentId)?.senderName ?? run.agentId,
      toolId: run.toolId,
      toolName: toolById.get(run.toolId) ?? run.toolId,
      state: run.state,
      approvalRequired: run.approvalRequired,
    }));

  return {
    hiddenMessageIds,
    activities: [...streamActivities, ...toolCallActivities, ...toolRunActivities]
      .filter((activity) => activity.timestamp)
      .sort((left, right) => left.timestamp.localeCompare(right.timestamp)),
  };
}
