import { useTranslation } from "react-i18next";
import type { PendingUserQuestion } from "../../types";

interface PendingUserQuestionPanelProps {
  question: PendingUserQuestion;
  agentName?: string | null;
  sendingMessage: boolean;
  onSelectOption: (option: string) => void;
}

export function PendingUserQuestionPanel({
  question,
  agentName,
  sendingMessage,
  onSelectOption,
}: PendingUserQuestionPanelProps) {
  const { t } = useTranslation();

  return (
    <div className="mx-2 mb-2 rounded-2xl border border-info/30 bg-info/8 p-3 text-sm shadow-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="badge badge-info badge-sm">{t("taskStatus.waiting_user_input")}</span>
        {agentName ? <span className="text-xs text-base-content/60">{agentName}</span> : null}
      </div>
      <div className="mt-2 whitespace-pre-wrap break-words font-medium">{question.question}</div>
      {question.context ? (
        <div className="mt-2 whitespace-pre-wrap break-words text-xs text-base-content/70">
          {question.context}
        </div>
      ) : null}
      {question.options.length > 0 ? (
        <div className="mt-3 flex flex-wrap gap-2">
          {question.options.map((option) => (
            <button
              key={option}
              type="button"
              className="btn btn-outline btn-sm rounded-full"
              disabled={sendingMessage}
              onClick={() => onSelectOption(option)}
            >
              {option}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
