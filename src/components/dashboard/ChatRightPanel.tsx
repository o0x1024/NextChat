import type { PointerEvent as ReactPointerEvent } from "react";
import type { Language } from "../../store/preferencesStore";
import type {
  AgentProfile,
  AuditEvent,
  ChatStreamTrack,
  ClaimBid,
  ConversationMessage,
  Lease,
  OwnerBlockerResolution,
  TaskBlockerRecord,
  TaskCard,
  ToolManifest,
  ToolRun,
  WorkflowCheckpointRecord,
  WorkGroup,
} from "../../types";
import { AgentExecutionDetailsPanel } from "./AgentExecutionDetailsPanel";
import { ChatMembersPanel } from "./ChatMembersPanel";
import { ChatRunningPanel } from "./ChatRunningPanel";
import type { PanelTarget } from "./chatManagementConfig";

interface ChatRightPanelProps {
  sidePanelOpen: boolean;
  sidePanelMode: "execution" | "running" | "members" | null;
  resizingRightPanel: boolean;
  language: Language;
  focusAgentId: string | null;
  currentMembers: AgentProfile[];
  agents: AgentProfile[];
  currentGroupTasks: TaskCard[];
  currentGroupMessages: ConversationMessage[];
  currentGroupStreamTracks: ChatStreamTrack[];
  toolRuns: ToolRun[];
  auditEvents: AuditEvent[];
  tools: ToolManifest[];
  activeTasks: TaskCard[];
  currentLeases: Lease[];
  currentApprovals: ToolRun[];
  currentTaskBlockers: TaskBlockerRecord[];
  claimBids: ClaimBid[];
  workflowCheckpoints: WorkflowCheckpointRecord[];
  highlightedTaskId: string | null;
  highlightedBlockerId: string | null;
  panelTarget: PanelTarget | null;
  currentGroup?: WorkGroup;
  availableAgentsForCurrentGroup: AgentProfile[];
  onRightPanelResizeStart: (event: ReactPointerEvent<HTMLDivElement>) => void;
  onRightPanelResizeMove: (event: ReactPointerEvent<HTMLDivElement>) => void;
  onRightPanelResizeEnd: (event: ReactPointerEvent<HTMLDivElement>) => void;
  onFocusAgentIdChange: (agentId: string | null) => void;
  onJumpToTask: (taskId?: string) => void;
  onJumpToBlocker: (blockerId: string) => void;
  onJumpToNarrative: (target: { taskId?: string; blockerId?: string }) => void;
  onTaskBoardRef: (node: HTMLDivElement | null) => void;
  onApprovalsRef: (node: HTMLDivElement | null) => void;
  onSetTaskCardRef: (taskId: string, node: HTMLDivElement | null) => void;
  onSetBlockerCardRef: (blockerId: string, node: HTMLDivElement | null) => void;
  onApproveRun: (toolRunId: string, approved: boolean) => Promise<void>;
  onResolveBlocker: (blockerId: string, resolution: OwnerBlockerResolution) => Promise<void>;
  onCancelTask: (taskCardId: string) => Promise<void>;
  onAddAgent: (agentId: string) => Promise<void>;
  onRemoveAgent: (agent: AgentProfile) => Promise<void>;
}

export function ChatRightPanel({
  sidePanelOpen,
  sidePanelMode,
  resizingRightPanel,
  language,
  focusAgentId,
  currentMembers,
  agents,
  currentGroupTasks,
  currentGroupMessages,
  currentGroupStreamTracks,
  toolRuns,
  auditEvents,
  tools,
  activeTasks,
  currentLeases,
  currentApprovals,
  currentTaskBlockers,
  claimBids,
  workflowCheckpoints,
  highlightedTaskId,
  highlightedBlockerId,
  panelTarget,
  currentGroup,
  availableAgentsForCurrentGroup,
  onRightPanelResizeStart,
  onRightPanelResizeMove,
  onRightPanelResizeEnd,
  onFocusAgentIdChange,
  onJumpToTask,
  onJumpToBlocker,
  onJumpToNarrative,
  onTaskBoardRef,
  onApprovalsRef,
  onSetTaskCardRef,
  onSetBlockerCardRef,
  onApproveRun,
  onResolveBlocker,
  onCancelTask,
  onAddAgent,
  onRemoveAgent,
}: ChatRightPanelProps) {
  if (!sidePanelOpen) {
    return null;
  }

  return (
    <>
      <div
        className={`-ml-2 hidden w-2 shrink-0 cursor-col-resize border-l border-transparent bg-transparent transition-colors hover:border-primary/20 hover:bg-primary/20 xl:block ${
          resizingRightPanel ? "border-primary/30 bg-primary/30" : ""
        }`}
        onPointerDown={onRightPanelResizeStart}
        onPointerMove={onRightPanelResizeMove}
        onPointerUp={onRightPanelResizeEnd}
        onPointerCancel={onRightPanelResizeEnd}
      />
      <aside className="flex min-h-0 w-full shrink-0 flex-col gap-3 overflow-y-auto xl:w-[var(--chat-right-panel-width,360px)]">
        {sidePanelMode === "execution" && (
          <AgentExecutionDetailsPanel
            language={language}
            focusAgentId={focusAgentId}
            onFocusAgentIdChange={onFocusAgentIdChange}
            onJumpToTask={(taskId) => onJumpToTask(taskId)}
            onJumpToBlocker={onJumpToBlocker}
            onJumpToNarrative={onJumpToNarrative}
            currentMembers={currentMembers}
            agents={agents}
            currentGroupTasks={currentGroupTasks}
            groupMessages={currentGroupMessages}
            streamTracks={currentGroupStreamTracks}
            toolRuns={toolRuns}
            auditEvents={auditEvents}
            tools={tools}
          />
        )}

        {sidePanelMode === "running" && (
          <ChatRunningPanel
            language={language}
            activeTasks={activeTasks}
            currentLeases={currentLeases}
            currentApprovals={currentApprovals}
            currentGroupTasks={currentGroupTasks}
            taskBlockers={currentTaskBlockers}
            claimBids={claimBids}
            agents={agents}
            tools={tools}
            workflowCheckpoints={workflowCheckpoints}
            highlightedTaskId={highlightedTaskId}
            highlightedBlockerId={highlightedBlockerId}
            targetBlockerId={panelTarget?.section === "blockers" ? panelTarget.blockerId : null}
            onTaskBoardRef={onTaskBoardRef}
            onApprovalsRef={onApprovalsRef}
            onSetTaskCardRef={onSetTaskCardRef}
            onSetBlockerCardRef={onSetBlockerCardRef}
            onJumpToTaskBoard={onJumpToTask}
            onApproveRun={onApproveRun}
            onResolveBlocker={onResolveBlocker}
          />
        )}

        {sidePanelMode === "members" && currentGroup && (
          <ChatMembersPanel
            currentGroup={currentGroup}
            currentMembers={currentMembers}
            availableAgents={availableAgentsForCurrentGroup}
            currentGroupTasks={currentGroupTasks}
            onCancelTask={onCancelTask}
            onAddAgent={onAddAgent}
            onRemoveAgent={onRemoveAgent}
          />
        )}
      </aside>
    </>
  );
}
