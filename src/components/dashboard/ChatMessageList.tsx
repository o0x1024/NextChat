import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  ChatStreamTrack,
  ConversationMessage,
  ToolManifest,
  ToolRun,
} from "../../types";
import { formatTime } from "./ui";
import { MarkdownMessage } from "./MarkdownMessage";
import {
  buildChatInlineActivities,
  formatInlineExecutionPayload,
  type ChatInlineActivity,
} from "./chatInlineActivities";
import {
  narrativeBadgeClass,
  narrativeBubbleClass,
  narrativeLabel,
  parseNarrativeContent,
} from "./narrative";
import { chainBadgeClass, narrativeChainToken } from "./chainVisual";

interface ChatMessageListProps {
  currentMessages: ConversationMessage[];
  streamTracks: ChatStreamTrack[];
  currentTaskTitles: Map<string, string>;
  currentTaskAssignees: Map<string, string>;
  currentMemberNames: Map<string, string>;
  currentTaskIds: Set<string>;
  activeTaskIds: Set<string>;
  toolRuns: ToolRun[];
  tools: ToolManifest[];
  language: Language;
  targetMessageId: string | null;
  onJumpToTaskBoard: (taskId?: string) => void;
  onJumpToBlocker: (blockerId: string) => void;
  onJumpToExecutionAgent: (agentId: string) => void;
}

function senderBubbleClass(message: ConversationMessage) {
  if (message.senderKind === "human") return "chat-bubble-primary";
  if (message.senderKind === "agent") return "chat-bubble-secondary";
  return "chat-bubble-neutral";
}

function executionBadgeClass(message: ConversationMessage) {
  if (message.executionMode === "real_model") return "badge-success";
  if (message.executionMode === "fallback") return "badge-warning";
  return "badge-ghost";
}

function compactTaskTitle(title: string) {
  return title.length > 28 ? `${title.slice(0, 28)}...` : title;
}

function shouldRenderMarkdown(message: ConversationMessage) {
  return message.kind !== "tool_call" && message.kind !== "tool_result";
}

export function ChatMessageList({
  currentMessages,
  streamTracks,
  currentTaskTitles,
  currentTaskAssignees,
  currentMemberNames,
  currentTaskIds,
  activeTaskIds,
  toolRuns,
  tools,
  language,
  targetMessageId,
  onJumpToTaskBoard,
  onJumpToBlocker,
  onJumpToExecutionAgent,
}: ChatMessageListProps) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const messageRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const shouldAutoScrollRef = useRef(true);
  const persistedMessageIds = useMemo(
    () => new Set(currentMessages.map((message) => message.id)),
    [currentMessages],
  );
  const { hiddenMessageIds, activities } = useMemo(
    () =>
      buildChatInlineActivities({
        currentMessages,
        streamTracks,
        toolRuns,
        tools,
        currentTaskIds,
      }),
    [currentMessages, currentTaskIds, streamTracks, toolRuns, tools],
  );
  const visibleMessages = useMemo(
    () => currentMessages.filter((message) => !hiddenMessageIds.has(message.id)),
    [currentMessages, hiddenMessageIds],
  );
  const streamingFingerprint = useMemo(
    () =>
      activities
        .map((activity) =>
          activity.type === "stream"
            ? `${activity.id}:${activity.timestamp}:${activity.status}:${activity.content.length}`
            : `${activity.id}:${activity.timestamp}`
        )
        .join("|"),
    [activities],
  );
  const lastMessageId = visibleMessages[visibleMessages.length - 1]?.id ?? "";

  useEffect(() => {
    const container = containerRef.current;
    if (!container || !shouldAutoScrollRef.current) {
      return;
    }
    container.scrollTop = container.scrollHeight;
  }, [lastMessageId, streamingFingerprint]);

  useEffect(() => {
    if (!targetMessageId) return;
    messageRefs.current[targetMessageId]?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [targetMessageId]);

  function handleScroll() {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const distanceToBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight;
    shouldAutoScrollRef.current = distanceToBottom < 80;
  }

  return (
    <div
      ref={containerRef}
      className="min-h-0 flex-1 space-y-1 overflow-x-hidden overflow-y-auto"
      onScroll={handleScroll}
    >
      {visibleMessages.length === 0 &&
        activities.map((activity) => (
          <InlineActivityRow
            key={activity.id}
            activity={activity}
            currentTaskTitles={currentTaskTitles}
            activeTaskIds={activeTaskIds}
            language={language}
            onJumpToTaskBoard={onJumpToTaskBoard}
            t={t}
          />
        ))}

      {visibleMessages.map((message, index) => {
        const nextTimestamp = visibleMessages[index + 1]?.createdAt;
        const inlineActivities = activities.filter(
          (activity) =>
            activity.timestamp >= message.createdAt &&
            (nextTimestamp ? activity.timestamp < nextTimestamp : true),
        );

        return (
          <div key={message.id} className="space-y-2">
            <MessageRow
              message={message}
              currentTaskTitles={currentTaskTitles}
              currentTaskAssignees={currentTaskAssignees}
              currentMemberNames={currentMemberNames}
              activeTaskIds={activeTaskIds}
              language={language}
              highlighted={targetMessageId === message.id}
              onJumpToTaskBoard={onJumpToTaskBoard}
              onJumpToBlocker={onJumpToBlocker}
              onJumpToExecutionAgent={onJumpToExecutionAgent}
              onSetMessageRef={(node) => {
                messageRefs.current[message.id] = node;
              }}
              t={t}
            />
            {inlineActivities.map((activity) => (
              <InlineActivityRow
                key={activity.id}
                activity={activity}
                currentTaskTitles={currentTaskTitles}
                activeTaskIds={activeTaskIds}
                language={language}
                onJumpToTaskBoard={onJumpToTaskBoard}
                t={t}
              />
            ))}
          </div>
        );
      })}

      {currentMessages.length === 0 && activities.length === 0 && (
        <div className="hero min-h-40 rounded-box bg-base-200">
          <div className="hero-content text-center">
            <div className="max-w-xs">
              <h3 className="text-base font-semibold">{t("noMessagesYet")}</h3>
              <p className="mt-1 text-sm text-base-content/60">{t("noMessagesHint")}</p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
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

function messageKindLabel(messageKind: ConversationMessage["kind"] | ChatStreamTrack["kind"]) {
  return messageKind.replace("_", " ");
}

function InlineActivityRow({
  activity,
  currentTaskTitles,
  activeTaskIds,
  language,
  onJumpToTaskBoard,
  t,
}: {
  activity: ChatInlineActivity;
  currentTaskTitles: Map<string, string>;
  activeTaskIds: Set<string>;
  language: Language;
  onJumpToTaskBoard: (taskId?: string) => void;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const taskTitle = activity.taskCardId ? currentTaskTitles.get(activity.taskCardId) : undefined;

  return (
    <div className="chat chat-start min-w-0">
      <div className="chat-bubble min-w-0 max-w-full border border-base-content/10 bg-base-200/70 text-sm text-base-content shadow-none">
        <div className="space-y-2">
          <div className="flex flex-wrap items-center gap-2 text-xs text-base-content/60">
            <span className="font-medium text-base-content/80">
              {activity.type === "tool_run" ? activity.agentName : activity.senderName}
            </span>
            {activity.type === "stream" ? (
              <>
                <span className="badge badge-xs badge-ghost">{messageKindLabel(activity.kind)}</span>
                <span className={`badge badge-xs ${activity.status === "streaming" ? "badge-info" : "badge-success"}`}>
                  {activity.status === "streaming" ? t("streamingStatus") : t("streamCompletedStatus")}
                </span>
              </>
            ) : (
              <>
                <span className="badge badge-xs badge-ghost">
                  {activity.toolName}
                </span>
                <span className={`badge badge-xs ${toolRunBadgeClass(activity.state)}`}>
                  {t(`toolRunState.${activity.state}`)}
                </span>
                {activity.approvalRequired ? (
                  <span className="badge badge-xs badge-warning">{t("approval")}</span>
                ) : null}
              </>
            )}
            {taskTitle ? (
              activeTaskIds.has(activity.taskCardId as string) ? (
                <button
                  type="button"
                  className="badge badge-ghost badge-xs transition-colors hover:border-primary hover:text-primary"
                  onClick={() => onJumpToTaskBoard(activity.taskCardId ?? undefined)}
                >
                  {t("linkedTask")} {compactTaskTitle(taskTitle)}
                </button>
              ) : (
                <span className="badge badge-ghost badge-xs">
                  {t("linkedTask")} {compactTaskTitle(taskTitle)}
                </span>
              )
            ) : null}
            <time>{formatTime(activity.timestamp, language)}</time>
          </div>

          {activity.type === "stream" ? (
            <div className="min-w-0 rounded-xl bg-base-100/80 px-3 py-2">
              <MarkdownMessage content={activity.content} />
              {activity.status === "streaming" ? (
                <span className="mt-1 inline-block h-4 w-1 animate-pulse bg-current align-middle" />
              ) : null}
            </div>
          ) : activity.type === "tool_call" ? (
            <details className="overflow-hidden rounded-xl bg-base-100/80" open={activity.state !== "completed"}>
              <summary className="cursor-pointer list-none px-3 py-2 text-xs font-medium text-base-content/70 [&::-webkit-details-marker]:hidden">
                {activity.state === "completed" ? activity.toolName : `${activity.toolName} · ${t("toolRunState.running")}`}
              </summary>
              <div className="space-y-2 border-t border-base-content/10 px-3 py-3">
                {activity.input.trim() ? (
                  <div className="space-y-1">
                    <div className="text-[11px] font-semibold uppercase tracking-wide text-base-content/45">
                      Input
                    </div>
                    <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded-lg bg-base-200 px-2 py-2 text-xs">
                      {formatInlineExecutionPayload(activity.input, true)}
                    </pre>
                  </div>
                ) : null}
                {activity.output.trim() ? (
                  <div className="space-y-1">
                    <div className="text-[11px] font-semibold uppercase tracking-wide text-base-content/45">
                      Output
                    </div>
                    <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded-lg bg-base-200 px-2 py-2 text-xs">
                      {formatInlineExecutionPayload(activity.output, true)}
                    </pre>
                  </div>
                ) : (
                  <div className="text-xs text-base-content/55">
                    {activity.state === "pending_approval"
                      ? t("toolRunNeedsApproval", { id: activity.callId })
                      : t(`toolRunState.${activity.state}`)}
                  </div>
                )}
              </div>
            </details>
          ) : (
            <div className="rounded-xl bg-base-100/80 px-3 py-2 text-xs text-base-content/70">
              {activity.state === "pending_approval"
                ? t("toolRunNeedsApproval", { id: activity.toolId })
                : t(`toolRunState.${activity.state}`)}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

interface MessageRowProps {
  message: ConversationMessage;
  currentTaskTitles: Map<string, string>;
  currentTaskAssignees: Map<string, string>;
  currentMemberNames: Map<string, string>;
  activeTaskIds: Set<string>;
  language: Language;
  highlighted: boolean;
  onJumpToTaskBoard: (taskId?: string) => void;
  onJumpToBlocker: (blockerId: string) => void;
  onJumpToExecutionAgent: (agentId: string) => void;
  onSetMessageRef: (node: HTMLDivElement | null) => void;
  t: ReturnType<typeof useTranslation>["t"];
}

function MessageRow({
  message,
  currentTaskTitles,
  currentTaskAssignees,
  currentMemberNames,
  activeTaskIds,
  language,
  highlighted,
  onJumpToTaskBoard,
  onJumpToBlocker,
  onJumpToExecutionAgent,
  onSetMessageRef,
  t,
}: MessageRowProps) {
  const narrative = parseNarrativeContent(message.content, message);
  const chain = narrative ? narrativeChainToken(narrative) : null;

  return (
    <div
      ref={onSetMessageRef}
      className={`chat min-w-0 transition-colors ${
        message.senderKind === "human" ? "chat-end" : "chat-start"
      } ${highlighted ? "rounded-box bg-primary/5" : ""}`}
    >
      <div className="chat-header mb-1 text-xs text-base-content/50">
        <span className="font-medium">{message.senderName}</span>
        {message.kind === "collaboration" && (
          <span className="ml-1 badge badge-info badge-xs">{t("collaboration")}</span>
        )}
        {narrative ? (
          <span className={`ml-1 badge badge-xs ${narrativeBadgeClass(narrative.narrativeType)}`}>
            {narrativeLabel(narrative.narrativeType)}
          </span>
        ) : null}
        {chain ? <span className={`ml-1 badge badge-xs ${chainBadgeClass(chain.key)}`}>{chain.label}</span> : null}
        {message.taskCardId && currentTaskTitles.get(message.taskCardId) && (
          activeTaskIds.has(message.taskCardId) ? (
            <button
              type="button"
              className="ml-1 badge badge-ghost badge-xs transition-colors hover:border-primary hover:text-primary"
              title={t("openTask")}
              onClick={() => onJumpToTaskBoard(message.taskCardId ?? undefined)}
            >
              {t("linkedTask")} {compactTaskTitle(currentTaskTitles.get(message.taskCardId) ?? "")}
            </button>
          ) : (
            <span
              className="ml-1 badge badge-ghost badge-xs"
              title={currentTaskTitles.get(message.taskCardId) ?? undefined}
            >
              {t("linkedTask")} {compactTaskTitle(currentTaskTitles.get(message.taskCardId) ?? "")}
            </span>
          )
        )}
        {message.executionMode && (
          <span className={`ml-1 badge badge-xs ${executionBadgeClass(message)}`}>
            {message.executionMode === "real_model" ? t("realModelReady") : t("fallbackExecution")}
          </span>
        )}
        <time className="ml-2">{formatTime(message.createdAt, language)}</time>
      </div>
      <div
        className={`chat-bubble min-w-0 max-w-full text-sm ${
          narrative ? narrativeBubbleClass(narrative, message) : senderBubbleClass(message)
        }`}
      >
        {narrative ? (
          <NarrativeContent
            envelope={narrative}
            language={language}
            activeTaskIds={activeTaskIds}
            currentTaskAssignees={currentTaskAssignees}
            currentMemberNames={currentMemberNames}
            onJumpToTaskBoard={onJumpToTaskBoard}
            onJumpToBlocker={onJumpToBlocker}
            onJumpToExecutionAgent={onJumpToExecutionAgent}
            t={t}
          />
        ) : shouldRenderMarkdown(message) ? (
          <MarkdownMessage content={message.content} />
        ) : (
          <div className="whitespace-pre-wrap break-words">{message.content}</div>
        )}
      </div>
    </div>
  );
}

function NarrativeContent({
  envelope,
  language,
  activeTaskIds,
  currentTaskAssignees,
  currentMemberNames,
  onJumpToTaskBoard,
  onJumpToBlocker,
  onJumpToExecutionAgent,
  t,
}: {
  envelope: NonNullable<ReturnType<typeof parseNarrativeContent>>;
  language: Language;
  activeTaskIds: Set<string>;
  currentTaskAssignees: Map<string, string>;
  currentMemberNames: Map<string, string>;
  onJumpToTaskBoard: (taskId?: string) => void;
  onJumpToBlocker: (blockerId: string) => void;
  onJumpToExecutionAgent: (agentId: string) => void;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  const assignedAgentId = envelope.taskId ? currentTaskAssignees.get(envelope.taskId) : undefined;
  const assignedAgentName = assignedAgentId ? currentMemberNames.get(assignedAgentId) : undefined;
  return (
    <div className="space-y-3">
      <div className="whitespace-pre-wrap break-words">{envelope.text}</div>
      {envelope.blockerId || (envelope.taskId && activeTaskIds.has(envelope.taskId)) || assignedAgentId ? (
        <div className="flex flex-wrap gap-2">
          {envelope.blockerId ? (
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              onClick={() => onJumpToBlocker(envelope.blockerId as string)}
            >
              {t("openBlocker")}
            </button>
          ) : null}
          {envelope.taskId && activeTaskIds.has(envelope.taskId) ? (
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              onClick={() => onJumpToTaskBoard(envelope.taskId ?? undefined)}
            >
              {t("openTask")}
            </button>
          ) : null}
          {assignedAgentId ? (
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              title={assignedAgentName ?? assignedAgentId}
              onClick={() => onJumpToExecutionAgent(assignedAgentId)}
            >
              {language === "zh" ? `查看成员${assignedAgentName ? ` ${assignedAgentName}` : ""}` : `Open member${assignedAgentName ? ` ${assignedAgentName}` : ""}`}
            </button>
          ) : null}
        </div>
      ) : null}
      {(envelope.stageTitle || envelope.taskTitle || typeof envelope.progressPercent === "number") && (
        <div className="flex flex-wrap gap-2">
          {envelope.stageTitle ? <span className="badge badge-outline">{envelope.stageTitle}</span> : null}
          {envelope.taskTitle ? <span className="badge badge-outline">{envelope.taskTitle}</span> : null}
          {typeof envelope.progressPercent === "number" ? (
            <span className="badge badge-outline">{envelope.progressPercent}%</span>
          ) : null}
        </div>
      )}
      {envelope.stages && envelope.stages.length > 0 ? (
        <div className="grid gap-2">
          {envelope.stages.map((stage) => (
            <div key={stage.id} className="rounded-xl border border-base-content/10 bg-base-100/70 p-3">
              <div className="flex flex-wrap items-center gap-2">
                <strong>{stage.title}</strong>
                <span className="badge badge-ghost badge-xs">{stage.executionMode === "parallel" ? "并行" : "串行"}</span>
                <span className="badge badge-ghost badge-xs">{stage.status}</span>
              </div>
              <div className="mt-1 text-xs opacity-75">{stage.goal}</div>
              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  className="btn btn-ghost btn-xs"
                  onClick={() => onJumpToTaskBoard()}
                >
                  {t("openTaskBoard")}
                </button>
                {stage.agents.map((agentId) => (
                  <button
                    key={`${stage.id}-${agentId}`}
                    type="button"
                    className="badge badge-ghost cursor-pointer transition-colors hover:border-primary hover:text-primary"
                    onClick={() => onJumpToExecutionAgent(agentId)}
                    title={currentMemberNames.get(agentId) ?? agentId}
                  >
                    {currentMemberNames.get(agentId) ?? agentId}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
