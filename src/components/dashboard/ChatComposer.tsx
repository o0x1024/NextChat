import type {
  ChangeEvent,
  FormEvent,
  KeyboardEvent as ReactKeyboardEvent,
  RefObject,
} from "react";
import { useTranslation } from "react-i18next";
import type { AgentProfile } from "../../types";
import type { MentionDraft } from "./mentions";

interface ChatComposerProps {
  value: string;
  sendingMessage: boolean;
  mentionDraft: MentionDraft | null;
  mentionOptions: AgentProfile[];
  mentionIndex: number;
  mentionError: string | null;
  currentApprovalsCount: number;
  activeTasksCount: number;
  stoppableTasksCount: number;
  stoppingExecution: boolean;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onChangeValue: (value: string) => void;
  onSetMentionIndex: (index: number | ((current: number) => number)) => void;
  onApplyMention: (agent: AgentProfile) => void;
  onOpenMentionPicker: () => void;
  onJumpToApprovals: () => void;
  onJumpToTaskBoard: () => void;
  onStopExecution: () => void;
  onClearHistory: () => void;
}

export function ChatComposer({
  value,
  sendingMessage,
  mentionDraft,
  mentionOptions,
  mentionIndex,
  mentionError,
  currentApprovalsCount,
  activeTasksCount,
  stoppableTasksCount,
  stoppingExecution,
  textareaRef,
  onSubmit,
  onChangeValue,
  onSetMentionIndex,
  onApplyMention,
  onOpenMentionPicker,
  onJumpToApprovals,
  onJumpToTaskBoard,
  onStopExecution,
  onClearHistory,
}: ChatComposerProps) {
  const { t } = useTranslation();

  function handleKeyDown(event: ReactKeyboardEvent<HTMLTextAreaElement>) {
    if (mentionDraft && mentionOptions.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        onSetMentionIndex((current) => (current + 1) % mentionOptions.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        onSetMentionIndex((current) =>
          current === 0 ? mentionOptions.length - 1 : current - 1,
        );
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        onApplyMention(mentionOptions[mentionIndex] ?? mentionOptions[0]);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        onChangeValue(
          value.slice(0, mentionDraft.start) + value.slice(mentionDraft.start + 1),
        );
        return;
      }
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (value.trim() && !sendingMessage) {
        event.currentTarget.form?.requestSubmit();
      }
    }
  }

  return (
    <div className="mt-auto shrink-0 pb-1 pt-3">
      <form
        className="flex flex-col rounded-2xl border border-primary/20 bg-base-100 p-1 pt-2 shadow-sm transition-all focus-within:ring-2 focus-within:ring-primary/10"
        onSubmit={(event) => {
          if (value.trim() && !sendingMessage) {
            onSubmit(event);
          } else {
            event.preventDefault();
          }
        }}
      >
        <textarea
          ref={textareaRef as RefObject<HTMLTextAreaElement>}
          className="textarea textarea-ghost min-h-[0px] w-full resize-none bg-transparent text-sm leading-relaxed placeholder:opacity-30 focus:outline-none"
          placeholder={t("taskPlaceholder")}
          rows={2}
          value={value}
          disabled={sendingMessage}
          onChange={(event: ChangeEvent<HTMLTextAreaElement>) => onChangeValue(event.target.value)}
          onKeyDown={handleKeyDown}
        />
        {mentionDraft ? (
          <div className="px-2">
            <div className="rounded-xl border border-base-content/10 bg-base-200/80 p-2">
              <div className="mb-2 flex items-center justify-between gap-2 px-1 text-[10px] font-bold uppercase tracking-widest text-base-content/40">
                <span>{t("mentionAgent")}</span>
                <span>{t("mentionPickerHint")}</span>
              </div>
              <div className="space-y-1">
                {mentionOptions.map((agent, index) => (
                  <button
                    key={agent.id}
                    type="button"
                    className={`flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                      index === mentionIndex
                        ? "bg-primary text-primary-content"
                        : "hover:bg-base-300"
                    }`}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      onApplyMention(agent);
                    }}
                  >
                    <span className="flex items-center gap-2">
                      <span className="badge badge-ghost border-none bg-base-100/20 text-[10px]">
                        {agent.avatar}
                      </span>
                      <span>{agent.name}</span>
                    </span>
                    <span className="text-xs opacity-70">{agent.role}</span>
                  </button>
                ))}
                {mentionOptions.length === 0 ? (
                  <div className="rounded-lg px-3 py-2 text-sm text-base-content/60">
                    {t("mentionNoMatches")}
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        ) : null}
        {mentionError ? (
          <div className="px-2 pt-2">
            <div className="alert alert-warning py-2 text-sm">{mentionError}</div>
          </div>
        ) : null}

        <div className="mt-2 flex items-center justify-between gap-3 px-2 pb-1">
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              className="btn btn-circle btn-ghost h-6 min-h-0 w-6 px-0"
              title={t("mentionAgent")}
              aria-label={t("mentionAgent")}
              onClick={onOpenMentionPicker}
            >
              <i className="fas fa-at text-xs" />
            </button>
            <button
              type="button"
              className="btn btn-circle btn-ghost h-6 min-h-0 w-6 px-0"
              title={currentApprovalsCount > 0 ? t("openApprovalsQueue") : t("noPendingApprovals")}
              aria-label={currentApprovalsCount > 0 ? t("openApprovalsQueue") : t("noPendingApprovals")}
              onClick={onJumpToApprovals}
              disabled={currentApprovalsCount === 0}
            >
              <i className="fas fa-shield-halved text-xs" />
            </button>
            <button
              type="button"
              className="btn btn-circle btn-ghost h-6 min-h-0 w-6 px-0"
              title={activeTasksCount > 0 ? t("openTaskBoard") : t("noActiveTasksInGroup")}
              aria-label={activeTasksCount > 0 ? t("openTaskBoard") : t("noActiveTasksInGroup")}
              onClick={onJumpToTaskBoard}
              disabled={activeTasksCount === 0}
            >
              <i className="fas fa-list-check text-xs" />
            </button>
            <button
              type="button"
              className="btn btn-circle btn-ghost h-6 min-h-0 w-6 px-0 text-base-content/60 hover:text-error"
              title={stoppableTasksCount > 0 ? t("stopExecution") : t("noActiveTasksToStop")}
              aria-label={stoppableTasksCount > 0 ? t("stopExecution") : t("noActiveTasksToStop")}
              onClick={onStopExecution}
              disabled={stoppableTasksCount === 0 || stoppingExecution}
            >
              <i
                className={`${
                  stoppingExecution ? "fas fa-spinner fa-spin text-[10px]" : "fas fa-stop text-[10px]"
                }`}
              />
            </button>
            <button
              type="button"
              className="btn btn-circle btn-ghost h-6 min-h-0 w-6 px-0 text-base-content/60 hover:text-warning"
              title={t("clearHistory")}
              aria-label={t("clearHistory")}
              onClick={onClearHistory}
            >
              <i className="fas fa-eraser text-[10px]" />
            </button>
          </div>

          <div className="flex items-center">
            <button
              type="submit"
              className={`btn btn-circle h-7 min-h-0 w-7 btn-xs border-none transition-all ${
                value.trim()
                  ? "bg-primary text-primary-content shadow-lg shadow-primary/20 hover:scale-110"
                  : "bg-base-200 text-base-content/20"
              }`}
              disabled={!value.trim() || sendingMessage}
            >
              <i className="fas fa-arrow-up text-[10px]" />
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
