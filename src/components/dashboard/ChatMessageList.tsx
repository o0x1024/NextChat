import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type { ChatStreamTrack, ConversationMessage } from "../../types";
import { formatTime } from "./ui";
import { MarkdownMessage } from "./MarkdownMessage";

interface ChatMessageListProps {
  currentMessages: ConversationMessage[];
  streamTracks: ChatStreamTrack[];
  currentTaskTitles: Map<string, string>;
  activeTaskIds: Set<string>;
  language: Language;
  onJumpToTaskBoard: (taskId?: string) => void;
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
  activeTaskIds,
  language,
  onJumpToTaskBoard,
}: ChatMessageListProps) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement | null>(null);
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
      className="min-h-0 flex-1 space-y-1 overflow-y-auto"
      onScroll={handleScroll}
    >
      {currentMessages.map((message) => (
        <div
          key={message.id}
          className={`chat ${message.senderKind === "human" ? "chat-end" : "chat-start"}`}
        >
          <div className="chat-header mb-1 text-xs text-base-content/50">
            <span className="font-medium">{message.senderName}</span>
            {message.kind === "collaboration" && (
              <span className="ml-1 badge badge-info badge-xs">{t("collaboration")}</span>
            )}
            {message.taskCardId && currentTaskTitles.get(message.taskCardId) && (
              activeTaskIds.has(message.taskCardId) ? (
                <button
                  type="button"
                  className="ml-1 badge badge-ghost badge-xs transition-colors hover:border-primary hover:text-primary"
                  title={t("openTask")}
                  onClick={() => onJumpToTaskBoard(message.taskCardId ?? undefined)}
                >
                  {t("linkedTask")}{" "}
                  {compactTaskTitle(currentTaskTitles.get(message.taskCardId) ?? "")}
                </button>
              ) : (
                <span
                  className="ml-1 badge badge-ghost badge-xs"
                  title={currentTaskTitles.get(message.taskCardId) ?? undefined}
                >
                  {t("linkedTask")}{" "}
                  {compactTaskTitle(currentTaskTitles.get(message.taskCardId) ?? "")}
                </span>
              )
            )}
            {message.executionMode && (
              <span className={`ml-1 badge badge-xs ${executionBadgeClass(message)}`}>
                {message.executionMode === "real_model"
                  ? t("realModelReady")
                  : t("fallbackExecution")}
              </span>
            )}
            <time className="ml-2">{formatTime(message.createdAt, language)}</time>
          </div>
          <div className={`chat-bubble text-sm ${senderBubbleClass(message)}`}>
            {shouldRenderMarkdown(message) ? (
              <MarkdownMessage content={message.content} />
            ) : (
              <div className="whitespace-pre-wrap break-words">{message.content}</div>
            )}
          </div>
        </div>
      ))}

      {visibleStreamTracks.map((track) => (
        <div key={`stream-${track.streamId}`} className="chat chat-start">
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
          <div className="chat-bubble chat-bubble-secondary text-sm">
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
