import {
  type FormEvent,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { pickDirectory } from "../../lib/tauri";
import type {
  AgentProfile,
  CreateWorkGroupInput,
  UpdateWorkGroupInput,
  WorkGroup,
} from "../../types";
import { ChatComposer } from "./ChatComposer";
import { ChatMessageList } from "./ChatMessageList";
import { ChatRightPanel } from "./ChatRightPanel";
import {
  type ChatManagementProps,
  type PanelTarget,
  emptyEditGroupForm,
  emptyGroupForm,
} from "./chatManagementConfig";
import {
  activeMentionDraft,
  insertMention,
  mentionCandidates,
  validateMentions,
} from "./mentions";
import { isBuiltinGroupOwner } from "./agentManagementUtils";
import { WorkGroupDialogs } from "./WorkGroupDialogs";
import { roleAccent } from "./ui";
import { findLatestNarrativeMessageId } from "./narrativeTargeting";

type SidePanelMode = "execution" | "running" | "members" | null;

export function ChatManagement({
  workGroups,
  agents,
  messages,
  chatStreamTracks,
  taskCards,
  pendingUserQuestions,
  taskBlockers,
  workflowCheckpoints,
  leases,
  claimBids,
  toolRuns,
  auditEvents,
  tools,
  settings,
  selectedWorkGroupId,
  language,
  onSelectWorkGroup,
  onCreateGroup,
  onDeleteGroup,
  onClearGroupHistory,
  onUpdateGroup,
  onSendMessage,
  onAddAgent,
  onRemoveAgent,
  onApproveRun,
  onCancelTask,
  onResolveBlocker,
}: ChatManagementProps) {
  const { t } = useTranslation();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(256);
  const [resizingSidebar, setResizingSidebar] = useState(false);
  const [sidePanelMode, setSidePanelMode] = useState<SidePanelMode>("execution");
  const [rightPanelWidth, setRightPanelWidth] = useState(360);
  const [resizingRightPanel, setResizingRightPanel] = useState(false);
  const [composerValue, setComposerValue] = useState("");
  const [sendingMessage, setSendingMessage] = useState(false);
  const [mentionIndex, setMentionIndex] = useState(0);
  const [mentionError, setMentionError] = useState<string | null>(null);
  const [stoppingExecution, setStoppingExecution] = useState(false);
  const [focusAgentId, setFocusAgentId] = useState<string | null>(null);
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [editModalOpen, setEditModalOpen] = useState(false);
  const [directoryPickerError, setDirectoryPickerError] = useState<string | null>(null);
  const [deleteTargetGroup, setDeleteTargetGroup] = useState<WorkGroup | null>(null);
  const [deletingGroup, setDeletingGroup] = useState(false);
  const [clearHistoryTargetGroup, setClearHistoryTargetGroup] = useState<WorkGroup | null>(null);
  const [clearingHistory, setClearingHistory] = useState(false);
  const [highlightedTaskId, setHighlightedTaskId] = useState<string | null>(null);
  const [highlightedBlockerId, setHighlightedBlockerId] = useState<string | null>(null);
  const [targetMessageId, setTargetMessageId] = useState<string | null>(null);
  const [panelTarget, setPanelTarget] = useState<PanelTarget | null>(null);
  const [groupForm, setGroupForm] = useState<CreateWorkGroupInput>(emptyGroupForm);
  const [editGroupForm, setEditGroupForm] = useState<UpdateWorkGroupInput>(emptyEditGroupForm);
  const [defaultWorkingDirectory, setDefaultWorkingDirectory] = useState(".");
  const rootRef = useRef<HTMLDivElement | null>(null);
  const sidebarResizeRef = useRef<{
    pointerId: number | null;
    startX: number;
    startWidth: number;
    nextWidth: number;
    rafId: number | null;
  }>({
    pointerId: null,
    startX: 0,
    startWidth: 256,
    nextWidth: 256,
    rafId: null,
  });
  const rightPanelResizeRef = useRef<{
    pointerId: number | null;
    startX: number;
    startWidth: number;
    nextWidth: number;
    rafId: number | null;
  }>({
    pointerId: null,
    startX: 0,
    startWidth: 360,
    nextWidth: 360,
    rafId: null,
  });

  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const taskBoardRef = useRef<HTMLDivElement | null>(null);
  const approvalsRef = useRef<HTMLDivElement | null>(null);
  const taskCardRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const blockerCardRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const currentGroup = useMemo(() => workGroups.find((group) => group.id === selectedWorkGroupId) ?? workGroups[0], [selectedWorkGroupId, workGroups]);

  const currentGroupMessages = useMemo(() => !currentGroup ? [] : messages.filter((message) => message.workGroupId === currentGroup.id), [currentGroup, messages]);
  const currentGroupStreamTracks = useMemo(() => !currentGroup ? [] : chatStreamTracks.filter((track) => track.workGroupId === currentGroup.id), [chatStreamTracks, currentGroup]);
  const currentMessages = useMemo(() => !currentGroup ? [] : currentGroupMessages.filter((message) => message.visibility === "main"), [currentGroup, currentGroupMessages]);
  const currentMembers = useMemo(() => {
    if (!currentGroup) return [];
    const memberIds = new Set(currentGroup.memberAgentIds);
    return agents.filter((agent) => memberIds.has(agent.id));
  }, [agents, currentGroup]);
  const currentMemberIds = useMemo(() => new Set(currentMembers.map((member) => member.id)), [currentMembers]);
  const availableAgentsForCurrentGroup = useMemo(() => agents.filter((agent) => !currentMemberIds.has(agent.id)), [agents, currentMemberIds]);

  const mentionDraft = useMemo(() => {
    const caret = textareaRef.current?.selectionStart ?? composerValue.length;
    return activeMentionDraft(composerValue, caret);
  }, [composerValue]);
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

  const stoppableTasks = useMemo(
    () =>
      currentGroupTasks.filter(
        (task) => !["completed", "cancelled", "needs_review"].includes(task.status),
      ),
    [currentGroupTasks],
  );

  const activeTasks = useMemo(
    () => currentGroupTasks.filter((task) => !["completed", "cancelled"].includes(task.status)),
    [currentGroupTasks],
  );

  const activeTaskIds = useMemo(() => new Set(activeTasks.map((task) => task.id)), [activeTasks]);
  const currentTaskIds = useMemo(() => new Set(currentGroupTasks.map((task) => task.id)), [currentGroupTasks]);
  const currentTaskTitles = useMemo(() => new Map(currentGroupTasks.map((task) => [task.id, task.title])), [currentGroupTasks]);
  const currentTaskAssignees = useMemo(() => new Map(currentGroupTasks.filter((task) => Boolean(task.assignedAgentId)).map((task) => [task.id, task.assignedAgentId as string])), [currentGroupTasks]);
  const currentMemberNames = useMemo(() => new Map(currentMembers.map((member) => [member.id, member.name])), [currentMembers]);
  const currentPendingQuestion = useMemo(
    () =>
      !currentGroup
        ? null
        : pendingUserQuestions.find((question) => question.workGroupId === currentGroup.id) ?? null,
    [currentGroup, pendingUserQuestions],
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
  const currentTaskBlockers = useMemo(
    () =>
      taskBlockers
        .filter((blocker) => currentTaskIds.has(blocker.taskId))
        .sort((left, right) => right.createdAt.localeCompare(left.createdAt)),
    [currentTaskIds, taskBlockers],
  );
  const sidePanelOpen = sidePanelMode !== null;

  async function submitMessage(content: string) {
    if (!currentGroup || sendingMessage) return;

    const trimmedContent = content.trim();
    if (!trimmedContent) return;

    const validation = validateMentions(trimmedContent, currentMembers);
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

    setSendingMessage(true);
    setComposerValue("");
    setMentionError(null);

    try {
      await onSendMessage(currentGroup.id, trimmedContent);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setComposerValue(trimmedContent);
      setMentionError(message);
      throw error;
    }
  }

  async function handleSend(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      await submitMessage(composerValue);
    } finally {
      setSendingMessage(false);
    }
  }

  async function handleAnswerPendingQuestion(answer: string) {
    try {
      await submitMessage(answer);
    } finally {
      setSendingMessage(false);
    }
  }
  async function handleStopExecution() {
    if (stoppingExecution || stoppableTasks.length === 0) {
      return;
    }
    setStoppingExecution(true);
    try {
      const results = await Promise.allSettled(
        stoppableTasks.map((task) => onCancelTask(task.id)),
      );
      const failed = results.find(
        (result): result is PromiseRejectedResult => result.status === "rejected",
      );
      if (failed) {
        throw failed.reason;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      window.alert(message);
    } finally {
      setStoppingExecution(false);
    }
  }

  async function handleCreateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      const sanitizedMemberAgentIds = (groupForm.memberAgentIds ?? []).filter((agentId) => {
        const agent = agents.find((candidate) => candidate.id === agentId);
        return !agent || !isBuiltinGroupOwner(agent);
      });
      await onCreateGroup({ ...groupForm, memberAgentIds: sanitizedMemberAgentIds });
      setGroupForm((form) => ({ ...emptyGroupForm, workingDirectory: defaultWorkingDirectory }));
      setDirectoryPickerError(null);
      setCreateModalOpen(false);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDirectoryPickerError(message);
    }
  }

  async function handleUpdateGroup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!currentGroup) return;
    try {
      await onUpdateGroup(editGroupForm);
      setDirectoryPickerError(null);
      setEditModalOpen(false);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDirectoryPickerError(message);
    }
  }

  async function handlePickWorkingDirectory(mode: "create" | "edit") {
    const currentPath =
      mode === "create" ? groupForm.workingDirectory : editGroupForm.workingDirectory;
    try {
      const selected = await pickDirectory(currentPath || undefined);
      if (!selected) return;
      if (mode === "create") {
        setGroupForm((form) => ({ ...form, workingDirectory: selected }));
      } else {
        setEditGroupForm((form) => ({ ...form, workingDirectory: selected }));
      }
      setDirectoryPickerError(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setDirectoryPickerError(message);
    }
  }

  async function handleAddMember(agentId: string) {
    if (!currentGroup) return;
    await onAddAgent(currentGroup.id, agentId);
  }

  async function handleRemoveMember(agent: AgentProfile) {
    if (!currentGroup) return;
    await onRemoveAgent(currentGroup.id, agent.id);
  }

  function handleToggleCreateMember(agentId: string) {
    const agent = agents.find((candidate) => candidate.id === agentId);
    if (agent && isBuiltinGroupOwner(agent)) {
      return;
    }
    setGroupForm((form) => {
      const memberIds = form.memberAgentIds ?? [];
      const alreadySelected = memberIds.includes(agentId);
      return {
        ...form,
        memberAgentIds: alreadySelected
          ? memberIds.filter((id) => id !== agentId)
          : [...memberIds, agentId],
      };
    });
  }

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
    setSidePanelMode("running");
    setPanelTarget({ section: "approvals" });
  }
  function jumpToTaskBoard(taskId?: string) {
    setSidePanelMode("running");
    setPanelTarget({ section: "tasks", taskId });
    setHighlightedTaskId(taskId ?? null);
  }
  function jumpToBlocker(blockerId: string) {
    setSidePanelMode("running");
    setPanelTarget({ section: "blockers", blockerId });
    setHighlightedBlockerId(blockerId);
  }

  function jumpToExecutionAgent(agentId: string) { setSidePanelMode("execution"); setFocusAgentId(agentId); }

  function jumpToNarrative(target: { taskId?: string; blockerId?: string }) {
    const messageId = findLatestNarrativeMessageId(currentMessages, target);
    if (messageId) setTargetMessageId(messageId);
  }
  function toggleSidePanel(mode: Exclude<SidePanelMode, null>) {
    setSidePanelMode((current) => (current === mode ? null : mode));
  }
  function openCreateGroupModal() {
    setGroupForm({ ...emptyGroupForm, workingDirectory: defaultWorkingDirectory });
    setDirectoryPickerError(null);
    setCreateModalOpen(true);
  }
  function openEditGroupModal(group: WorkGroup) {
    setEditGroupForm({
      id: group.id,
      name: group.name,
      goal: group.goal,
      workingDirectory: group.workingDirectory,
      kind: group.kind,
      defaultVisibility: group.defaultVisibility,
      autoArchive: group.autoArchive,
    });
    setDirectoryPickerError(null);
    setEditModalOpen(true);
  }
  function handleDeleteGroup(group: WorkGroup) {
    setDeleteTargetGroup(group);
  }
  function handleClearHistory(group: WorkGroup) {
    setClearHistoryTargetGroup(group);
  }

  async function confirmDeleteGroup() {
    if (!deleteTargetGroup || deletingGroup) {
      return;
    }
    setDeletingGroup(true);
    try {
      await onDeleteGroup(deleteTargetGroup.id);
      setDeleteTargetGroup(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      window.alert(message);
    } finally {
      setDeletingGroup(false);
    }
  }

  async function confirmClearHistory() {
    if (!clearHistoryTargetGroup || clearingHistory) {
      return;
    }
    setClearingHistory(true);
    try {
      await onClearGroupHistory(clearHistoryTargetGroup.id);
      setClearHistoryTargetGroup(null);
      setHighlightedTaskId(null);
      setPanelTarget(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      window.alert(message);
    } finally {
      setClearingHistory(false);
    }
  }

  function clampSidebarWidth(width: number) {
    const minWidth = 220;
    const rootWidth = rootRef.current?.getBoundingClientRect().width ?? 1024;
    const maxWidth = Math.max(360, Math.floor(rootWidth * 0.7));
    return Math.min(maxWidth, Math.max(minWidth, width));
  }

  function applySidebarWidth(width: number) {
    rootRef.current?.style.setProperty("--chat-sidebar-width", `${width}px`);
  }

  function handleSidebarResizeStart(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const normalizedWidth = clampSidebarWidth(sidebarWidth);
    sidebarResizeRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: normalizedWidth,
      nextWidth: normalizedWidth,
      rafId: null,
    };
    applySidebarWidth(normalizedWidth);
    setResizingSidebar(true);
  }

  function handleSidebarResizeMove(event: ReactPointerEvent<HTMLDivElement>) {
    const state = sidebarResizeRef.current;
    if (!resizingSidebar || state.pointerId !== event.pointerId) {
      return;
    }
    const candidate = clampSidebarWidth(state.startWidth + (event.clientX - state.startX));
    state.nextWidth = candidate;
    if (state.rafId !== null) {
      return;
    }
    state.rafId = window.requestAnimationFrame(() => {
      applySidebarWidth(sidebarResizeRef.current.nextWidth);
      sidebarResizeRef.current.rafId = null;
    });
  }

  function handleSidebarResizeEnd(event: ReactPointerEvent<HTMLDivElement>) {
    const state = sidebarResizeRef.current;
    if (state.pointerId !== event.pointerId) {
      return;
    }
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (state.rafId !== null) {
      window.cancelAnimationFrame(state.rafId);
      state.rafId = null;
    }
    const finalWidth = clampSidebarWidth(state.nextWidth);
    applySidebarWidth(finalWidth);
    setSidebarWidth(finalWidth);
    setResizingSidebar(false);
    sidebarResizeRef.current.pointerId = null;
  }

  function clampRightPanelWidth(width: number) {
    const minWidth = 300;
    const rootWidth = rootRef.current?.getBoundingClientRect().width ?? 1280;
    const maxWidth = Math.max(420, Math.floor(rootWidth * 0.55));
    return Math.min(maxWidth, Math.max(minWidth, width));
  }

  function applyRightPanelWidth(width: number) {
    rootRef.current?.style.setProperty("--chat-right-panel-width", `${width}px`);
  }

  function handleRightPanelResizeStart(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const normalizedWidth = clampRightPanelWidth(rightPanelWidth);
    rightPanelResizeRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: normalizedWidth,
      nextWidth: normalizedWidth,
      rafId: null,
    };
    applyRightPanelWidth(normalizedWidth);
    setResizingRightPanel(true);
  }

  function handleRightPanelResizeMove(event: ReactPointerEvent<HTMLDivElement>) {
    const state = rightPanelResizeRef.current;
    if (!resizingRightPanel || state.pointerId !== event.pointerId) {
      return;
    }
    const candidate = clampRightPanelWidth(state.startWidth - (event.clientX - state.startX));
    state.nextWidth = candidate;
    if (state.rafId !== null) {
      return;
    }
    state.rafId = window.requestAnimationFrame(() => {
      applyRightPanelWidth(rightPanelResizeRef.current.nextWidth);
      rightPanelResizeRef.current.rafId = null;
    });
  }

  function handleRightPanelResizeEnd(event: ReactPointerEvent<HTMLDivElement>) {
    const state = rightPanelResizeRef.current;
    if (state.pointerId !== event.pointerId) {
      return;
    }
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (state.rafId !== null) {
      window.cancelAnimationFrame(state.rafId);
      state.rafId = null;
    }
    const finalWidth = clampRightPanelWidth(state.nextWidth);
    applyRightPanelWidth(finalWidth);
    setRightPanelWidth(finalWidth);
    setResizingRightPanel(false);
    rightPanelResizeRef.current.pointerId = null;
  }

  useEffect(() => {
    const nextDefault = currentGroup?.workingDirectory?.trim() || ".";
    setDefaultWorkingDirectory(nextDefault);
    setGroupForm((form) => {
      if (form.workingDirectory !== "." && form.workingDirectory.trim() !== "") {
        return form;
      }
      return { ...form, workingDirectory: nextDefault };
    });
  }, [currentGroup?.id, currentGroup?.workingDirectory]);

  useEffect(() => {
    setMentionIndex(0);
  }, [mentionDraft?.start, mentionDraft?.query]);

  useEffect(() => {
    if (sidePanelMode !== "running" || !panelTarget) {
      return;
    }

    const target =
      panelTarget.section === "approvals"
        ? approvalsRef.current
        : panelTarget.section === "blockers"
          ? blockerCardRefs.current[panelTarget.blockerId]
        : panelTarget.taskId
          ? taskCardRefs.current[panelTarget.taskId]
          : taskBoardRef.current;

    if (!target) {
      return;
    }

    target.scrollIntoView({ behavior: "smooth", block: "nearest" });
    const clearTimer = window.setTimeout(() => setPanelTarget(null), 500);
    return () => window.clearTimeout(clearTimer);
  }, [panelTarget, sidePanelMode, activeTasks.length, currentApprovals.length]);

  useEffect(() => {
    if (!highlightedTaskId) {
      return;
    }
    const clearTimer = window.setTimeout(() => setHighlightedTaskId(null), 1800);
    return () => window.clearTimeout(clearTimer);
  }, [highlightedTaskId]);

  useEffect(() => {
    if (!highlightedBlockerId) {
      return;
    }
    const clearTimer = window.setTimeout(() => setHighlightedBlockerId(null), 1800);
    return () => window.clearTimeout(clearTimer);
  }, [highlightedBlockerId]);

  useEffect(() => {
    if (!targetMessageId) return;
    const clearTimer = window.setTimeout(() => setTargetMessageId(null), 1800);
    return () => window.clearTimeout(clearTimer);
  }, [targetMessageId]);

  useEffect(() => {
    applySidebarWidth(clampSidebarWidth(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    applyRightPanelWidth(clampRightPanelWidth(rightPanelWidth));
  }, [rightPanelWidth]);

  useEffect(() => {
    document.body.style.cursor = resizingSidebar || resizingRightPanel ? "col-resize" : "";
    return () => {
      document.body.style.cursor = "";
    };
  }, [resizingRightPanel, resizingSidebar]);

  return (
    <div ref={rootRef} className="flex h-full gap-0">
      {sidebarOpen && (
        <>
          <div
            className={`flex shrink-0 flex-col border-r border-base-content/10 ${
              resizingSidebar ? "" : "transition-[width] duration-150"
            }`}
            style={{ width: "var(--chat-sidebar-width, 256px)" }}
          >
            <div className="flex items-center justify-between border-b border-base-content/10 px-4 py-3">
              <h2 className="text-sm font-bold">{t("chatManagement")}</h2>
              <div className="flex items-center gap-1">
                <button className="btn btn-primary btn-xs" onClick={openCreateGroupModal}>
                  + {t("create")}
                </button>
                <button className="btn btn-ghost btn-xs" onClick={() => setSidebarOpen(false)}>
                  <i className="fas fa-indent" />
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-x-hidden overflow-y-auto">
              <ul className="menu menu-sm w-full gap-0.5 p-2">
                {workGroups.map((group) => {
                  const isActive = currentGroup?.id === group.id;
                  return (
                    <li key={group.id} className="w-full overflow-hidden">
                      <div
                        role="button"
                        tabIndex={0}
                        className={`flex w-full items-center gap-2 overflow-hidden rounded-lg py-2.5 transition-colors ${
                          isActive ? "bg-primary text-primary-content" : "hover:bg-base-200"
                        }`}
                        onClick={() => onSelectWorkGroup(group.id)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            onSelectWorkGroup(group.id);
                          }
                        }}
                      >
                        <div
                          className={`grid h-8 w-8 shrink-0 place-items-center rounded-lg text-[10px] font-bold ${
                            isActive
                              ? "border border-primary-content/30 bg-primary-content/10 text-primary-content"
                              : "border border-primary/20 bg-primary/10 text-primary"
                          }`}
                        >
                          {group.name.slice(0, 2).toUpperCase()}
                        </div>
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-sm font-medium">{group.name}</div>
                          <div
                            className={`truncate text-xs ${
                              isActive ? "text-primary-content/75" : "text-base-content/50"
                            }`}
                          >
                            {group.goal}
                          </div>
                        </div>
                        <div className="flex shrink-0 items-center gap-1">
                          <button
                            type="button"
                            className={`btn btn-ghost btn-xs ${
                              isActive ? "text-primary-content hover:bg-primary-content/20" : ""
                            }`}
                            title={t("edit")}
                            onClick={(event) => {
                              event.stopPropagation();
                              openEditGroupModal(group);
                            }}
                          >
                            <i className="fas fa-pen" />
                          </button>
                          <button
                            type="button"
                            className={`btn btn-ghost btn-xs ${
                              isActive
                                ? "text-primary-content hover:bg-primary-content/20"
                                : "text-error"
                            }`}
                            title={t("delete")}
                            onClick={(event) => {
                              event.stopPropagation();
                              handleDeleteGroup(group);
                            }}
                          >
                            <i className="fas fa-trash" />
                          </button>
                        </div>
                        <span
                          className={`badge badge-xs shrink-0 ${
                            isActive
                              ? "border-primary-content/30 bg-primary-content/15 text-primary-content"
                              : "badge-ghost"
                          }`}
                        >
                          {group.kind === "persistent" ? "P" : "E"}
                        </span>
                      </div>
                    </li>
                  );
                })}
                {workGroups.length === 0 && (
                  <li className="py-4 text-center text-sm text-base-content/50">
                    {t("noWorkGroupsYet")}
                  </li>
                )}
              </ul>
            </div>
          </div>

          <div
            className={`-ml-px w-2 shrink-0 cursor-col-resize border-l border-base-content/10 bg-transparent transition-colors hover:bg-primary/20 ${
              resizingSidebar ? "bg-primary/30" : ""
            }`}
            onPointerDown={handleSidebarResizeStart}
            onPointerMove={handleSidebarResizeMove}
            onPointerUp={handleSidebarResizeEnd}
            onPointerCancel={handleSidebarResizeEnd}
          />
        </>
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
                <span
                  className="badge badge-outline badge-sm min-w-0 max-w-[32rem] shrink gap-1"
                  title={currentGroup.workingDirectory}
                >
                  <i className="fas fa-folder-open text-[10px]" />
                  <span className="truncate">{currentGroup.workingDirectory}</span>
                </span>
              </div>

              <div className="flex shrink-0 items-center gap-2">
                <button
                  className={`btn btn-sm ${sidePanelMode === "execution" ? "btn-primary" : "btn-ghost"}`}
                  onClick={() => toggleSidePanel("execution")}
                >
                  {t("executionDetailsPanel")}
                </button>
                <button
                  className={`btn btn-sm ${sidePanelMode === "running" ? "btn-primary" : "btn-ghost"}`}
                  onClick={() => toggleSidePanel("running")}
                >
                  {t("runningPanel")}
                </button>
                <button
                  className={`btn btn-sm ${sidePanelMode === "members" ? "btn-primary" : "btn-ghost"}`}
                  onClick={() => toggleSidePanel("members")}
                >
                  {t("members")} ({currentMembers.length})
                </button>
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

            <div
              className={`flex min-h-0 flex-1 px-3 py-4 ${
                sidePanelOpen ? "flex-col gap-4 xl:flex-row" : "flex-col"
              }`}
            >
              <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                <ChatMessageList
                  currentMessages={currentMessages}
                  streamTracks={currentGroupStreamTracks}
                  currentTaskTitles={currentTaskTitles}
                  currentTaskAssignees={currentTaskAssignees}
                  currentMemberNames={currentMemberNames}
                  activeTaskIds={activeTaskIds}
                  language={language}
                  targetMessageId={targetMessageId}
                  onJumpToTaskBoard={jumpToTaskBoard}
                  onJumpToBlocker={jumpToBlocker}
                  onJumpToExecutionAgent={jumpToExecutionAgent}
                />
                <ChatComposer
                  value={composerValue}
                  sendingMessage={sendingMessage}
                  mentionDraft={mentionDraft}
                  mentionOptions={mentionOptions}
                  mentionIndex={mentionIndex}
                  mentionError={mentionError}
                  pendingQuestion={currentPendingQuestion}
                  pendingQuestionAgentName={currentPendingQuestion ? currentMemberNames.get(currentPendingQuestion.agentId) ?? null : null}
                  currentApprovalsCount={currentApprovals.length}
                  activeTasksCount={activeTasks.length}
                  stoppableTasksCount={stoppableTasks.length}
                  stoppingExecution={stoppingExecution}
                  textareaRef={textareaRef}
                  onSubmit={(event) => void handleSend(event)}
                  onChangeValue={(value) => { setComposerValue(value); setMentionError(null); }}
                  onSetMentionIndex={setMentionIndex}
                  onApplyMention={applyMention}
                  onOpenMentionPicker={openMentionPicker}
                  onAnswerPendingQuestion={(answer) => void handleAnswerPendingQuestion(answer)}
                  onJumpToApprovals={jumpToApprovals}
                  onJumpToTaskBoard={() => jumpToTaskBoard()}
                  onStopExecution={() => void handleStopExecution()}
                  onClearHistory={() => { if (currentGroup) handleClearHistory(currentGroup); }}
                />
              </div>

              <ChatRightPanel
                sidePanelOpen={sidePanelOpen}
                sidePanelMode={sidePanelMode}
                resizingRightPanel={resizingRightPanel}
                language={language}
                focusAgentId={focusAgentId}
                currentMembers={currentMembers}
                agents={agents}
                currentGroupTasks={currentGroupTasks}
                currentGroupMessages={currentGroupMessages}
                currentGroupStreamTracks={currentGroupStreamTracks}
                toolRuns={toolRuns}
                auditEvents={auditEvents}
                tools={tools}
                activeTasks={activeTasks}
                currentLeases={currentLeases}
                currentApprovals={currentApprovals}
                currentTaskBlockers={currentTaskBlockers}
                claimBids={claimBids}
                workflowCheckpoints={workflowCheckpoints}
                highlightedTaskId={highlightedTaskId}
                highlightedBlockerId={highlightedBlockerId}
                panelTarget={panelTarget}
                currentGroup={currentGroup}
                availableAgentsForCurrentGroup={availableAgentsForCurrentGroup}
                onRightPanelResizeStart={handleRightPanelResizeStart}
                onRightPanelResizeMove={handleRightPanelResizeMove}
                onRightPanelResizeEnd={handleRightPanelResizeEnd}
                onFocusAgentIdChange={setFocusAgentId}
                onJumpToTask={jumpToTaskBoard}
                onJumpToBlocker={jumpToBlocker}
                onJumpToNarrative={jumpToNarrative}
                onTaskBoardRef={(node) => { taskBoardRef.current = node; }}
                onApprovalsRef={(node) => { approvalsRef.current = node; }}
                onSetTaskCardRef={(taskId, node) => { taskCardRefs.current[taskId] = node; }}
                onSetBlockerCardRef={(blockerId, node) => { blockerCardRefs.current[blockerId] = node; }}
                onApproveRun={onApproveRun}
                onResolveBlocker={onResolveBlocker}
                onCancelTask={onCancelTask}
                onAddAgent={handleAddMember}
                onRemoveAgent={handleRemoveMember}
              />
            </div>
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center">
            <div className="text-center text-base-content/50"><div className="mb-3 text-4xl">💬</div><div>{t("noWorkGroupSelected")}</div></div>
          </div>
        )}
      </div>

      <WorkGroupDialogs
        createModalOpen={createModalOpen}
        editModalOpen={editModalOpen}
        deleteTargetGroup={deleteTargetGroup}
        deletingGroup={deletingGroup}
        clearHistoryTargetGroup={clearHistoryTargetGroup}
        clearingHistory={clearingHistory}
        currentGroup={currentGroup}
        agents={agents}
        groupForm={groupForm}
        editGroupForm={editGroupForm}
        directoryPickerError={directoryPickerError} onSetCreateModalOpen={setCreateModalOpen}
        onSetEditModalOpen={setEditModalOpen}
        onSetDeleteTargetGroup={setDeleteTargetGroup} onSetClearHistoryTargetGroup={setClearHistoryTargetGroup}
        onSetGroupForm={setGroupForm} onSetEditGroupForm={setEditGroupForm}
        onHandleCreateGroup={handleCreateGroup} onHandleUpdateGroup={handleUpdateGroup}
        onHandlePickWorkingDirectory={handlePickWorkingDirectory}
        onHandleToggleCreateMember={handleToggleCreateMember}
        onConfirmDeleteGroup={confirmDeleteGroup} onConfirmClearHistory={confirmClearHistory}
      />
    </div>
  );
}
