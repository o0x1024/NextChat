import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentManagement } from "./components/dashboard/AgentManagement";
import { ChatManagement } from "./components/dashboard/ChatManagement";
import { SettingsWorkspace, type SettingsTabType } from "./components/dashboard/SettingsWorkspace";
import { Sidebar, type ViewType } from "./components/dashboard/Sidebar";
import { ToolWorkspace } from "./components/dashboard/ToolWorkspace";
import { applyThemeToDocument } from "./constants/themes";
import { syncUiScaleBaseVariables } from "./lib/uiScale";
import { useAppStore } from "./store/appStore";
import { usePreferencesStore, type Language } from "./store/preferencesStore";
import type {
  CreateAgentInput,
  CreateWorkGroupInput,
  UpdateAgentInput,
  UpdateWorkGroupInput,
} from "./types";

function App() {
  const { t, i18n } = useTranslation();
  const store = useAppStore();
  const init = useAppStore((state) => state.init);
  const theme = usePreferencesStore((state) => state.theme);
  const language = usePreferencesStore((state) => state.language);
  const setLanguage = usePreferencesStore((state) => state.setLanguage);

  const {
    loading,
    error,
    workGroups,
    agents,
    messages,
    chatStreamTracks,
    taskCards,
    taskBlockers,
    workflowCheckpoints,
    workflows,
    workflowStages,
    leases,
    claimBids,
    toolRuns,
    auditEvents,
    tools,
    skills,
    selectedWorkGroupId,
    settings,
    toast,
  } = store;

  const [activeView, setActiveView] = useState<ViewType>("agents");
  const [settingsTab, setSettingsTab] = useState<SettingsTabType>("ai");
  const [mountedViews, setMountedViews] = useState<Record<ViewType, boolean>>({
    agents: true,
    chats: false,
    tools: false,
    settings: false,
  });

  const fontSize = usePreferencesStore((state) => state.fontSize);
  const componentScale = usePreferencesStore((state) => state.componentScale);
  const componentSpacing = usePreferencesStore((state) => state.componentSpacing);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    applyThemeToDocument(theme);
  }, [theme]);

  useEffect(() => {
    syncUiScaleBaseVariables(document.documentElement, componentScale);
  }, [theme, componentScale]);

  useEffect(() => {
    document.documentElement.lang = language;
    void i18n.changeLanguage(language);
  }, [i18n, language]);

  useEffect(() => {
    setMountedViews((current) => {
      if (current[activeView]) {
        return current;
      }
      return { ...current, [activeView]: true };
    });
  }, [activeView]);

  const viewIcons: Record<ViewType, string> = {
    agents: "fas fa-robot",
    chats: "fas fa-comments",
    tools: "fas fa-wrench",
    settings: "fas fa-cog",
  };

  const viewTitles: Record<ViewType, string> = {
    agents: "agentManagement",
    chats: "chatManagement",
    tools: "toolManagement",
    settings: "systemSettings",
  };

  return (
    <div
      data-theme={theme}
      className="app-shell flex h-screen w-screen overflow-hidden bg-base-200 text-base-content antialiased"
    >
      <style>{`
        :root {
          font-size: ${fontSize}px !important;
          --component-scale: ${componentScale} !important;
          --component-gap: ${componentSpacing}px !important;
          --radius-selector: calc(var(--ui-radius-selector-base, 0.5rem) * var(--component-scale)) !important;
          --radius-field: calc(var(--ui-radius-field-base, 0.25rem) * var(--component-scale)) !important;
          --radius-box: calc(var(--ui-radius-box-base, 0.5rem) * var(--component-scale)) !important;
          --size-selector: calc(var(--ui-size-selector-base, 0.25rem) * var(--component-scale)) !important;
          --size-field: calc(var(--ui-size-field-base, 0.25rem) * var(--component-scale)) !important;
        }
        /* Apply dynamic gap to specific layout elements */
        .flex { gap: var(--component-gap, inherit); }
        .grid { gap: var(--component-gap, inherit); }
        .space-y-4 > * + * { margin-top: var(--component-gap, 1rem) !important; }
        .space-x-4 > * + * { margin-left: var(--component-gap, 1rem) !important; }
        .app-shell { gap: 0 !important; }
      `}</style>

      {/* Sidebar */}
      <Sidebar activeView={activeView} onViewChange={setActiveView} />

      {/* Main Content Area */}
      <div className="flex min-w-0 flex-1 flex-col bg-base-200">
        {/* Top Navbar */}
        <header className="navbar bg-base-100/40 backdrop-blur-md border-b border-base-content/5 px-6 h-14 shrink-0 z-20">
          <div className="flex-1 gap-3">
            <div className="flex items-center gap-2.5 px-3 py-1.5 rounded-xl bg-base-content/5 border border-base-content/5 text-base-content/70">
              <i className={`${viewIcons[activeView]} text-xs`} />
              <h2 className="text-[10px] font-bold uppercase tracking-[0.2em]">
                {t(viewTitles[activeView])}
              </h2>
            </div>
          </div>
          <div className="flex-none flex items-center gap-1.5 px-2">
            {/* Language Switch */}
            <div className="dropdown dropdown-end w-fit">
              <div tabIndex={0} role="button" className="btn btn-ghost btn-xs h-8 px-2.5 gap-2 hover:bg-base-content/10 rounded-lg font-bold border-none text-[10px] flex items-center">
                <i className="fas fa-language text-sm opacity-50" />
                <span className="hidden sm:inline uppercase tracking-wider">{language}</span>
              </div>
              <ul tabIndex={0} className="dropdown-content menu p-1.5 shadow-2xl bg-base-100 border border-base-content/10 rounded-xl w-40 z-[100] mt-2 animate-in fade-in slide-in-from-top-1 duration-200">
                <li className="menu-title px-3 py-1 text-[9px] font-bold uppercase opacity-30 tracking-widest">{t("languageLabel")}</li>
                <li>
                  <a onClick={() => setLanguage("zh")} className={`flex justify-between rounded-lg py-1.5 px-3 text-xs ${language === "zh" ? "active bg-primary text-primary-content font-bold" : ""}`}>
                    <span>中文 (简体)</span>
                    {language === "zh" && <i className="fas fa-check text-[10px]" />}
                  </a>
                </li>
                <li>
                  <a onClick={() => setLanguage("en")} className={`flex justify-between rounded-lg py-1.5 px-3 text-xs ${language === "en" ? "active bg-primary text-primary-content font-bold" : ""}`}>
                    <span>English (US)</span>
                    {language === "en" && <i className="fas fa-check text-[10px]" />}
                  </a>
                </li>
              </ul>
            </div>
          </div>
        </header>

        {/* Content Body */}
        <main className="relative min-h-0 flex-1 overflow-hidden px-3 pb-4 pt-2 lg:px-4 lg:pb-6 lg:pt-3">
          {error && (
            <div className="toast toast-top toast-center z-[101] mt-16 animate-in slide-in-from-top duration-300">
              <div className="alert alert-error shadow-2xl rounded-2xl border border-white/10">
                <i className="fas fa-exclamation-circle" />
                <span className="font-semibold text-sm">{error}</span>
              </div>
            </div>
          )}

          {toast && (
            <div className="toast toast-top toast-center z-[101] mt-16 animate-in slide-in-from-top duration-300">
              <div className={`alert alert-${toast.type === 'success' ? 'success' : toast.type === 'error' ? 'error' : toast.type === 'warning' ? 'warning' : 'info'} shadow-2xl rounded-2xl border border-white/10`}>
                <i className={`fas ${
                  toast.type === 'success' ? 'fa-check-circle' : 
                  toast.type === 'error' ? 'fa-exclamation-circle' :
                  toast.type === 'warning' ? 'fa-exclamation-triangle' :
                  'fa-info-circle'
                }`} />
                <span className="font-semibold text-sm">{toast.message}</span>
              </div>
            </div>
          )}

          {loading && (
            <div className="absolute inset-0 z-30 flex items-center justify-center bg-base-200/40 backdrop-blur-md">
              <div className="flex flex-col items-center gap-4">
                <span className="loading loading-spinner text-primary loading-lg" />
                <span className="text-xs font-bold uppercase tracking-widest opacity-40 animate-pulse">Initializing Interface...</span>
              </div>
            </div>
          )}

          <div className="h-full overflow-hidden bg-base-100 rounded-[2rem] border border-base-content/5 shadow-2xl shadow-black/10 transition-all duration-500">
            {mountedViews.agents && (
              <section
                className={`h-full ${activeView === "agents" ? "block" : "hidden"}`}
                aria-hidden={activeView !== "agents"}
              >
                <AgentManagement
                  agents={agents}
                  workGroups={workGroups}
                  skills={skills}
                  tools={tools}
                  settings={settings}
                  onCreateAgent={async (input: CreateAgentInput) => { await store.createAgent(input); }}
                  onUpdateAgent={async (input: UpdateAgentInput) => { await store.updateAgent(input); }}
                  onDeleteAgent={async (id: string) => { await store.deleteAgent(id); }}
                  onAddAgentToWorkGroup={async (workGroupId: string, agentId: string) => {
                    await store.addAgent(workGroupId, agentId);
                  }}
                />
              </section>
            )}

            {mountedViews.chats && (
              <section
                className={`h-full ${activeView === "chats" ? "block" : "hidden"}`}
                aria-hidden={activeView !== "chats"}
              >
                <ChatManagement
                  workGroups={workGroups}
                  agents={agents}
                  messages={messages}
                  chatStreamTracks={chatStreamTracks}
                  taskCards={taskCards}
                  pendingUserQuestions={store.pendingUserQuestions}
                  taskBlockers={taskBlockers}
                  workflowCheckpoints={workflowCheckpoints}
                  workflows={workflows}
                  workflowStages={workflowStages}
                  leases={leases}
                  claimBids={claimBids}
                  toolRuns={toolRuns}
                  auditEvents={auditEvents}
                  tools={tools}
                  settings={settings}
                  selectedWorkGroupId={selectedWorkGroupId}
                  language={language as Language}
                  onSelectWorkGroup={(id: string) => store.setSelectedWorkGroupId(id)}
                  onCreateGroup={async (input: CreateWorkGroupInput) => { await store.createGroup(input); }}
                  onDeleteGroup={async (workGroupId: string) => { await store.deleteGroup(workGroupId); }}
                  onClearGroupHistory={async (workGroupId: string) => { await store.clearGroupHistory(workGroupId); }}
                  onUpdateGroup={async (input: UpdateWorkGroupInput) => { await store.updateGroup(input); }}
                  onSendMessage={async (workGroupId: string, content: string) => {
                    await store.sendMessage({ workGroupId, content });
                  }}
                  onAddAgent={async (workGroupId: string, agentId: string) => {
                    await store.addAgent(workGroupId, agentId);
                  }}
                  onRemoveAgent={async (workGroupId: string, agentId: string) => {
                    await store.removeAgent(workGroupId, agentId);
                  }}
                  onApproveRun={async (id: string, ok: boolean) => {
                    await store.approveRun(id, ok);
                  }}
                  onCancelTask={async (taskCardId: string) => {
                    await store.cancelTask(taskCardId);
                  }}
                  onResolveBlocker={async (blockerId, resolution) => {
                    await store.resolveBlocker(blockerId, resolution);
                  }}
                  onCancelWorkflow={async (workflowId) => {
                    await store.cancelWorkflow(workflowId);
                  }}
                  onPauseWorkflow={async (workflowId) => {
                    await store.pauseWorkflow(workflowId);
                  }}
                  onResumeWorkflow={async (workflowId) => {
                    await store.resumeWorkflow(workflowId);
                  }}
                  onSkipStage={async (workflowId, stageId) => {
                    await store.skipWorkflowStage(workflowId, stageId);
                  }}
                  onAddStage={async (input) => {
                    await store.addWorkflowStage(input);
                  }}
                  onUpdateStage={async (input) => {
                    await store.updateWorkflowStage(input);
                  }}
                  onRemoveStage={async (stageId) => {
                    await store.removeWorkflowStage(stageId);
                  }}
                />
              </section>
            )}

            {mountedViews.tools && (
              <section
                className={`h-full ${activeView === "tools" ? "block" : "hidden"}`}
                aria-hidden={activeView !== "tools"}
              >
                <ToolWorkspace
                  tools={tools}
                  skills={skills}
                  toolRuns={toolRuns}
                  agents={agents}
                  onInstallSkillFromGithub={async (source: string, skillPath?: string) => {
                    return store.installSkillFromGithub(source, skillPath);
                  }}
                  onInstallSkillFromLocal={async (sourcePath: string) => {
                    return store.installSkillFromLocal(sourcePath);
                  }}
                  onUpdateInstalledSkill={async (skillId: string, name?: string, promptTemplate?: string) => {
                    await store.updateInstalledSkill(skillId, name, promptTemplate);
                  }}
                  onSetInstalledSkillEnabled={async (skillId: string, enabled: boolean) => {
                    await store.setInstalledSkillEnabled(skillId, enabled);
                  }}
                  onDeleteInstalledSkill={async (skillId: string) => {
                    await store.deleteInstalledSkill(skillId);
                  }}
                  onGetInstalledSkillDetail={async (skillId: string) => {
                    return store.getInstalledSkillDetail(skillId);
                  }}
                  onUpdateSkillDetail={async (input) => {
                    return store.updateSkillDetail(input);
                  }}
                  onReadInstalledSkillFile={async (skillId: string, relativePath: string) => {
                    return store.readInstalledSkillFile(skillId, relativePath);
                  }}
                  onUpsertInstalledSkillFile={async (
                    skillId: string,
                    relativePath: string,
                    content: string,
                  ) => {
                    await store.upsertInstalledSkillFile(skillId, relativePath, content);
                  }}
                  onDeleteInstalledSkillFile={async (skillId: string, relativePath: string) => {
                    await store.deleteInstalledSkillFile(skillId, relativePath);
                  }}
                />
              </section>
            )}

            {mountedViews.settings && (
              <section
                className={`h-full ${activeView === "settings" ? "block" : "hidden"}`}
                aria-hidden={activeView !== "settings"}
              >
                <SettingsWorkspace
                  settingsTab={settingsTab}
                  onSettingsTabChange={setSettingsTab}
                />
              </section>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

export default App;
