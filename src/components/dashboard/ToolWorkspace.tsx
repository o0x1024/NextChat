import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AgentProfile, ToolManifest, ToolRun } from "../../types";

interface ToolWorkspaceProps {
  tools: ToolManifest[];
  toolRuns: ToolRun[];
  agents: AgentProfile[];
}

type ToolCategory = "all" | string;
type ToolTab = "builtin" | "mcp" | "skills";

export function ToolWorkspace({ tools, toolRuns, agents }: ToolWorkspaceProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<ToolTab>("builtin");
  const [activeCategory, setActiveCategory] = useState<ToolCategory>("all");

  // Group tools by category
  const categories = useMemo(() => {
    const map = new Map<string, ToolManifest[]>();
    for (const tool of tools) {
      const cat = tool.category || "other";
      if (!map.has(cat)) map.set(cat, []);
      map.get(cat)!.push(tool);
    }
    return map;
  }, [tools]);

  const categoryList = useMemo(() => Array.from(categories.keys()), [categories]);

  const filteredTools = useMemo(() => {
    if (activeCategory === "all") return tools;
    return tools.filter((t) => (t.category || "other") === activeCategory);
  }, [tools, activeCategory]);

  // Group filtered tools by category for display
  const groupedDisplay = useMemo(() => {
    const map = new Map<string, ToolManifest[]>();
    for (const tool of filteredTools) {
      const cat = tool.category || "other";
      if (!map.has(cat)) map.set(cat, []);
      map.get(cat)!.push(tool);
    }
    return map;
  }, [filteredTools]);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-base-content/10 px-6 py-4">
        <div>
          <h1 className="text-xl font-bold">{t("toolManagement")}</h1>
          <p className="text-sm text-base-content/60 mt-0.5">{t("toolManagementDesc")}</p>
        </div>
        <div className="flex gap-2">
          <button className="btn btn-primary btn-sm gap-1">
            <i className="fas fa-plus" /> {t("addTool")}
          </button>
          <button className="btn btn-ghost btn-sm gap-1">
            <i className="fas fa-sync-alt" /> {t("refreshModels")}
          </button>
        </div>
      </div>

      {/* Tabs: Built-in / MCP / Skills */}
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

      {/* Info banner */}
      <div className="px-6 pt-3">
        <div className="alert alert-info py-2">
          <i className="fas fa-info-circle" />
          <span className="text-sm">{t("builtinToolsHint")}</span>
        </div>
      </div>

      {/* Category Filter */}
      <div className="flex gap-2 px-6 pt-3 flex-wrap">
        <button
          className={`badge py-3 cursor-pointer transition-all ${activeCategory === "all" ? "badge-primary" : "badge-ghost"
            }`}
          onClick={() => setActiveCategory("all")}
        >
          {t("all")} ({tools.length})
        </button>
        {categoryList.map((cat) => (
          <button
            key={cat}
            className={`badge py-3 cursor-pointer transition-all gap-1.5 ${activeCategory === cat ? "badge-primary" : "badge-ghost"
              }`}
            onClick={() => setActiveCategory(cat)}
          >
            <i className={getCategoryIconClass(cat)} />
            {cat} ({categories.get(cat)?.length ?? 0})
          </button>
        ))}
      </div>

      {/* Tool List Table */}
      <div className="flex-1 overflow-auto px-6 py-4">
        {Array.from(groupedDisplay.entries()).map(([category, categoryTools]) => (
          <div key={category} className="mb-6">
            <div className="flex items-center gap-2 mb-3">
              <i className={`${getCategoryIconClass(category)} text-primary`} />
              <span className="text-sm font-semibold">{category}</span>
              <span className="text-xs text-base-content/50">{categoryTools.length}</span>
            </div>

            <table className="table w-full">
              <thead>
                <tr>
                  <th className="w-16">{t("enabled")}</th>
                  <th>{t("name")}</th>
                  <th className="hidden xl:table-cell">{t("description")}</th>
                  <th className="w-20">{t("version")}</th>
                  <th className="w-24 text-right">{t("actions")}</th>
                </tr>
              </thead>
              <tbody>
                {categoryTools.map((tool) => {
                  const relatedRuns = toolRuns.filter((r) => r.toolId === tool.id);
                  return (
                    <tr key={tool.id} className="hover">
                      <td>
                        <input
                          type="checkbox"
                          className="toggle toggle-primary toggle-sm"
                          defaultChecked={true}
                        />
                      </td>
                      <td>
                        <div className="flex items-center gap-2">
                          <i className={`${getCategoryIconClass(tool.category)} text-primary`} />
                          <div>
                            <div className="font-medium text-sm">{tool.name}</div>
                            <div className="text-xs text-base-content/50">{tool.id}</div>
                          </div>
                        </div>
                      </td>
                      <td className="hidden xl:table-cell">
                        <span className="text-sm text-base-content/70 line-clamp-2">
                          {tool.description}
                        </span>
                      </td>
                      <td>
                        <span className="text-xs text-base-content/50">v1.0.0</span>
                      </td>
                      <td className="text-right">
                        <div className="flex items-center justify-end gap-1">
                          {tool.riskLevel === "high" && (
                            <span className="badge badge-error badge-xs">
                              <i className="fas fa-exclamation" />
                            </span>
                          )}
                          {relatedRuns.length > 0 && (
                            <span className="badge badge-ghost badge-xs">{relatedRuns.length}</span>
                          )}
                          <button className="btn btn-ghost btn-xs btn-circle">
                            <i className="fas fa-play text-xs" />
                          </button>
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
    </div>
  );
}

function getCategoryIconClass(category: string): string {
  const icons: Record<string, string> = {
    network: "fas fa-globe",
    system: "fas fa-terminal",
    ai: "fas fa-brain",
    file: "fas fa-folder-open",
    browser: "fas fa-window-maximize",
    planning: "fas fa-clipboard-list",
    other: "fas fa-cube",
  };
  return icons[category?.toLowerCase()] ?? "fas fa-cube";
}
