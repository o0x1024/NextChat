import type { ChangeEvent, FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type { AgentProfile, ConversationMessage, WorkGroup } from "../../types";
import { formatTime } from "./ui";

interface ChatWorkspaceProps {
  currentWorkGroup?: WorkGroup;
  currentMembers: AgentProfile[];
  currentMessages: ConversationMessage[];
  language: Language;
  backstageOpen: boolean;
  composerValue: string;
  loading: boolean;
  onComposerChange: (value: string) => void;
  onSendMessage: (event: FormEvent<HTMLFormElement>) => void;
  onSelectAgent: (id: string) => void;
  onSelectTask: (id?: string) => void;
  onInsertMention: (agent: AgentProfile) => void;
  onToggleBackstage: () => void;
}

function senderBadgeClass(message: ConversationMessage) {
  if (message.senderKind === "human") return "badge-info";
  if (message.senderKind === "agent") return "badge-success";
  return "badge-warning";
}

function bubbleClass(message: ConversationMessage) {
  if (message.senderKind === "human") return "chat-bubble-primary";
  if (message.senderKind === "agent") return "chat-bubble-secondary";
  return "chat-bubble-neutral";
}

export function ChatWorkspace({
  currentWorkGroup,
  currentMembers,
  currentMessages,
  language,
  backstageOpen,
  composerValue,
  loading,
  onComposerChange,
  onSendMessage,
  onSelectAgent,
  onSelectTask,
  onInsertMention,
  onToggleBackstage,
}: ChatWorkspaceProps) {
  const { t } = useTranslation();
  const summaryMessages = currentMessages.filter((message) => message.visibility === "main");
  const backstageMessages = currentMessages.filter(
    (message) => message.visibility === "backstage",
  );

  return (
    <section className="card card-border flex min-h-0 flex-1 bg-base-100">
      <div className="card-body min-h-0 p-0">
        <div className="rounded-t-box bg-base-200 px-4 py-4">
          <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
            <div>
              <div className="text-lg font-semibold">
                {currentWorkGroup?.name ?? t("selectWorkGroup")}
              </div>
              <div className="mt-1 text-sm text-base-content/60">
                {currentWorkGroup?.goal ?? t("noWorkGroupSelected")}
              </div>
            </div>

            <div className="flex flex-wrap gap-2">
              <span className="badge badge-neutral">
                {currentWorkGroup?.kind === "persistent" ? t("persistent") : t("ephemeral")}
              </span>
              <span className="badge badge-primary">
                {t("itemsCount", { count: summaryMessages.length })}
              </span>
              <button
                className={`btn btn-sm ${backstageOpen ? "btn-primary" : "btn-soft"}`}
                onClick={onToggleBackstage}
              >
                {backstageOpen ? t("hideBackstage") : t("toggleBackstage")}
              </button>
            </div>
          </div>
        </div>

        <div className="border-t border-base-300 px-4 py-4">
          <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
            <div>
              <div className="text-sm font-medium">{t("activeSession")}</div>
              <div className="text-sm text-base-content/60">{t("summaryFirstHint")}</div>
            </div>

            <div className="flex flex-wrap gap-2">
              {currentMembers.map((agent) => (
                <div key={agent.id} className="join">
                  <button
                    className="btn btn-sm btn-soft join-item"
                    onClick={() => onInsertMention(agent)}
                  >
                    <span className="avatar placeholder">
                      <span className="w-7 rounded-full bg-primary/10 text-xs text-primary">
                        {agent.avatar}
                      </span>
                    </span>
                    @{agent.name}
                  </button>
                  <button
                    className="btn btn-sm btn-ghost join-item"
                    onClick={() => onSelectAgent(agent.id)}
                  >
                    {t("agentDirectory")}
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div
          className={`grid min-h-0 flex-1 gap-4 px-4 py-4 ${backstageOpen ? "xl:grid-cols-[minmax(0,1fr)_320px]" : ""
            }`}
        >
          <div className="flex min-h-0 flex-col">
            <div className="navbar rounded-box bg-base-200 px-4 py-3">
              <div className="navbar-start">
                <h2 className="text-sm font-medium">{t("summaryFeed")}</h2>
              </div>
              <div className="navbar-end">
                <span className="badge badge-primary">
                  {t("itemsCount", { count: summaryMessages.length })}
                </span>
              </div>
            </div>

            <div className="mt-4 flex min-h-0 flex-1 flex-col gap-4 overflow-auto pr-1">
              {summaryMessages.map((message) => (
                <div
                  key={message.id}
                  className={`chat ${message.senderKind === "human" ? "chat-end" : "chat-start"}`}
                >
                  <div className="chat-header mb-2 flex flex-wrap items-center gap-2 text-xs text-base-content/60">
                    <span className={`badge ${senderBadgeClass(message)}`}>
                      {message.senderName}
                    </span>
                    <span className="badge badge-neutral">{message.kind}</span>
                    <time>{formatTime(message.createdAt, language)}</time>
                  </div>
                  <div className={`chat-bubble whitespace-pre-wrap ${bubbleClass(message)}`}>
                    {message.content}
                  </div>
                  {message.taskCardId ? (
                    <button
                      className="btn btn-link btn-sm mt-1 px-0"
                      onClick={() => onSelectTask(message.taskCardId ?? undefined)}
                    >
                      {t("openTask")}
                    </button>
                  ) : null}
                </div>
              ))}

              {summaryMessages.length === 0 ? (
                <div className="hero min-h-56 rounded-box bg-base-200">
                  <div className="hero-content text-center">
                    <div className="max-w-md">
                      <h3 className="text-lg font-semibold">{t("noMessagesYet")}</h3>
                      <p className="mt-2 text-sm text-base-content/60">
                        {t("noMessagesHint")}
                      </p>
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
          </div>

          {backstageOpen ? (
            <aside className="card card-border flex min-h-0 flex-col bg-base-100">
              <div className="rounded-t-box bg-base-200 px-4 py-3">
                <div className="flex items-center justify-between gap-3">
                  <h3 className="text-sm font-medium">{t("backstagePanel")}</h3>
                  <span className="badge badge-primary">
                    {t("itemsCount", { count: backstageMessages.length })}
                  </span>
                </div>
                <p className="mt-1 text-sm text-base-content/60">{t("backstageHint")}</p>
              </div>

              <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto px-4 py-4">
                {backstageMessages.map((message) => (
                  <div key={message.id} className="card card-border bg-base-100">
                    <div className="card-body gap-2 p-3">
                      <div className="flex flex-wrap items-center gap-2 text-xs text-base-content/60">
                        <span className={`badge ${senderBadgeClass(message)}`}>
                          {message.senderName}
                        </span>
                        <span className="badge badge-neutral">{message.kind}</span>
                        <time>{formatTime(message.createdAt, language)}</time>
                      </div>
                      <div className="whitespace-pre-wrap text-sm">{message.content}</div>
                      {message.taskCardId ? (
                        <button
                          className="btn btn-link btn-sm mt-1 px-0"
                          onClick={() => onSelectTask(message.taskCardId ?? undefined)}
                        >
                          {t("openTask")}
                        </button>
                      ) : null}
                    </div>
                  </div>
                ))}

                {backstageMessages.length === 0 ? (
                  <div className="hero min-h-40 rounded-box bg-base-200">
                    <div className="hero-content text-center">
                      <div className="max-w-xs">
                        <h3 className="text-base font-semibold">{t("noBackstageMessages")}</h3>
                        <p className="mt-2 text-sm text-base-content/60">
                          {t("backstageHint")}
                        </p>
                      </div>
                    </div>
                  </div>
                ) : null}
              </div>
            </aside>
          ) : null}
        </div>

        <div className="border-t border-base-300 p-4">
          <form
            className="flex flex-col overflow-hidden rounded-2xl border-2 border-primary/20 bg-base-100 shadow-sm transition-colors focus-within:border-primary"
            onSubmit={(e) => {
              if (composerValue.trim()) {
                onSendMessage(e);
              } else {
                e.preventDefault();
              }
            }}
          >
            <textarea
              className="textarea w-full resize-none border-0 bg-transparent px-4 pb-2 pt-4 text-sm focus:outline-none"
              placeholder={t("taskPlaceholder")}
              rows={3}
              value={composerValue}
              onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                onComposerChange(event.target.value)
              }
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  if (composerValue.trim()) {
                    onSendMessage(e as unknown as FormEvent<HTMLFormElement>);
                  }
                }
              }}
            />
            <div className="flex items-center justify-between bg-base-100 px-2 pb-2 pt-1">
              <div className="flex items-center gap-1 text-base-content/60">
                <div className="tooltip tooltip-top" data-tip={t("attachFile", "Attach file")}>
                  <button type="button" className="btn btn-circle btn-ghost btn-sm">
                    <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
                    </svg>
                  </button>
                </div>
                <div className="tooltip tooltip-top" data-tip={t("mentionAgent", "Mention Agent")}>
                  <button type="button" className="btn btn-circle btn-ghost btn-sm">
                    <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
                    </svg>
                  </button>
                </div>
                <div className="tooltip tooltip-top" data-tip={t("createTaskCard", "Create Task Card")}>
                  <button type="button" className="btn btn-circle btn-ghost btn-sm">
                    <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
                    </svg>
                  </button>
                </div>
                <div className="tooltip tooltip-top" data-tip={t("invokeTool", "Invoke Tool")}>
                  <button type="button" className="btn btn-circle btn-ghost btn-sm">
                    <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                  </button>
                </div>
              </div>

              <div className="flex items-center gap-2 pr-1 text-sm">
                <span className="hidden text-xs text-base-content/40 sm:inline-block">
                  Enter to send
                </span>
                <details className="dropdown dropdown-top dropdown-end">
                  <summary className="btn btn-ghost btn-sm px-2 gap-1 text-base-content/70">
                    <span className="badge badge-neutral badge-sm">Summary</span>
                    <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 15l7-7 7 7" />
                    </svg>
                  </summary>
                  <ul className="dropdown-content menu bg-base-200 rounded-box z-[1] w-52 p-2 shadow">
                    <li><a>Summary</a></li>
                    <li><a>Backstage</a></li>
                  </ul>
                </details>

                <button
                  type="submit"
                  className="btn btn-circle btn-primary btn-sm"
                  disabled={!currentWorkGroup || loading || !composerValue.trim()}
                >
                  <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 10l7-7m0 0l7 7m-7-7v18" />
                  </svg>
                </button>
              </div>
            </div>
          </form>
        </div>
      </div>
    </section>
  );
}
