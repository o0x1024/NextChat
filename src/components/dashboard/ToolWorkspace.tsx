import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTranslation } from "react-i18next";
import type {
  AgentProfile,
  SkillDetail,
  SkillPack,
  ToolManifest,
  ToolRun,
  UpdateSkillDetailInput,
} from "../../types";

interface ToolWorkspaceProps {
  tools: ToolManifest[];
  skills: SkillPack[];
  toolRuns: ToolRun[];
  agents: AgentProfile[];
  onInstallSkillFromGithub: (source: string, skillPath?: string) => Promise<number>;
  onInstallSkillFromLocal: (sourcePath: string) => Promise<number>;
  onUpdateInstalledSkill: (skillId: string, name?: string, promptTemplate?: string) => Promise<void>;
  onSetInstalledSkillEnabled: (skillId: string, enabled: boolean) => Promise<void>;
  onDeleteInstalledSkill: (skillId: string) => Promise<void>;
  onGetInstalledSkillDetail: (skillId: string) => Promise<SkillDetail>;
  onUpdateSkillDetail: (input: UpdateSkillDetailInput) => Promise<SkillDetail>;
  onReadInstalledSkillFile: (skillId: string, relativePath: string) => Promise<string>;
  onUpsertInstalledSkillFile: (skillId: string, relativePath: string, content: string) => Promise<void>;
  onDeleteInstalledSkillFile: (skillId: string, relativePath: string) => Promise<void>;
}

type ToolCategory = "all" | string;
type ToolTab = "builtin" | "mcp" | "skills";
type InstallSourceType = "local" | "github";

export function ToolWorkspace({
  tools,
  skills,
  toolRuns,
  agents,
  onInstallSkillFromGithub,
  onInstallSkillFromLocal,
  onSetInstalledSkillEnabled,
  onDeleteInstalledSkill,
  onGetInstalledSkillDetail,
  onUpdateSkillDetail,
  onReadInstalledSkillFile,
  onUpsertInstalledSkillFile,
  onDeleteInstalledSkillFile,
}: ToolWorkspaceProps) {
  const { t } = useTranslation();
  const tx = (key: string, defaultValue: string) => t(key, { defaultValue });

  const [activeTab, setActiveTab] = useState<ToolTab>("builtin");
  const [activeCategory, setActiveCategory] = useState<ToolCategory>("all");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);

  const [installModalOpen, setInstallModalOpen] = useState(false);
  const [installSourceType, setInstallSourceType] = useState<InstallSourceType>("local");
  const [installLocalPath, setInstallLocalPath] = useState("");
  const [installGithubRepo, setInstallGithubRepo] = useState("");

  const [detailModalOpen, setDetailModalOpen] = useState(false);
  const [detail, setDetail] = useState<SkillDetail | null>(null);
  const [selectedFilePath, setSelectedFilePath] = useState("");
  const [selectedFileContent, setSelectedFileContent] = useState("");
  const [newFilePath, setNewFilePath] = useState("");

  const categories = useMemo(() => {
    const map = new Map<string, ToolManifest[]>();
    for (const tool of tools) {
      const category = tool.category || "other";
      if (!map.has(category)) map.set(category, []);
      map.get(category)!.push(tool);
    }
    return map;
  }, [tools]);

  const categoryList = useMemo(() => Array.from(categories.keys()), [categories]);

  const filteredTools = useMemo(() => {
    if (activeCategory === "all") return tools;
    return tools.filter((tool) => (tool.category || "other") === activeCategory);
  }, [tools, activeCategory]);

  const groupedDisplay = useMemo(() => {
    const map = new Map<string, ToolManifest[]>();
    for (const tool of filteredTools) {
      const category = tool.category || "other";
      if (!map.has(category)) map.set(category, []);
      map.get(category)!.push(tool);
    }
    return map;
  }, [filteredTools]);

  const installedSkills = useMemo(
    () => skills.filter((skill) => skill.editable || skill.id.startsWith("skill.local.")),
    [skills],
  );

  const runAction = useCallback(async (action: () => Promise<void>, okMessage?: string) => {
    setWorking(true);
    setError("");
    try {
      await action();
      if (okMessage) setStatus(okMessage);
    } catch (value) {
      setError(value instanceof Error ? value.message : tx("skills.requestFailed", "Request failed"));
    } finally {
      setWorking(false);
    }
  }, [tx]);

  const installFromDroppedPath = useCallback(
    async (path: string) => {
      await runAction(async () => {
        const count = await onInstallSkillFromLocal(path);
        setStatus(tx("skills.installedFromDrop", "Detected and installed {{count}} skill(s) from dropped path.").replace("{{count}}", String(count)));
      });
    },
    [onInstallSkillFromLocal, runAction, tx],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebviewWindow()
      .onDragDropEvent(async (event) => {
        if (disposed || activeTab !== "skills") return;
        if (event.payload.type !== "drop") return;
        for (const path of event.payload.paths) {
          await installFromDroppedPath(path);
        }
      })
      .then((dispose) => {
        unlisten = dispose;
      });
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [activeTab, installFromDroppedPath]);

  async function openSkillDetail(skillId: string) {
    await runAction(async () => {
      const next = await onGetInstalledSkillDetail(skillId);
      setDetail(next);
      setSelectedFilePath("");
      setSelectedFileContent("");
      setNewFilePath("");
      setDetailModalOpen(true);
    });
  }

  async function submitInstall() {
    if (installSourceType === "local") {
      const path = installLocalPath.trim();
      if (!path) {
        setError(tx("skills.localPathRequired", "Local path is required."));
        return;
      }
      await runAction(async () => {
        const count = await onInstallSkillFromLocal(path);
        setStatus(tx("skills.installedFromLocal", "Detected and installed {{count}} skill(s) from local source.").replace("{{count}}", String(count)));
      });
    } else {
      const repo = installGithubRepo.trim();
      if (!repo) {
        setError(tx("skills.githubRepoRequired", "GitHub repository is required."));
        return;
      }
      await runAction(async () => {
        const count = await onInstallSkillFromGithub(repo, undefined);
        setStatus(tx("skills.installedFromGithub", "Detected and installed {{count}} skill(s) from GitHub.").replace("{{count}}", String(count)));
      });
    }
    setInstallModalOpen(false);
    setInstallLocalPath("");
    setInstallGithubRepo("");
  }

  async function saveSkillDetail() {
    if (!detail) return;
    await runAction(async () => {
      const input: UpdateSkillDetailInput = {
        skillId: detail.skillId,
        enabled: detail.enabled,
        name: detail.name,
        description: detail.description,
        argumentHint: detail.argumentHint,
        userInvocable: detail.userInvocable,
        disableModelInvocation: detail.disableModelInvocation,
        allowedTools: detail.allowedTools,
        model: detail.model,
        context: detail.context,
        agent: detail.agent,
        hooksJson: detail.hooksJson,
        summary: detail.summary,
        content: detail.content,
      };
      const next = await onUpdateSkillDetail(input);
      setDetail(next);
      setStatus(tx("skills.savedDetail", "Saved skill detail: {{name}}.").replace("{{name}}", next.name));
    });
  }

  async function loadSkillFile(path: string) {
    if (!detail) return;
    setSelectedFilePath(path);
    await runAction(async () => {
      const content = await onReadInstalledSkillFile(detail.skillId, path);
      setSelectedFileContent(content);
    });
  }

  async function saveCurrentFile() {
    if (!detail) return;
    const target = (selectedFilePath || newFilePath).trim();
    if (!target) {
      setError(tx("skills.filePathRequired", "File path is required."));
      return;
    }
    await runAction(async () => {
      await onUpsertInstalledSkillFile(detail.skillId, target, selectedFileContent);
      const next = await onGetInstalledSkillDetail(detail.skillId);
      setDetail(next);
      setSelectedFilePath(target);
      setStatus(tx("skills.savedFile", "Saved file: {{path}}.").replace("{{path}}", target));
    });
  }

  async function deleteCurrentFile(path: string) {
    if (!detail) return;
    await runAction(async () => {
      await onDeleteInstalledSkillFile(detail.skillId, path);
      const next = await onGetInstalledSkillDetail(detail.skillId);
      setDetail(next);
      if (selectedFilePath === path) {
        setSelectedFilePath("");
        setSelectedFileContent("");
      }
      setStatus(tx("skills.deletedFile", "Deleted file: {{path}}.").replace("{{path}}", path));
    });
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-base-content/10 px-6 py-4">
        <div>
          <h1 className="text-xl font-bold">{t("toolManagement")}</h1>
          <p className="mt-0.5 text-sm text-base-content/60">{t("toolManagementDesc")}</p>
        </div>
        <div className="flex items-center gap-2">
          <span className="badge badge-ghost">{agents.length} agents</span>
          {activeTab === "skills" ? (
            <button className="btn btn-primary btn-sm gap-1" onClick={() => setInstallModalOpen(true)} disabled={working}>
              <i className="fas fa-plus" /> {tx("skills.installSkill", "Install Skill")}
            </button>
          ) : null}
        </div>
      </div>

      <div className="px-6 pt-4">
        <div className="tabs tabs-boxed w-fit">
          <a className={`tab gap-2 ${activeTab === "builtin" ? "tab-active" : ""}`} onClick={() => setActiveTab("builtin")}>
            <i className="fas fa-tools" /> {t("builtinTools")}
          </a>
          <a className={`tab gap-2 ${activeTab === "mcp" ? "tab-active" : ""}`} onClick={() => setActiveTab("mcp")}>
            <i className="fas fa-plug" /> MCP {t("tools")}
          </a>
          <a className={`tab gap-2 ${activeTab === "skills" ? "tab-active" : ""}`} onClick={() => setActiveTab("skills")}>
            <i className="fas fa-bullseye" /> {t("skills")}
          </a>
        </div>
      </div>

      {(status || error) && (
        <div className="px-6 pt-3">
          {status ? <div className="alert alert-success py-2 text-sm">{status}</div> : null}
          {error ? <div className={`alert alert-error py-2 text-sm ${status ? "mt-2" : ""}`}>{error}</div> : null}
        </div>
      )}

      {activeTab !== "skills" ? (
        <>
          <div className="px-6 pt-3">
            <div className="alert alert-info py-2">
              <i className="fas fa-info-circle" />
              <span className="text-sm">{t("builtinToolsHint")}</span>
            </div>
          </div>
          <div className="flex flex-wrap gap-2 px-6 pt-3">
            <button className={`badge cursor-pointer py-3 transition-all ${activeCategory === "all" ? "badge-primary" : "badge-ghost"}`} onClick={() => setActiveCategory("all")}>
              {t("all")} ({tools.length})
            </button>
            {categoryList.map((category) => (
              <button key={category} className={`badge cursor-pointer gap-1.5 py-3 transition-all ${activeCategory === category ? "badge-primary" : "badge-ghost"}`} onClick={() => setActiveCategory(category)}>
                <i className={getCategoryIconClass(category)} />
                {category} ({categories.get(category)?.length ?? 0})
              </button>
            ))}
          </div>
          <div className="flex-1 overflow-auto px-6 py-4">
            {Array.from(groupedDisplay.entries()).map(([category, categoryTools]) => (
              <div key={category} className="mb-6">
                <div className="mb-3 flex items-center gap-2">
                  <i className={`${getCategoryIconClass(category)} text-primary`} />
                  <span className="text-sm font-semibold">{category}</span>
                </div>
                <table className="table w-full">
                  <thead>
                    <tr><th>{t("name")}</th><th className="hidden xl:table-cell">{t("description")}</th><th className="w-24 text-right">{t("actions")}</th></tr>
                  </thead>
                  <tbody>
                    {categoryTools.map((tool) => {
                      const relatedRuns = toolRuns.filter((run) => run.toolId === tool.id);
                      return (
                        <tr key={tool.id}>
                          <td><div className="text-sm font-medium">{tool.name}</div><div className="text-xs text-base-content/50">{tool.id}</div></td>
                          <td className="hidden xl:table-cell text-sm text-base-content/70">{tool.description}</td>
                          <td className="text-right">{relatedRuns.length > 0 ? <span className="badge badge-ghost badge-xs">{relatedRuns.length}</span> : null}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            ))}
          </div>
        </>
      ) : (
        <div className="flex-1 overflow-auto px-6 py-4">
          <div className="mb-3 text-sm text-base-content/60">{tx("skills.dropHint", "Drop a skill folder anywhere in this Skills page to install.")}</div>
          <div className="overflow-x-auto rounded-box border border-base-content/10">
            <table className="table table-zebra w-full">
              <thead>
                <tr>
                  <th className="w-20">{tx("skills.enabled", "Enabled")}</th>
                  <th>{tx("skills.name", "Name")}</th>
                  <th>ID</th>
                  <th>{tx("skills.source", "Source")}</th>
                  <th className="w-56 text-right">{tx("skills.actions", "Actions")}</th>
                </tr>
              </thead>
              <tbody>
                {installedSkills.map((skill) => (
                  <tr key={skill.id}>
                    <td>
                      <input
                        type="checkbox"
                        className="toggle toggle-primary toggle-sm"
                        checked={skill.enabled}
                        onChange={(event) => {
                          void runAction(
                            () => onSetInstalledSkillEnabled(skill.id, event.target.checked),
                            tx(event.target.checked ? "skills.enabledSkill" : "skills.disabledSkill", `${event.target.checked ? "Enabled" : "Disabled"} skill: {{name}}`).replace("{{name}}", skill.name),
                          );
                        }}
                        disabled={working}
                      />
                    </td>
                    <td><div className="font-medium">{skill.name}</div><div className="text-xs text-base-content/60 line-clamp-1">{skill.promptTemplate}</div></td>
                    <td className="text-xs">{skill.id}</td>
                    <td><span className="badge badge-ghost">{skill.source || "local"}</span></td>
                    <td className="text-right">
                      <div className="flex justify-end gap-2">
                        <button className="btn btn-ghost btn-xs" onClick={() => { void openSkillDetail(skill.id); }} disabled={working}>{tx("skills.edit", "Edit")}</button>
                        <button className="btn btn-error btn-xs" onClick={() => { void runAction(() => onDeleteInstalledSkill(skill.id), tx("skills.deletedSkill", "Deleted skill: {{name}}.").replace("{{name}}", skill.name)); }} disabled={working}>{tx("skills.delete", "Delete")}</button>
                      </div>
                    </td>
                  </tr>
                ))}
                {installedSkills.length === 0 ? <tr><td colSpan={5} className="text-center text-sm text-base-content/60">{tx("skills.noInstalled", "No installed skills yet.")}</td></tr> : null}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {installModalOpen ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
          <div className="card w-full max-w-lg bg-base-100 shadow-xl">
            <div className="card-body gap-3">
              <h3 className="card-title">{tx("skills.installSkill", "Install Skill")}</h3>
              <div className="join">
                <button className={`btn btn-sm join-item ${installSourceType === "local" ? "btn-primary" : "btn-ghost"}`} onClick={() => setInstallSourceType("local")}>{tx("skills.localPath", "Local Path")}</button>
                <button className={`btn btn-sm join-item ${installSourceType === "github" ? "btn-primary" : "btn-ghost"}`} onClick={() => setInstallSourceType("github")}>GitHub</button>
              </div>
              {installSourceType === "local" ? (
                <input className="input input-bordered input-sm" placeholder={tx("skills.localPathPlaceholder", "/path/to/folder")} value={installLocalPath} onChange={(event) => setInstallLocalPath(event.target.value)} />
              ) : (
                <input className="input input-bordered input-sm" placeholder={tx("skills.githubPlaceholder", "owner/repo or https://github.com/owner/repo")} value={installGithubRepo} onChange={(event) => setInstallGithubRepo(event.target.value)} />
              )}
              <div className="card-actions justify-end">
                <button className="btn btn-ghost btn-sm" onClick={() => setInstallModalOpen(false)} disabled={working}>{tx("cancel", "Cancel")}</button>
                <button className="btn btn-primary btn-sm" onClick={() => { void submitInstall(); }} disabled={working}>{tx("install", "Install")}</button>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {detailModalOpen && detail ? (
        <div className="fixed inset-0 z-50 bg-black/40 p-4">
          <div className="mx-auto flex h-full max-w-7xl flex-col rounded-box bg-base-100">
            <div className="flex items-center justify-between border-b border-base-content/10 px-5 py-3">
              <h3 className="text-lg font-semibold">{tx("skills.editSkill", "Edit Skill")}: {detail.name}</h3>
              <div className="flex gap-2">
                <button className="btn btn-ghost btn-sm" onClick={() => setDetailModalOpen(false)}>{tx("close", "Close")}</button>
                <button className="btn btn-primary btn-sm" onClick={() => { void saveSkillDetail(); }} disabled={working}>{tx("skills.saveSkill", "Save Skill")}</button>
              </div>
            </div>
            <div className="grid min-h-0 flex-1 gap-4 overflow-hidden p-4 xl:grid-cols-[1.1fr_0.9fr]">
              <div className="space-y-4 overflow-auto pr-1">
                <div className="grid gap-3 md:grid-cols-2">
                  <FormField label={tx("skills.skillName", "Skill Name")}><input className="input input-bordered input-sm w-full" value={detail.name} onChange={(e) => setDetail({ ...detail, name: e.target.value })} /></FormField>
                  <FormField label={tx("skills.skillDescription", "Skill Description")}><input className="input input-bordered input-sm w-full" value={detail.description} onChange={(e) => setDetail({ ...detail, description: e.target.value })} /></FormField>
                  <FormField label={tx("skills.argumentHint", "Argument Hint")} className="md:col-span-2"><input className="input input-bordered input-sm w-full" value={detail.argumentHint ?? ""} onChange={(e) => setDetail({ ...detail, argumentHint: e.target.value })} /></FormField>
                </div>

                <div className="grid gap-3 md:grid-cols-2 rounded-box bg-base-200 p-3">
                  <label className="flex items-center gap-2 text-sm"><input type="checkbox" className="checkbox checkbox-sm" checked={detail.userInvocable} onChange={(e) => setDetail({ ...detail, userInvocable: e.target.checked })} />{tx("skills.userInvocable", "Allow user invocation")}</label>
                  <label className="flex items-center gap-2 text-sm"><input type="checkbox" className="checkbox checkbox-sm" checked={detail.disableModelInvocation} onChange={(e) => setDetail({ ...detail, disableModelInvocation: e.target.checked })} />{tx("skills.disableModelInvocation", "Disable model invocation")}</label>
                </div>

                <div className="grid gap-3 md:grid-cols-3">
                  <FormField label={tx("model", "Model")}><input className="input input-bordered input-sm w-full" value={detail.model ?? ""} onChange={(e) => setDetail({ ...detail, model: e.target.value })} /></FormField>
                  <FormField label={tx("skills.context", "Context")}><input className="input input-bordered input-sm w-full" value={detail.context ?? ""} onChange={(e) => setDetail({ ...detail, context: e.target.value })} /></FormField>
                  <FormField label={tx("agent", "Agent")}><input className="input input-bordered input-sm w-full" value={detail.agent ?? ""} onChange={(e) => setDetail({ ...detail, agent: e.target.value })} /></FormField>
                </div>

                <FormField label={tx("skills.allowedTools", "Allowed Tools (control who invokes)")}>
                  <input className="input input-bordered input-sm w-full" value={detail.allowedTools ?? ""} onChange={(e) => setDetail({ ...detail, allowedTools: e.target.value })} placeholder={tx("skills.allowedToolsPlaceholder", "user,agent:model-name")} />
                </FormField>

                <FormField label={tx("skills.skillContent", "Skill Content (SKILL.md body)")}>
                  <textarea className="textarea textarea-bordered h-56 w-full" value={detail.content} onChange={(e) => setDetail({ ...detail, content: e.target.value })} />
                </FormField>

                <div className="grid gap-3 md:grid-cols-2">
                  <FormField label={tx("skills.hooksJson", "Hooks (JSON)")}><textarea className="textarea textarea-bordered h-28 w-full" value={detail.hooksJson ?? ""} onChange={(e) => setDetail({ ...detail, hooksJson: e.target.value })} /></FormField>
                  <FormField label={tx("skills.shortSummary", "Short Summary")}><textarea className="textarea textarea-bordered h-28 w-full" value={detail.summary ?? ""} onChange={(e) => setDetail({ ...detail, summary: e.target.value })} /></FormField>
                </div>
              </div>

              <div className="grid min-h-0 gap-3 overflow-hidden">
                <div className="card card-border min-h-0 bg-base-100">
                  <div className="card-body min-h-0 p-3">
                    <div className="mb-2 flex items-center justify-between">
                      <span className="font-medium">{tx("skills.skillFiles", "Skill Files")}</span>
                      <span className="badge badge-ghost">{detail.files.length}</span>
                    </div>
                    <div className="grid min-h-0 gap-2">
                      <div className="max-h-48 overflow-auto rounded-box border border-base-content/10">
                        <ul className="menu menu-sm p-1">
                          {detail.files.map((file) => (
                            <li key={file.path}>
                              <a onClick={() => { void loadSkillFile(file.path); }} className={selectedFilePath === file.path ? "active" : ""}>
                                <span className="truncate">{file.path}</span>
                                <span className="text-xs text-base-content/50">{Math.max(1, Math.round(file.size / 1024))}KB</span>
                              </a>
                            </li>
                          ))}
                        </ul>
                      </div>
                      <input className="input input-bordered input-sm w-full" placeholder={tx("skills.newFilePlaceholder", "new file path: docs/patterns.md")} value={newFilePath} onChange={(e) => setNewFilePath(e.target.value)} />
                      <textarea className="textarea textarea-bordered h-52 w-full" value={selectedFileContent} onChange={(e) => setSelectedFileContent(e.target.value)} placeholder={selectedFilePath ? tx("skills.editingFile", "Editing {{path}}").replace("{{path}}", selectedFilePath) : tx("skills.selectOrNewFile", "Select file or set new file path")} />
                      <div className="flex justify-end gap-2">
                        {selectedFilePath ? <button className="btn btn-error btn-xs" onClick={() => { void deleteCurrentFile(selectedFilePath); }} disabled={working}>{tx("skills.deleteFile", "Delete File")}</button> : null}
                        <button className="btn btn-primary btn-xs" onClick={() => { void saveCurrentFile(); }} disabled={working}>{tx("skills.saveFile", "Save File")}</button>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function FormField({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <div className={className}>
      <div className="mb-1 text-xs font-medium">{label}</div>
      {children}
    </div>
  );
}

function getCategoryIconClass(category: string): string {
  const icons: Record<string, string> = {
    filesystem: "fas fa-folder-open",
    workspace: "fas fa-magnifying-glass",
    system: "fas fa-terminal",
    network: "fas fa-globe",
    browser: "fas fa-window-maximize",
    content: "fas fa-pen",
    coordination: "fas fa-clipboard-list",
    skills: "fas fa-bullseye",
    other: "fas fa-cube",
  };
  return icons[category?.toLowerCase()] ?? "fas fa-cube";
}
