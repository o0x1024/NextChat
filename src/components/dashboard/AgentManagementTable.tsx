import { useTranslation } from "react-i18next";
import type { AgentProfile } from "../../types";

type ProviderReason = "disabled" | "missingApiKey" | "unsupported" | "noModels" | null;

type ProviderRowInfo = {
    available: boolean;
    reason: ProviderReason;
};

interface AgentManagementTableProps {
    agents: AgentProfile[];
    selectedAgentIds: string[];
    allVisibleSelected: boolean;
    onToggleSelectAll: () => void;
    onToggleSelect: (agentId: string) => void;
    onEdit: (agent: AgentProfile) => void;
    onDelete: (agent: AgentProfile) => void;
    isBuiltinGroupOwner: (agent: AgentProfile) => boolean;
    providerInfoById: Map<string, ProviderRowInfo>;
    providerReasonHint: (reason: ProviderReason) => string;
    providerRuntimeLabel: (providerId: string) => string;
}

export function AgentManagementTable({
    agents,
    selectedAgentIds,
    allVisibleSelected,
    onToggleSelectAll,
    onToggleSelect,
    onEdit,
    onDelete,
    isBuiltinGroupOwner,
    providerInfoById,
    providerReasonHint,
    providerRuntimeLabel,
}: AgentManagementTableProps) {
    const { t } = useTranslation();

    return (
        <div className="bg-base-100 rounded-xl border border-base-content/10 shadow-sm overflow-hidden">
            <table className="table w-full border-separate border-spacing-0">
                <thead>
                    <tr className="bg-base-200/50">
                        <th className="bg-transparent border-b border-base-content/10 py-4 pl-6 w-12">
                            <input
                                type="checkbox"
                                className="checkbox checkbox-sm"
                                checked={allVisibleSelected}
                                onChange={onToggleSelectAll}
                                aria-label={t("selectAllAgents")}
                            />
                        </th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60">{t("avatar")}</th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60">{t("agentName")}</th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60">{t("role")}</th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60">{t("model")}</th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60">{t("tools")}</th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60">{t("skills")}</th>
                        <th className="bg-transparent border-b border-base-content/10 py-4 font-semibold text-xs opacity-60 text-right pr-6">{t("actions")}</th>
                    </tr>
                </thead>
                <tbody className="divide-y divide-base-content/5">
                    {agents.map((agent) => {
                        const locked = isBuiltinGroupOwner(agent);
                        const selected = selectedAgentIds.includes(agent.id);
                        const providerInfo = providerInfoById.get(agent.modelPolicy.provider);
                        return (
                            <tr
                                key={agent.id}
                                className={`group transition-colors ${
                                    selected ? "bg-primary/5" : "hover:bg-base-200/30"
                                }`}
                            >
                                <td className="py-4 pl-6">
                                    <input
                                        type="checkbox"
                                        className="checkbox checkbox-sm"
                                        checked={selected}
                                        disabled={locked}
                                        onChange={() => onToggleSelect(agent.id)}
                                        aria-label={t("selectAgent", { name: agent.name })}
                                    />
                                </td>
                                <td className="py-4">
                                    <div className="grid h-10 w-10 place-items-center rounded-xl bg-primary/10 text-primary text-xs font-bold ring-1 ring-primary/20 ring-inset">
                                        {agent.avatar}
                                    </div>
                                </td>
                                <td className="py-4">
                                    <div className="font-semibold text-sm">{agent.name}</div>
                                    <div className="text-[10px] opacity-40 font-mono mt-0.5 truncate max-w-24">ID: {agent.id.slice(0, 8)}...</div>
                                </td>
                                <td className="py-4">
                                    <span className="badge badge-outline badge-sm text-[10px] font-bold py-2 px-2.5 opacity-80 uppercase tracking-widest">{agent.role}</span>
                                </td>
                                <td className="py-4">
                                    <div className="flex items-center gap-1.5">
                                        <i className="fas fa-microchip text-[10px] opacity-40" />
                                        <span className="text-xs font-medium opacity-80">{agent.modelPolicy.model}</span>
                                    </div>
                                    <div className="mt-1">
                                        <span
                                            className={`badge badge-xs border-none ${
                                                providerInfo?.available ? "bg-success/10 text-success" : "bg-warning/10 text-warning"
                                            }`}
                                            title={providerReasonHint(providerInfo?.reason ?? null)}
                                        >
                                            {providerRuntimeLabel(agent.modelPolicy.provider)}
                                        </span>
                                    </div>
                                </td>
                                <td className="py-4 text-center">
                                    <div className="badge badge-primary/10 text-primary border-none font-bold text-[10px] px-2.5 py-2">{agent.toolIds.length}</div>
                                </td>
                                <td className="py-4 text-center">
                                    <div className="badge badge-secondary/10 text-secondary border-none font-bold text-[10px] px-2.5 py-2">{agent.skillIds.length}</div>
                                </td>
                                <td className="py-4 text-right pr-6">
                                    <div className="flex items-center justify-end gap-1.5">
                                        <button
                                            className="btn btn-ghost btn-xs w-8 h-8 rounded-lg hover:bg-primary/10 hover:text-primary transition-all p-0"
                                            onClick={() => onEdit(agent)}
                                            title={t("edit")}
                                        >
                                            <i className="fas fa-pen text-[10px]" />
                                        </button>
                                        <button
                                            className={`btn btn-ghost btn-xs h-8 w-8 rounded-lg p-0 transition-all ${
                                                locked
                                                    ? "cursor-not-allowed text-base-content/30"
                                                    : "hover:bg-error/10 hover:text-error"
                                            }`}
                                            onClick={() => {
                                                if (!locked) {
                                                    onDelete(agent);
                                                }
                                            }}
                                            title={locked ? t("groupOwnerLocked") : t("delete")}
                                            disabled={locked}
                                        >
                                            <i className="fas fa-trash-alt text-[10px]" />
                                        </button>
                                    </div>
                                </td>
                            </tr>
                        );
                    })}
                    {agents.length === 0 && (
                        <tr>
                            <td colSpan={8} className="text-center text-base-content/40 py-16 bg-base-100">
                                <div className="flex flex-col items-center gap-3">
                                    <i className="fas fa-robot text-4xl opacity-10" />
                                    <span className="text-sm font-medium">{t("noAgentsYet")}</span>
                                </div>
                            </td>
                        </tr>
                    )}
                </tbody>
            </table>
        </div>
    );
}
