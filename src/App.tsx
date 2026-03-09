import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentManagement } from "./components/dashboard/AgentManagement";
import { ChatManagement } from "./components/dashboard/ChatManagement";
import { SettingsWorkspace, type SettingsTabType } from "./components/dashboard/SettingsWorkspace";
import { Sidebar, type ViewType } from "./components/dashboard/Sidebar";
import { ToolWorkspace } from "./components/dashboard/ToolWorkspace";
import { applyThemeToDocument, daisyThemes, type ThemeMode } from "./constants/themes";
import { useAppStore } from "./store/appStore";
import { usePreferencesStore, type Language } from "./store/preferencesStore";
import type { CreateAgentInput, CreateWorkGroupInput, UpdateAgentInput } from "./types";

function App() {
  const { t, i18n } = useTranslation();
  const store = useAppStore();
  const init = useAppStore((state) => state.init);
  const theme = usePreferencesStore((state) => state.theme);
  const setTheme = usePreferencesStore((state) => state.setTheme);
  const language = usePreferencesStore((state) => state.language);
  const setLanguage = usePreferencesStore((state) => state.setLanguage);

  const {
    loading,
    error,
    workGroups,
    agents,
    messages,
    taskCards,
    leases,
    claimBids,
    toolRuns,
    tools,
    skills,
    selectedWorkGroupId,
    backstageOpen,
    settings,
  } = store;

  const [activeView, setActiveView] = useState<ViewType>("agents");
  const [settingsTab, setSettingsTab] = useState<SettingsTabType>("ai");

  const fontSize = usePreferencesStore((state) => state.fontSize);
  const componentSpacing = usePreferencesStore((state) => state.componentSpacing);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    applyThemeToDocument(theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = language;
    void i18n.changeLanguage(language);
  }, [i18n, language]);

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
    <div data-theme={theme} className="flex h-screen w-screen bg-base-200 text-base-content antialiased overflow-hidden">
      <style>{`
        :root {
          font-size: ${fontSize}px !important;
          --component-gap: ${componentSpacing}px !important;
        }
        /* Apply dynamic gap to specific layout elements */
        .flex { gap: var(--component-gap, inherit); }
        .grid { gap: var(--component-gap, inherit); }
        .space-y-4 > * + * { margin-top: var(--component-gap, 1rem) !important; }
        .space-x-4 > * + * { margin-left: var(--component-gap, 1rem) !important; }
      `}</style>

      {/* Sidebar */}
      <Sidebar activeView={activeView} onViewChange={setActiveView} />

      {/* Main Content Area */}
      <div className="flex flex-1 flex-col min-w-0 bg-base-200">
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
        <main className="flex-1 min-h-0 relative p-4 lg:p-6 overflow-hidden">
          {error && (
            <div className="toast toast-top toast-center z-[101] mt-16 animate-in slide-in-from-top duration-300">
              <div className="alert alert-error shadow-2xl rounded-2xl border border-white/10">
                <i className="fas fa-exclamation-circle" />
                <span className="font-semibold text-sm">{error}</span>
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
            {activeView === "agents" && (
              <AgentManagement
                agents={agents}
                skills={skills}
                tools={tools}
                settings={settings}
                onCreateAgent={async (input: CreateAgentInput) => { await store.createAgent(input); }}
                onUpdateAgent={async (input: UpdateAgentInput) => { await store.updateAgent(input); }}
                onDeleteAgent={async (id: string) => { await store.deleteAgent(id); }}
              />
            )}

            {activeView === "chats" && (
              <ChatManagement
                workGroups={workGroups}
                agents={agents}
                messages={messages}
                taskCards={taskCards}
                leases={leases}
                claimBids={claimBids}
                toolRuns={toolRuns}
                tools={tools}
                selectedWorkGroupId={selectedWorkGroupId}
                language={language as Language}
                backstageOpen={backstageOpen}
                onSelectWorkGroup={(id: string) => store.setSelectedWorkGroupId(id)}
                onCreateGroup={async (input: CreateWorkGroupInput) => { await store.createGroup(input); }}
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
                onToggleBackstage={() => store.toggleBackstage()}
              />
            )}

            {activeView === "tools" && (
              <ToolWorkspace tools={tools} toolRuns={toolRuns} agents={agents} />
            )}

            {activeView === "settings" && (
              <SettingsWorkspace
                settingsTab={settingsTab}
                onSettingsTabChange={setSettingsTab}
              />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

export default App;
