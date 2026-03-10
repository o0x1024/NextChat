import {
  type ChangeEvent,
  type FormEvent,
  type MouseEvent as ReactMouseEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import type { Language } from "../../store/preferencesStore";
import type {
  AgentProfile,
  ClaimBid,
  ConversationMessage,
  CreateWorkGroupInput,
  Lease,
  SystemSettings,
  TaskCard,
  ToolManifest,
  ToolRun,
  UpdateWorkGroupInput,
  WorkGroup,
} from "../../types";
import {
  activeMentionDraft,
  insertMention,
  mentionCandidates,
  validateMentions,
} from "./mentions";
import { formatTime, roleAccent, statusBadgeClass } from "./ui";

interface ChatManagementProps {
  workGroups: WorkGroup[];
  agents: AgentProfile[];
  messages: ConversationMessage[];
  taskCards: TaskCard[];
  leases: Lease[];
  claimBids: ClaimBid[];
  toolRuns: ToolRun[];
  tools: ToolManifest[];
  settings: SystemSettings;
  selectedWorkGroupId?: string;
  language: Language;
  backstageOpen: boolean;
  onSelectWorkGroup: (id: string) => void;
  onCreateGroup: (input: CreateWorkGroupInput) => Promise<void>;
  onUpdateGroup: (input: UpdateWorkGroupInput) => Promise<void>;
  onSendMessage: (workGroupId: string, content: string) => Promise<void>;
  onAddAgent: (workGroupId: string, agentId: string) => Promise<void>;
  onRemoveAgent: (workGroupId: string, agentId: string) => Promise<void>;
  onApproveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  onToggleBackstage: () => void;
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

type PanelTarget =
  | { section: "tasks"; taskId?: string }
  | { section: "approvals" };

type StreamPhase = "start" | "delta" | "done";

interface AgentStreamPayload {
  streamId: string;
  phase: StreamPhase;
  conversationId: string;
  workGroupId: string;
  senderId: string;
  senderName: string;
  kind: ConversationMessage["kind"];
  visibility: ConversationMessage["visibility"];
  taskCardId?: string | null;
  sequence: number;
  delta?: string | null;
  fullContent?: string | null;
  createdAt: string;
}

interface AgentStreamTrack {
  streamId: string;
  senderId: string;
  senderName: string;
  status: "streaming" | "completed";
  kind: ConversationMessage["kind"];
  visibility: ConversationMessage["visibility"];
  taskCardId?: string | null;
  content: string;
  updatedAt: string;
  createdAt: string;
}

function streamStatusBadgeClass(status: AgentStreamTrack["status"]) {
  return status === "streaming" ? "badge-info" : "badge-success";
}

const emptyGroupForm: CreateWorkGroupInput = {
  name: "",
  goal: "",
  kind: "persistent",
  defaultVisibility: "summary",
  autoArchive: false,
};

const emptyEditGroupForm: UpdateWorkGroupInput = {
  id: "",
  ...emptyGroupForm,
};

export function ChatManagement({
  workGroups,
  agents,
  messages,
  taskCards,
  leases,
  claimBids,
  toolRuns,
  tools,
  settings,
  selectedWorkGroupId,
  language,
  backstageOpen,
  onSelectWorkGroup,
  onCreateGroup,
  onUpdateGroup,
  onSendMessage,
  onAddAgent,
  onRemoveAgent,
  onApproveRun,
  onToggleBackstage,
}: ChatManagementProps) {
  const { t } = useTranslation();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(256);
  const [resizingSidebar, setResizingSidebar] = useState(false);
  const [runningPanelOpen, setRunningPanelOpen] = useState(false);
  const [composerValue, setComposerValue] = useState("");
  const [mentionIndex, setMentionIndex] = useState(0);
  const [mentionError, setMentionError] = useState<string | null>(null);
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [editModalOpen, setEditModalOpen] = useState(false);
  const [memberModalOpen, setMemberModalOpen] = useState(false);
  const [highlightedTaskId, setHighlightedTaskId] = useState<string | null>(null);
  const [panelTarget, setPanelTarget] = useState<PanelTarget | null>(null);
  const [groupForm, setGroupForm] = useState<CreateWorkGroupInput>(emptyGroupForm);
  const [editGroupForm, setEditGroupForm] = useState<UpdateWorkGroupInput>(emptyEditGroupForm);
  const [streamingEnabled, setStreamingEnabled] = useState(true);
  const [focusAgentId, setFocusAgentId] = useState<string | null>(null);
  const [showAllStreams, setShowAllStreams] = useState(false);
  const [streamTracks, setStreamTracks] = useState<Record<string, AgentStreamTrack>>({});
  const pendingDeltasRef = useRef<Record<string, string>>({});
  const doneQueueRef = useRef<Record<string, Partial<AgentStreamTrack>>>({});
  const streamedMessageIdsRef = useRef<Set<string>>(new Set());
  const fallbackMessageIdsRef = useRef<Set<string>>(new Set());
  const rootRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const taskBoardRef = useRef<HTMLDivElement | null>(null);
  const approvalsRef = useRef<HTMLDivElement | null>(null);
  const taskCardRefs = useRef<Record<string, HTMLDivElement | null>>({});

  const currentGroup = useMemo(
    () => workGroups.find((group) => group.id === selectedWorkGroupId) ?? workGroups[0],
    [selectedWorkGroupId, workGroups],
  );

  const currentMessages = useMemo(() => {
    if (!currentGroup) return [];
    const allMessages = messages.filter((message) => message.workGroupId === currentGroup.id);
    return backstageOpen
      ? allMessages
      : allMessages.filter((message) => message.visibility === "main");
  }, [backstageOpen, currentGroup, messages]);

  const streamTrackList = useMemo(
    () =>
      Object.values(streamTracks)
        .filter((track) => track.kind === "summary")
        .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)),
    [streamTracks],
  );

  const visibleStreamTracks = useMemo(() => {
    if (showAllStreams) {
      return streamTrackList;
    }
    if (focusAgentId) {
      return streamTrackList.filter((track) => track.senderId === focusAgentId);
    }
    return streamTrackList.slice(0, 1);
  }, [focusAgentId, showAllStreams, streamTrackList]);

  const streamingCount = streamTrackList.filter((track) => track.status === "streaming").length;

  const currentMembers = useMemo(() => {
    if (!currentGroup) return [];
    const memberIds = new Set(currentGroup.memberAgentIds);
    return agents.filter((agent) => memberIds.has(agent.id));
  }, [agents, currentGroup]);
  const mentionDraft = useMemo(() => {
    const caret = textareaRef.current?.selectionStart ?? composerValue.length;
    return activeMentionDraft(composerValue, caret);
  }, [composerValue, currentMembers]);
  const mentionOptions = useMemo(
    () => mentionCandidates(currentMembers, mentionDraft?.query ?? "").slice(0, 6),
    [currentMembers, mentionDraft],
  );

  const currentGroupTasks = useMemo(() => {
    if (!currentGroup) return [];
    return taskCards
      .filter((task) => task.workGroupId === currentGroup.id)
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
  }, [currentGroup, taskCards]);

  const activeTasks = useMemo(
    () =>
      currentGroupTasks.filter(
        (task) => !["completed", "cancelled"].includes(task.status),
      ),
    [currentGroupTasks],
  );
  const activeTaskIds = useMemo(
    () => new Set(activeTasks.map((task) => task.id)),
    [activeTasks],
  );

  const currentTaskIds = useMemo(
    () => new Set(currentGroupTasks.map((task) => task.id)),
    [currentGroupTasks],
  );
  const currentTaskTitles = useMemo(
    () => new Map(currentGroupTasks.map((task) => [task.id, task.title])),
    [currentGroupTasks],
  );

  const currentLeases = useMemo(
    () =>
      leases.filter(
        (lease) => currentTaskIds.has(lease.taskCardId) && lease.state !== "released",
      ),
    [currentTaskIds, leases],
  );

  const currentApprovals = useMemo(
    () =>
      toolRuns.filter(
        (run) =>
          currentTaskIds.has(run.taskCardId) &&
          run.approvalRequired &&
          run.state === "pending_approval",
      ),
    [currentTaskIds, toolRuns],
  );

  async function handleSend(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!currentGroup || !composerValue.trim()) return;
    const validation = validateMentions(composerValue, currentMembers);
    if (validation.invalidMentions.length > 0) {
      setMentionError(
        t("mentionUnknownAgents", {
          names: validation.invalidMentions.map((item) => `@${item}`).join(", "),
        }),
      );
      return;
    }
    if (validation.ambiguousMentions.length > 0) {
      setMentionError(
        t("mentionAmbiguousAgents", {
          names: validation.ambiguousMentions.map((item) => `@${item}`).join(", "),
        }),
      );
      return;
    }
    await onSendMessage(currentGroup.id, composerValue.trim());
    setComposerValue("");
    setMentionError(null);
  }

  async function handleCreateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onCreateGroup(groupForm);
    setGroupForm(emptyGroupForm);
    setCreateModalOpen(false);
  }

  async function handleUpdateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!currentGroup) return;
    await onUpdateGroup(editGroupForm);
    setEditModalOpen(false);
  }

  async function handleToggleMember(agent: AgentProfile) {
    if (!currentGroup) return;
    if (currentGroup.memberAgentIds.includes(agent.id)) {
      await onRemoveAgent(currentGroup.id, agent.id);
      return;
    }
    await onAddAgent(currentGroup.id, agent.id);
  }

  useEffect(() => {
    setMentionIndex(0);
  }, [mentionDraft?.start, mentionDraft?.query]);

  useEffect(() => {
    setStreamTracks({});
    pendingDeltasRef.current = {};
    doneQueueRef.current = {};
    streamedMessageIdsRef.current = new Set();
    fallbackMessageIdsRef.current = new Set();
  }, [currentGroup?.id]);

  useEffect(() => {
    if (!currentGroup) {
      return;
    }

    const off: Array<() => void> = [];
    const register = async (eventName: string) => {
      const unlisten = await listen<unknown>(eventName, (event) => {
        const payload = event.payload as AgentStreamPayload | undefined;
        if (!payload || payload.workGroupId !== currentGroup.id) {
          return;
        }
        if (payload.phase === "start") {
          setStreamTracks((current) => ({
            ...current,
            [payload.streamId]: {
              streamId: payload.streamId,
              senderId: payload.senderId,
              senderName: payload.senderName,
              status: "streaming",
              kind: payload.kind,
              visibility: payload.visibility,
              taskCardId: payload.taskCardId ?? null,
              content: "",
              createdAt: payload.createdAt,
              updatedAt: payload.createdAt,
            },
          }));
          streamedMessageIdsRef.current.add(payload.streamId);
          return;
        }
        if (payload.phase === "delta") {
          pendingDeltasRef.current[payload.streamId] =
            (pendingDeltasRef.current[payload.streamId] ?? "") + (payload.delta ?? "");
          return;
        }
        if (payload.phase === "done") {
          doneQueueRef.current[payload.streamId] = {
            status: "completed",
            updatedAt: payload.createdAt,
            content: payload.fullContent ?? undefined,
          };
        }
      });
      off.push(() => {
        void unlisten();
      });
    };

    void Promise.all([
      register("chat:stream-start"),
      register("chat:stream-delta"),
      register("chat:stream-done"),
    ]);

    return () => {
      off.forEach((stop) => stop());
    };
  }, [currentGroup?.id]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const pending = pendingDeltasRef.current;
      const doneQueue = doneQueueRef.current;
      const hasPending =
        Object.keys(pending).length > 0 || Object.keys(doneQueue).length > 0;
      if (!hasPending) {
        return;
      }
      setStreamTracks((current) => {
        const next = { ...current };
        for (const [streamId, delta] of Object.entries(pending)) {
          const track = next[streamId];
          if (!track) continue;
          next[streamId] = {
            ...track,
            content: `${track.content}${delta}`,
            updatedAt: new Date().toISOString(),
          };
        }
        for (const [streamId, done] of Object.entries(doneQueue)) {
          const track = next[streamId];
          if (!track) continue;
          next[streamId] = {
            ...track,
            status: "completed",
            content: done.content ?? track.content,
            updatedAt: done.updatedAt ?? new Date().toISOString(),
          };
        }
        return next;
      });
      pendingDeltasRef.current = {};
      doneQueueRef.current = {};
    }, 150);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!currentGroup || !streamingEnabled) {
      return;
    }
    const fallbackCandidates = currentMessages.filter(
      (message) =>
        message.senderKind === "agent" &&
        message.kind === "summary" &&
        !streamedMessageIdsRef.current.has(message.id),
    );
    if (fallbackCandidates.length === 0) {
      return;
    }
    const newlySeen = fallbackCandidates.filter(
      (message) => !fallbackMessageIdsRef.current.has(message.id),
    );
    if (newlySeen.length === 0) {
      return;
    }
    setStreamTracks((current) => {
      const next = { ...current };
      for (const message of newlySeen) {
        fallbackMessageIdsRef.current.add(message.id);
        next[message.id] = {
          streamId: message.id,
          senderId: message.senderId,
          senderName: message.senderName,
          status: "completed",
          kind: message.kind,
          visibility: message.visibility,
          taskCardId: message.taskCardId ?? null,
          content: message.content,
          createdAt: message.createdAt,
          updatedAt: message.createdAt,
        };
      }
      return next;
    });
  }, [currentGroup, currentMessages, streamingEnabled]);

  useEffect(() => {
    if (!runningPanelOpen || !panelTarget) {
      return;
    }

    const target =
      panelTarget.section === "approvals"
        ? approvalsRef.current
        : panelTarget.taskId
          ? taskCardRefs.current[panelTarget.taskId]
          : taskBoardRef.current;

    if (!target) {
      return;
    }

    target.scrollIntoView({ behavior: "smooth", block: "nearest" });
    const clearTimer = window.setTimeout(() => setPanelTarget(null), 500);
    return () => window.clearTimeout(clearTimer);
  }, [panelTarget, runningPanelOpen, activeTasks.length, currentApprovals.length]);

  useEffect(() => {
    if (!highlightedTaskId) {
      return;
    }

    const clearTimer = window.setTimeout(() => setHighlightedTaskId(null), 1800);
    return () => window.clearTimeout(clearTimer);
  }, [highlightedTaskId]);

  function applyMention(agent: AgentProfile) {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    const caret = textarea.selectionStart ?? composerValue.length;
    const existingDraft = activeMentionDraft(composerValue, caret);
    const hasTrigger = composerValue.slice(Math.max(0, caret - 1), caret) === "@";
    const sourceValue = hasTrigger
      ? composerValue
      : `${composerValue.slice(0, caret)}@${composerValue.slice(caret)}`;
    const draft =
      existingDraft ??
      (hasTrigger
        ? { start: caret - 1, end: caret, query: "" }
        : { start: caret, end: caret + 1, query: "" });
    const { nextValue, caret: nextCaret } = insertMention(sourceValue, draft, agent);
    setComposerValue(nextValue);
    setMentionError(null);
    requestAnimationFrame(() => {
      textarea.focus();
      textarea.setSelectionRange(nextCaret, nextCaret);
    });
  }

  function openMentionPicker() {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }
    const caret = textarea.selectionStart ?? composerValue.length;
    const hasTrigger = composerValue.slice(Math.max(0, caret - 1), caret) === "@";
    if (!hasTrigger) {
      const nextValue = `${composerValue.slice(0, caret)}@${composerValue.slice(caret)}`;
      setComposerValue(nextValue);
      requestAnimationFrame(() => {
        textarea.focus();
        const nextCaret = caret + 1;
        textarea.setSelectionRange(nextCaret, nextCaret);
      });
    } else {
      textarea.focus();
    }
    setMentionError(null);
  }

  function jumpToApprovals() {
    setRunningPanelOpen(true);
    setPanelTarget({ section: "approvals" });
  }

  function jumpToTaskBoard(taskId?: string) {
    setRunningPanelOpen(true);
    setPanelTarget({ section: "tasks", taskId });
    setHighlightedTaskId(taskId ?? null);
  }

  function openEditGroupModal() {
    if (!currentGroup) return;
    setEditGroupForm({
      id: currentGroup.id,
      name: currentGroup.name,
      goal: currentGroup.goal,
      kind: currentGroup.kind,
      defaultVisibility: currentGroup.defaultVisibility,
      autoArchive: currentGroup.autoArchive,
    });
    setEditModalOpen(true);
  }

  return (
    <div className="flex h-full">
      {sidebarOpen && (
        <div className="flex w-64 shrink-0 flex-col border-r border-base-content/10 transition-all duration-300">
          <div className="flex items-center justify-between border-b border-base-content/10 px-4 py-3">
            <h2 className="text-sm font-bold">{t("chatManagement")}</h2>
            <div className="flex items-center gap-1">
              <button className="btn btn-primary btn-xs" onClick={() => setCreateModalOpen(true)}>
                + {t("create")}
              </button>
              <button className="btn btn-ghost btn-xs" onClick={() => setSidebarOpen(false)}>
                <i className="fas fa-indent" />
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto ">
            <ul className="menu menu-sm gap-0.5 p-2">
              {workGroups.map((group) => (
                <li key={group.id}>
                  <a
                    className={`flex items-center gap-2 rounded-lg py-2.5 ${currentGroup?.id === group.id ? "menu-active" : ""
                      }`}
                    onClick={() => onSelectWorkGroup(group.id)}
                  >
                    <div className="grid h-8 w-8 place-items-center rounded-lg border border-primary/20 bg-primary/10 text-[10px] font-bold text-primary">
                      {group.name.slice(0, 2).toUpperCase()}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium">{group.name}</div>
                      <div className="truncate text-xs text-base-content/50">{group.goal}</div>
                    </div>
                    <span className="badge badge-ghost badge-xs">
                      {group.kind === "persistent" ? "P" : "E"}
                    </span>
                  </a>
                </li>
              ))}
              {workGroups.length === 0 && (
                <li className="py-4 text-center text-sm text-base-content/50">
                  {t("noWorkGroupsYet")}
                </li>
              )}
            </ul>
          </div>
        </div>
      )}

      <div className="flex min-w-0 flex-1 flex-col">
        {currentGroup ? (
          <>
            <div className="flex items-center justify-between border-b border-base-content/10 px-5 py-3">
              <div className="flex min-w-0 items-center gap-3">
                {!sidebarOpen && (
                  <button className="btn btn-ghost btn-sm -ml-2" onClick={() => setSidebarOpen(true)}>
                    <i className="fas fa-outdent" />
                  </button>
                )}
                <h3 className="truncate text-base font-bold">{currentGroup.name}</h3>
                <span className="badge badge-ghost badge-sm shrink-0">
                  {currentGroup.kind === "persistent" ? t("persistent") : t("ephemeral")}
                </span>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  className={`btn btn-sm ${streamingEnabled ? "btn-primary" : "btn-ghost"}`}
                  onClick={() => setStreamingEnabled((current) => !current)}
                >
                  {streamingEnabled ? t("streamOn") : t("streamOff")}
                </button>
                <button
                  className={`btn btn-sm ${backstageOpen ? "btn-primary" : "btn-ghost"}`}
                  onClick={onToggleBackstage}
                >
                  {backstageOpen ? t("mainFeed") : t("backstage")}
                </button>
                <button
                  className={`btn btn-sm ${runningPanelOpen ? "btn-primary" : "btn-ghost"}`}
                  onClick={() => setRunningPanelOpen(!runningPanelOpen)}
                >
                  {t("runningPanel")}
                </button>
                <button className="btn btn-ghost btn-sm" onClick={openEditGroupModal}>
                  {t("edit")}
                </button>
                <button className="btn btn-ghost btn-sm" onClick={() => setMemberModalOpen(true)}>
                  {t("members")} ({currentMembers.length})
                </button>
                {currentMembers.length > 0 ? (
                  <select
                    className="select select-bordered select-sm w-40"
                    value={focusAgentId ?? ""}
                    onChange={(event) => setFocusAgentId(event.target.value || null)}
                  >
                    <option value="">{t("focusAuto")}</option>
                    {currentMembers.map((agent) => (
                      <option key={agent.id} value={agent.id}>
                        {agent.name}
                      </option>
                    ))}
                  </select>
                ) : null}
              </div>
            </div>

            {currentMembers.length > 0 && (
              <div className="flex items-center gap-2 overflow-x-auto border-b border-base-content/5 px-5 py-2">
                {currentMembers.map((agent) => {
                  const accent = roleAccent(agent.role);
                  return (
                    <button
                      type="button"
                      key={agent.id}
                      className="badge badge-ghost shrink-0 gap-1.5 py-3 transition-colors hover:border-primary hover:text-primary"
                      style={{ borderColor: `${accent}40`, color: accent }}
                      onClick={() => applyMention(agent)}
                    >
                      <span className="text-[10px] font-bold">{agent.avatar}</span>
                      {agent.name}
                    </button>
                  );
                })}
              </div>
            )}

            <div className={`grid min-h-0 flex-1 gap-4 px-5 py-4 ${runningPanelOpen ? "xl:grid-cols-[minmax(0,1fr)_340px]" : "grid-cols-1"}`}>
              <div className="flex min-h-0 flex-col">
                <section className="mb-3 max-h-72 shrink-0 overflow-hidden rounded-box border border-base-content/10 bg-base-100 p-3">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <strong className="text-sm">{t("agentStreams")}</strong>
                      <span className="badge badge-ghost badge-sm">
                        {t("agentStreamsCount", { count: streamTrackList.length })}
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="badge badge-info badge-sm">
                        {t("runningCount", { count: streamingCount })}
                      </span>
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => setShowAllStreams((current) => !current)}
                        disabled={streamTrackList.length <= 1}
                      >
                        {showAllStreams ? t("collapseStreams") : t("showAllStreams")}
                      </button>
                    </div>
                  </div>
                  {streamTrackList.length > 0 ? (
                    <div className="max-h-56 space-y-2 overflow-y-auto pr-1">
                      {visibleStreamTracks.map((track) => (
                        <div key={track.streamId} className="rounded-box bg-base-200 px-3 py-2">
                          <div className="mb-1 flex items-center justify-between gap-2 text-xs">
                            <div className="flex items-center gap-2">
                              <span className="font-semibold">{track.senderName}</span>
                              <span className={`badge badge-xs ${streamStatusBadgeClass(track.status)}`}>
                                {track.status === "streaming"
                                  ? t("streamingStatus")
                                  : t("streamCompletedStatus")}
                              </span>
                            </div>
                            <time className="text-base-content/60">
                              {formatTime(track.updatedAt, language)}
                            </time>
                          </div>
                          <div className="max-h-32 overflow-y-auto whitespace-pre-wrap pr-1 text-sm">
                            {streamingEnabled || track.status === "completed"
                              ? track.content || t("streamingStatus")
                              : t("streamPausedPreview")}
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="text-xs text-base-content/60">{t("noAgentStreams")}</div>
                  )}
                </section>

                <div className="min-h-0 flex-1 space-y-1 overflow-y-auto">
                  {currentMessages.map((message) => (
                    <div
                      key={message.id}
                      className={`chat ${message.senderKind === "human" ? "chat-end" : "chat-start"}`}
                    >
                      <div className="chat-header mb-1 text-xs text-base-content/50">
                        <span className="font-medium">{message.senderName}</span>
                        {message.visibility === "backstage" && (
                          <span className="ml-1 badge badge-warning badge-xs">{t("backstage")}</span>
                        )}
                        {message.kind === "collaboration" && (
                          <span className="ml-1 badge badge-info badge-xs">{t("collaboration")}</span>
                        )}
                        {message.taskCardId && currentTaskTitles.get(message.taskCardId) && (
                          activeTaskIds.has(message.taskCardId) ? (
                            <button
                              type="button"
                              className="ml-1 badge badge-ghost badge-xs transition-colors hover:border-primary hover:text-primary"
                              title={t("openTask")}
                              onClick={() => jumpToTaskBoard(message.taskCardId ?? undefined)}
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
                      <div className={`chat-bubble whitespace-pre-wrap text-sm ${senderBubbleClass(message)}`}>
                        {streamingEnabled &&
                        message.senderKind === "agent" &&
                        message.kind === "summary"
                          ? t("agentSummaryEvent")
                          : message.content}
                      </div>
                    </div>
                  ))}

                  {currentMessages.length === 0 && (
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

                <div className="mt-auto shrink-0 px-5 pb-6 pt-3">
                  <form
                    className="flex flex-col bg-base-100 rounded-2xl border border-primary/20 shadow-sm focus-within:ring-2 focus-within:ring-primary/10 transition-all p-2 pt-3"
                    onSubmit={(event) => {
                      if (composerValue.trim()) {
                        void handleSend(event);
                      } else {
                        event.preventDefault();
                      }
                    }}
                  >
                    <textarea
                      ref={textareaRef}
                      className="textarea textarea-ghost w-full min-h-[60px] resize-none focus:outline-none bg-transparent placeholder:opacity-30 text-sm leading-relaxed"
                      placeholder={t("taskPlaceholder")}
                      rows={2}
                      value={composerValue}
                      onChange={(event: ChangeEvent<HTMLTextAreaElement>) => {
                        setComposerValue(event.target.value);
                        setMentionError(null);
                      }}
                      onKeyDown={(event) => {
                        if (mentionDraft && mentionOptions.length > 0) {
                          if (event.key === "ArrowDown") {
                            event.preventDefault();
                            setMentionIndex((current) => (current + 1) % mentionOptions.length);
                            return;
                          }
                          if (event.key === "ArrowUp") {
                            event.preventDefault();
                            setMentionIndex((current) =>
                              current === 0 ? mentionOptions.length - 1 : current - 1,
                            );
                            return;
                          }
                          if (event.key === "Enter" || event.key === "Tab") {
                            event.preventDefault();
                            applyMention(mentionOptions[mentionIndex] ?? mentionOptions[0]);
                            return;
                          }
                          if (event.key === "Escape") {
                            event.preventDefault();
                            setComposerValue((value) =>
                              value.slice(0, mentionDraft.start) +
                              value.slice(mentionDraft.start + 1),
                            );
                            return;
                          }
                        }
                        if (event.key === "Enter" && !event.shiftKey) {
                          event.preventDefault();
                          if (composerValue.trim()) {
                            void handleSend(event as unknown as FormEvent<HTMLFormElement>);
                          }
                        }
                      }}
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
                                  applyMention(agent);
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
                          className="btn btn-ghost btn-xs gap-1"
                          title={t("mentionAgent")}
                          onClick={openMentionPicker}
                        >
                          <i className="fas fa-at text-xs" />
                          {t("mentionAgent")}
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs gap-1"
                          title={currentApprovals.length > 0 ? t("openApprovalsQueue") : t("noPendingApprovals")}
                          onClick={jumpToApprovals}
                          disabled={currentApprovals.length === 0}
                        >
                          <i className="fas fa-shield-halved text-xs" />
                          {t("approvals")} ({currentApprovals.length})
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs gap-1"
                          title={activeTasks.length > 0 ? t("openTaskBoard") : t("noActiveTasksInGroup")}
                          onClick={() => jumpToTaskBoard()}
                          disabled={activeTasks.length === 0}
                        >
                          <i className="fas fa-list-check text-xs" />
                          {t("taskBoard")} ({activeTasks.length})
                        </button>
                        <span className="badge badge-ghost gap-1" title={t("toolsAutoSelectHint")}>
                          <i className="fas fa-tools text-[10px]" />
                          {t("toolsAutoSelect")}
                        </span>
                      </div>

                      <div className="flex items-center gap-4">
                        <div
                          className="flex items-center gap-1.5 text-[10px] font-bold text-base-content/40"
                          title={t("toolsAutoSelectHint")}
                        >
                          {settings.globalConfig.defaultLLMModel} <i className="fas fa-chevron-down text-[8px]" />
                        </div>
                        <div className="text-base-content/30"><i className="fas fa-language text-xs" /></div>
                        <button
                          type="submit"
                          className={`btn btn-circle btn-xs w-7 h-7 min-h-0 border-none transition-all ${composerValue.trim()
                            ? "bg-primary text-primary-content hover:scale-110 shadow-lg shadow-primary/20"
                            : "bg-base-200 text-base-content/20"
                            }`}
                          disabled={!composerValue.trim()}
                        >
                          <i className="fas fa-arrow-up text-[10px]" />
                        </button>
                      </div>
                    </div>
                  </form>
                </div>
              </div>

              {runningPanelOpen && (
                <aside className="flex min-h-0 flex-col gap-3 overflow-y-auto w-[340px]">
                  <section className="card card-border bg-base-100">
                    <div className="card-body gap-3">
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("runningPanel")}</h3>
                        <span className="badge badge-secondary">{t("groupBound")}</span>
                      </div>
                      <p className="text-sm text-base-content/60">{t("runningPanelHint")}</p>
                      <div className="stats stats-vertical bg-base-200">
                        <div className="stat px-4 py-3">
                          <div className="stat-title">{t("activeTasks")}</div>
                          <div className="stat-value text-base">{activeTasks.length}</div>
                        </div>
                        <div className="stat px-4 py-3">
                          <div className="stat-title">{t("leasesTitle")}</div>
                          <div className="stat-value text-base">{currentLeases.length}</div>
                        </div>
                        <div className="stat px-4 py-3">
                          <div className="stat-title">{t("approvals")}</div>
                          <div className="stat-value text-base">{currentApprovals.length}</div>
                        </div>
                      </div>
                    </div>
                  </section>

                  <section className="card card-border bg-base-100">
                    <div className="card-body gap-3" ref={taskBoardRef}>
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("taskBoard")}</h3>
                        <span className="badge badge-primary">{activeTasks.length}</span>
                      </div>
                      <div className="space-y-2">
                        {activeTasks.slice(0, 8).map((task) => {
                          const lease = currentLeases.find((item) => item.taskCardId === task.id);
                          const owner = agents.find((agent) => agent.id === lease?.ownerAgentId);
                          const bidCount = claimBids.filter((bid) => bid.taskCardId === task.id).length;
                          return (
                            <div
                              key={task.id}
                              ref={(node) => {
                                taskCardRefs.current[task.id] = node;
                              }}
                              className={`rounded-box px-4 py-3 transition-colors ${
                                highlightedTaskId === task.id
                                  ? "bg-primary/10 ring-1 ring-primary/30"
                                  : "bg-base-200"
                              }`}
                            >
                              <div className="mb-2 flex items-start justify-between gap-3">
                                <strong className="line-clamp-2 text-sm">{task.title}</strong>
                                <span className={`badge shrink-0 ${statusBadgeClass(task.status)}`}>
                                  {t(`taskStatus.${task.status}`)}
                                </span>
                              </div>
                              <p className="line-clamp-2 text-xs text-base-content/65">
                                {task.normalizedGoal}
                              </p>
                              <div className="mt-3 flex flex-wrap gap-2 text-xs">
                                <span className="badge badge-neutral">
                                  {owner ? t("ownerLabel", { name: owner.name }) : t("unassigned")}
                                </span>
                                <span className="badge badge-ghost">
                                  {t("bidsCount", { count: bidCount })}
                                </span>
                                <button
                                  type="button"
                                  className="btn btn-ghost btn-xs"
                                  onClick={() => jumpToTaskBoard(task.id)}
                                >
                                  {t("openTask")}
                                </button>
                              </div>
                            </div>
                          );
                        })}

                        {activeTasks.length === 0 && (
                          <div className="alert alert-soft">
                            <span className="text-sm">{t("noActiveTasksInGroup")}</span>
                          </div>
                        )}
                      </div>
                    </div>
                  </section>

                  <section className="card card-border bg-base-100">
                    <div className="card-body gap-3" ref={approvalsRef}>
                      <div className="flex items-center justify-between">
                        <h3 className="card-title text-base">{t("approvals")}</h3>
                        <span className="badge badge-warning">{currentApprovals.length}</span>
                      </div>
                      <div className="space-y-2">
                        {currentApprovals.map((run) => {
                          const task = currentGroupTasks.find((item) => item.id === run.taskCardId);
                          const tool = tools.find((item) => item.id === run.toolId);
                          return (
                            <div key={run.id} className="rounded-box bg-warning/10 px-4 py-3">
                              <div className="mb-1 flex items-start justify-between gap-2">
                                <strong className="line-clamp-1 text-sm">
                                  {tool?.name ?? run.toolId}
                                </strong>
                                <span className="badge badge-warning shrink-0">{t("approval")}</span>
                              </div>
                              <p className="text-xs text-base-content/70">
                                {task?.title ?? t("toolRunNeedsApproval", { id: run.id.slice(0, 8) })}
                              </p>
                              <div className="mt-3 flex gap-2">
                                <button
                                  className="btn btn-primary btn-xs"
                                  onClick={() => void onApproveRun(run.id, true)}
                                >
                                  {t("approve")}
                                </button>
                                <button
                                  className="btn btn-soft btn-xs"
                                  onClick={() => void onApproveRun(run.id, false)}
                                >
                                  {t("reject")}
                                </button>
                                {task ? (
                                  <button
                                    type="button"
                                    className="btn btn-ghost btn-xs"
                                    onClick={() => jumpToTaskBoard(task.id)}
                                  >
                                    {t("openTask")}
                                  </button>
                                ) : null}
                              </div>
                            </div>
                          );
                        })}

                        {currentApprovals.length === 0 && (
                          <div className="alert alert-soft">
                            <span className="text-sm">{t("noPendingApprovals")}</span>
                          </div>
                        )}
                      </div>
                    </div>
                  </section>
                </aside>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center">
            <div className="text-center text-base-content/50">
              <div className="mb-3 text-4xl">💬</div>
              <div>{t("noWorkGroupSelected")}</div>
            </div>
          </div>
        )}
      </div>

      {createModalOpen && (
        <dialog className="modal modal-open" onClick={() => setCreateModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => setCreateModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">{t("createWorkGroup")}</h3>
            <form className="space-y-3" onSubmit={(event) => void handleCreateGroup(event)}>
              <input
                className="input input-bordered input-sm w-full"
                placeholder={t("name")}
                required
                value={groupForm.name}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  setGroupForm((form) => ({ ...form, name: event.target.value }))
                }
              />
              <textarea
                className="textarea textarea-bordered textarea-sm w-full"
                placeholder={t("sharedGoal")}
                rows={2}
                value={groupForm.goal}
                onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                  setGroupForm((form) => ({ ...form, goal: event.target.value }))
                }
              />
              <select
                className="select select-bordered select-sm w-full"
                value={groupForm.kind}
                onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                  setGroupForm((form) => ({
                    ...form,
                    kind: event.target.value as CreateWorkGroupInput["kind"],
                  }))
                }
              >
                <option value="persistent">{t("persistent")}</option>
                <option value="ephemeral">{t("ephemeral")}</option>
              </select>
              <div className="modal-action">
                <button className="btn btn-ghost btn-sm" type="button" onClick={() => setCreateModalOpen(false)}>
                  {t("cancel")}
                </button>
                <button className="btn btn-primary btn-sm" type="submit">
                  {t("createGroup")}
                </button>
              </div>
            </form>
          </div>
        </dialog>
      )}

      {editModalOpen && currentGroup && (
        <dialog className="modal modal-open" onClick={() => setEditModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => setEditModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">
              {t("edit")} - {currentGroup.name}
            </h3>
            <form className="space-y-3" onSubmit={(event) => void handleUpdateGroup(event)}>
              <input
                className="input input-bordered input-sm w-full"
                placeholder={t("name")}
                required
                value={editGroupForm.name}
                onChange={(event: ChangeEvent<HTMLInputElement>) =>
                  setEditGroupForm((form) => ({ ...form, name: event.target.value }))
                }
              />
              <textarea
                className="textarea textarea-bordered textarea-sm w-full"
                placeholder={t("sharedGoal")}
                rows={2}
                value={editGroupForm.goal}
                onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                  setEditGroupForm((form) => ({ ...form, goal: event.target.value }))
                }
              />
              <select
                className="select select-bordered select-sm w-full"
                value={editGroupForm.kind}
                onChange={(event: ChangeEvent<HTMLSelectElement>) =>
                  setEditGroupForm((form) => ({
                    ...form,
                    kind: event.target.value as UpdateWorkGroupInput["kind"],
                  }))
                }
              >
                <option value="persistent">{t("persistent")}</option>
                <option value="ephemeral">{t("ephemeral")}</option>
              </select>
              <div className="modal-action">
                <button className="btn btn-ghost btn-sm" type="button" onClick={() => setEditModalOpen(false)}>
                  {t("cancel")}
                </button>
                <button className="btn btn-primary btn-sm" type="submit">
                  {t("save")}
                </button>
              </div>
            </form>
          </div>
        </dialog>
      )}

      {memberModalOpen && currentGroup && (
        <dialog className="modal modal-open" onClick={() => setMemberModalOpen(false)}>
          <div className="modal-box" onClick={(event) => event.stopPropagation()}>
            <button
              className="btn btn-sm btn-circle btn-ghost absolute right-3 top-3"
              onClick={() => setMemberModalOpen(false)}
            >
              ✕
            </button>
            <h3 className="mb-4 text-lg font-bold">
              {t("members")} - {currentGroup.name}
            </h3>
            <div className="space-y-2">
              {agents.map((agent) => {
                const isMember = currentGroup.memberAgentIds.includes(agent.id);
                return (
                  <div
                    key={agent.id}
                    className="flex items-center justify-between rounded-box bg-base-200 px-4 py-2.5"
                  >
                    <div className="flex items-center gap-2">
                      <div className="grid h-8 w-8 place-items-center rounded-btn bg-primary/10 text-xs font-bold text-primary">
                        {agent.avatar}
                      </div>
                      <div>
                        <div className="text-sm font-medium">{agent.name}</div>
                        <div className="text-xs text-base-content/50">{agent.role}</div>
                      </div>
                    </div>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={isMember}
                      onChange={() => void handleToggleMember(agent)}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        </dialog>
      )}
    </div>
  );
}
