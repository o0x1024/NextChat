import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTranslation } from "react-i18next";
import type { AgentProfile, SkillPack, ToolManifest, ToolRun } from "../../types";

interface ToolWorkspaceProps {
  tools: ToolManifest[];
  skills: SkillPack[];
  toolRuns: ToolRun[];
  agents: AgentProfile[];
  onInstallSkillFromGithub: (source: string, skillPath?: string) => Promise<void>;
  onInstallSkillFromLocal: (sourcePath: string) => Promise<void>;
  onUpdateInstalledSkill: (skillId: string, name?: string, promptTemplate?: string) => Promise<void>;
  onSetInstalledSkillEnabled: (skillId: string, enabled: boolean) => Promise<void>;
  onDeleteInstalledSkill: (skillId: string) => Promise<void>;
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
  onUpdateInstalledSkill,
  onSetInstalledSkillEnabled,
  onDeleteInstalledSkill,
}: ToolWorkspaceProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<ToolTab>("builtin");
  const [activeCategory, setActiveCategory] = useState<ToolCategory>("all");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);

  const [installModalOpen, setInstallModalOpen] = useState(false);
  const [installSourceType, setInstallSourceType] = useState<InstallSourceType>("local");
  const [installLocalPath, setInstallLocalPath] = useState("");
  const [installGithubRepo, setInstallGithubRepo] = useState("");
  const [installGithubPath, setInstallGithubPath] = useState("");

  const [editingSkill, setEditingSkill] = useState<SkillPack | null>(null);
  const [editName, setEditName] = useState("");
  const [editPrompt, setEditPrompt] = useState("");

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
      setError(value instanceof Error ? value.message : "Request failed");
    } finally {
      setWorking(false);
    }
  }, []);

  const installFromDroppedPath = useCallback(
    async (path: string) => {
      await runAction(
        () => onInstallSkillFromLocal(path),
        `Installed skill from dropped path: ${path}`,
      );
    },
    [onInstallSkillFromLocal, runAction],
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

  function openEditModal(skill: SkillPack) {
    setEditingSkill(skill);
    setEditName(skill.name);
    setEditPrompt(skill.promptTemplate);
  }

  async function submitInstall() {
    if (installSourceType === "local") {
      const path = installLocalPath.trim();
      if (!path) {
        setError("Local path is required.");
        return;
      }
      await runAction(
        () => onInstallSkillFromLocal(path),
        `Installed local skill: ${path}`,
      );
    } else {
      const repo = installGithubRepo.trim();
      if (!repo) {
        setError("GitHub repository is required.");
        return;
      }
      await runAction(
        () => onInstallSkillFromGithub(repo, installGithubPath.trim() || undefined),
        `Installed GitHub skill: ${repo}`,
      );
    }
    setInstallModalOpen(false);
    setInstallLocalPath("");
    setInstallGithubRepo("");
    setInstallGithubPath("");
  }

  async function submitSkillEdit() {
    if (!editingSkill) return;
    await runAction(
      () => onUpdateInstalledSkill(editingSkill.id, editName.trim(), editPrompt.trim()),
      `Updated skill: ${editingSkill.name}`,
    );
    setEditingSkill(null);
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
            <button
              className="btn btn-primary btn-sm gap-1"
              onClick={() => setInstallModalOpen(true)}
              disabled={working}
            >
              <i className="fas fa-plus" /> Install Skill
            </button>
          ) : null}
        </div>
      </div>

      <div className="px-6 pt-4">
        <div className="tabs tabs-boxed w-fit">
          <a
            className={`tab gap-2 ${activeTab === "builtin" ? "tab-active" : ""}`}
            onClick={() => setActiveTab("builtin")}
          >
            <i className="fas fa-tools" /> {t("builtinTools")}
          </a>
          <a
            className={`tab gap-2 ${activeTab === "mcp" ? "tab-active" : ""}`}
            onClick={() => setActiveTab("mcp")}
          >
            <i className="fas fa-plug" /> MCP {t("tools")}
          </a>
          <a
            className={`tab gap-2 ${activeTab === "skills" ? "tab-active" : ""}`}
            onClick={() => setActiveTab("skills")}
          >
            <i className="fas fa-bullseye" /> Skills
          </a>
        </div>
      </div>

      {(status || error) && (
        <div className="px-6 pt-3">
          {status ? (
            <div className="alert alert-success py-2 text-sm">
              <i className="fas fa-check-circle" />
              <span>{status}</span>
            </div>
          ) : null}
          {error ? (
            <div className={`alert alert-error py-2 text-sm ${status ? "mt-2" : ""}`}>
              <i className="fas fa-exclamation-circle" />
              <span>{error}</span>
            </div>
          ) : null}
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
            <button
              className={`badge cursor-pointer py-3 transition-all ${
                activeCategory === "all" ? "badge-primary" : "badge-ghost"
              }`}
              onClick={() => setActiveCategory("all")}
            >
              {t("all")} ({tools.length})
            </button>
            {categoryList.map((category) => (
              <button
                key={category}
                className={`badge cursor-pointer gap-1.5 py-3 transition-all ${
                  activeCategory === category ? "badge-primary" : "badge-ghost"
                }`}
                onClick={() => setActiveCategory(category)}
              >
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
                  <span className="text-xs text-base-content/50">{categoryTools.length}</span>
                </div>

                <table className="table w-full">
                  <thead>
                    <tr>
                      <th>{t("name")}</th>
                      <th className="hidden xl:table-cell">{t("description")}</th>
                      <th className="w-24 text-right">{t("actions")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {categoryTools.map((tool) => {
                      const relatedRuns = toolRuns.filter((run) => run.toolId === tool.id);
                      return (
                        <tr key={tool.id} className="hover">
                          <td>
                            <div className="flex items-center gap-2">
                              <i className={`${getCategoryIconClass(tool.category)} text-primary`} />
                              <div>
                                <div className="text-sm font-medium">{tool.name}</div>
                                <div className="text-xs text-base-content/50">{tool.id}</div>
                              </div>
                            </div>
                          </td>
                          <td className="hidden xl:table-cell">
                            <span className="line-clamp-2 text-sm text-base-content/70">
                              {tool.description}
                            </span>
                          </td>
                          <td className="text-right">
                            <div className="flex items-center justify-end gap-1">
                              {tool.riskLevel === "high" ? (
                                <span className="badge badge-error badge-xs">High</span>
                              ) : null}
                              {relatedRuns.length > 0 ? (
                                <span className="badge badge-ghost badge-xs">{relatedRuns.length}</span>
                              ) : null}
                            </div>
                          </td>
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
          <div className="mb-3 text-sm text-base-content/60">
            Drop a skill folder anywhere in this Skills page to install.
          </div>

          <div className="overflow-x-auto rounded-box border border-base-content/10">
            <table className="table table-zebra w-full">
              <thead>
                <tr>
                  <th className="w-20">Enabled</th>
                  <th>Name</th>
                  <th>ID</th>
                  <th>Source</th>
                  <th className="w-48 text-right">Actions</th>
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
                            `${event.target.checked ? "Enabled" : "Disabled"} skill: ${skill.name}`,
                          );
                        }}
                        disabled={working}
                      />
                    </td>
                    <td>
                      <div className="font-medium">{skill.name}</div>
                      <div className="text-xs text-base-content/60 line-clamp-1">
                        {skill.promptTemplate}
                      </div>
                    </td>
                    <td className="text-xs">{skill.id}</td>
                    <td>
                      <span className="badge badge-ghost">{skill.source || "local"}</span>
                    </td>
                    <td className="text-right">
                      <div className="flex justify-end gap-2">
                        <button
                          className="btn btn-ghost btn-xs"
                          onClick={() => openEditModal(skill)}
                          disabled={working}
                        >
                          Edit
                        </button>
                        <button
                          className="btn btn-error btn-xs"
                          onClick={() => {
                            void runAction(
                              () => onDeleteInstalledSkill(skill.id),
                              `Deleted skill: ${skill.name}`,
                            );
                          }}
                          disabled={working}
                        >
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
                {installedSkills.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="text-center text-sm text-base-content/60">
                      No installed skills yet.
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {installModalOpen ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
          <div className="card w-full max-w-lg bg-base-100 shadow-xl">
            <div className="card-body gap-3">
              <h3 className="card-title">Install Skill</h3>
              <div className="join">
                <button
                  className={`btn btn-sm join-item ${
                    installSourceType === "local" ? "btn-primary" : "btn-ghost"
                  }`}
                  onClick={() => setInstallSourceType("local")}
                >
                  Local Path
                </button>
                <button
                  className={`btn btn-sm join-item ${
                    installSourceType === "github" ? "btn-primary" : "btn-ghost"
                  }`}
                  onClick={() => setInstallSourceType("github")}
                >
                  GitHub
                </button>
              </div>
              {installSourceType === "local" ? (
                <label className="form-control">
                  <span className="label-text text-xs">Path</span>
                  <input
                    className="input input-bordered input-sm mt-1"
                    placeholder="/path/to/skill or /path/to/SKILL.md"
                    value={installLocalPath}
                    onChange={(event) => setInstallLocalPath(event.target.value)}
                  />
                </label>
              ) : (
                <>
                  <label className="form-control">
                    <span className="label-text text-xs">Repository</span>
                    <input
                      className="input input-bordered input-sm mt-1"
                      placeholder="owner/repo or https://github.com/owner/repo"
                      value={installGithubRepo}
                      onChange={(event) => setInstallGithubRepo(event.target.value)}
                    />
                  </label>
                  <label className="form-control">
                    <span className="label-text text-xs">Skill path (optional)</span>
                    <input
                      className="input input-bordered input-sm mt-1"
                      placeholder="skills/my-skill"
                      value={installGithubPath}
                      onChange={(event) => setInstallGithubPath(event.target.value)}
                    />
                  </label>
                </>
              )}
              <div className="card-actions justify-end">
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => setInstallModalOpen(false)}
                  disabled={working}
                >
                  Cancel
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => {
                    void submitInstall();
                  }}
                  disabled={working}
                >
                  Install
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {editingSkill ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
          <div className="card w-full max-w-lg bg-base-100 shadow-xl">
            <div className="card-body gap-3">
              <h3 className="card-title">Edit Skill</h3>
              <label className="form-control">
                <span className="label-text text-xs">Name</span>
                <input
                  className="input input-bordered input-sm mt-1"
                  value={editName}
                  onChange={(event) => setEditName(event.target.value)}
                />
              </label>
              <label className="form-control">
                <span className="label-text text-xs">Description</span>
                <textarea
                  className="textarea textarea-bordered textarea-sm mt-1"
                  rows={4}
                  value={editPrompt}
                  onChange={(event) => setEditPrompt(event.target.value)}
                />
              </label>
              <div className="card-actions justify-end">
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => setEditingSkill(null)}
                  disabled={working}
                >
                  Cancel
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => {
                    void submitSkillEdit();
                  }}
                  disabled={working}
                >
                  Save
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}
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
