import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type { ChatStreamTrack, ConversationMessage } from "../../types";
import { formatTime } from "./ui";
import { MarkdownMessage } from "./MarkdownMessage";
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
  activeTaskIds: Set<string>;
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
  activeTaskIds,
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

  const visibleStreamTracks = useMemo(
    () =>
      streamTracks
        .filter((track) => track.visibility === "main" && !persistedMessageIds.has(track.streamId))
        .sort((left, right) => left.updatedAt.localeCompare(right.updatedAt)),
    [streamTracks, persistedMessageIds],
  );
  const streamingFingerprint = useMemo(
    () =>
      visibleStreamTracks
        .map((track) => `${track.streamId}:${track.updatedAt}:${track.content.length}`)
        .join("|"),
    [visibleStreamTracks],
  );
  const lastMessageId = currentMessages[currentMessages.length - 1]?.id ?? "";

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
      {currentMessages.map((message) => (
        <MessageRow
          key={message.id}
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
      ))}

      {visibleStreamTracks.map((track) => (
        <div key={`stream-${track.streamId}`} className="chat chat-start min-w-0">
          <div className="chat-header mb-1 text-xs text-base-content/50">
            <span className="font-medium">{track.senderName}</span>
            {track.taskCardId && currentTaskTitles.get(track.taskCardId) && (
              activeTaskIds.has(track.taskCardId) ? (
                <button
                  type="button"
                  className="ml-1 badge badge-ghost badge-xs transition-colors hover:border-primary hover:text-primary"
                  title={t("openTask")}
                  onClick={() => onJumpToTaskBoard(track.taskCardId ?? undefined)}
                >
                  {t("linkedTask")}{" "}
                  {compactTaskTitle(currentTaskTitles.get(track.taskCardId) ?? "")}
                </button>
              ) : (
                <span
                  className="ml-1 badge badge-ghost badge-xs"
                  title={currentTaskTitles.get(track.taskCardId) ?? undefined}
                >
                  {t("linkedTask")}{" "}
                  {compactTaskTitle(currentTaskTitles.get(track.taskCardId) ?? "")}
                </span>
              )
            )}
            <span
              className={`ml-1 badge badge-xs ${
                track.status === "streaming" ? "badge-info" : "badge-success"
              }`}
            >
              {track.status === "streaming" ? t("streamingStatus") : t("streamCompletedStatus")}
            </span>
            <time className="ml-2">{formatTime(track.updatedAt, language)}</time>
          </div>
          <div className="chat-bubble chat-bubble-secondary min-w-0 max-w-full text-sm">
            <div className="whitespace-pre-wrap break-words">
              {track.content}
              {track.status === "streaming" ? (
                <span className="ml-1 inline-block h-4 w-1 animate-pulse bg-current align-middle" />
              ) : null}
            </div>
          </div>
        </div>
      ))}

      {currentMessages.length === 0 && visibleStreamTracks.length === 0 && (
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
