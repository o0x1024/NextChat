import { type ChangeEvent, type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
    AgentProfile,
    CreateAgentInput,
    SkillPack,
    SystemSettings,
    ToolManifest,
    UpdateAgentInput,
} from "../../types";

interface AgentManagementProps {
    agents: AgentProfile[];
    skills: SkillPack[];
    tools: ToolManifest[];
    settings: SystemSettings;
    onCreateAgent: (input: CreateAgentInput) => Promise<void>;
    onUpdateAgent: (input: UpdateAgentInput) => Promise<void>;
    onDeleteAgent: (id: string) => Promise<void>;
}

const emptyForm: CreateAgentInput = {
    name: "",
    avatar: "AX",
    role: "",
    objective: "",
    provider: "openai",
    model: "gpt-4o",
    temperature: 0.7,
    skillIds: [],
    toolIds: [],
    maxParallelRuns: 2,
    canSpawnSubtasks: true,
};

export function AgentManagement({
    agents,
    skills,
    tools,
    settings,
    onCreateAgent,
    onUpdateAgent,
    onDeleteAgent,
}: AgentManagementProps) {
    const { t } = useTranslation();
    const [modalOpen, setModalOpen] = useState(false);
    const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
    const [editingAgent, setEditingAgent] = useState<AgentProfile | null>(null);
    const [form, setForm] = useState<CreateAgentInput>(emptyForm);
    const [search, setSearch] = useState("");

    const filteredAgents = useMemo(() => {
        if (!search.trim()) return agents;
        const q = search.toLowerCase();
        return agents.filter(
            (a) =>
                a.name.toLowerCase().includes(q) ||
                a.role.toLowerCase().includes(q) ||
                a.objective.toLowerCase().includes(q)
        );
    }, [agents, search]);

    function openCreate() {
        setEditingAgent(null);
        setForm(emptyForm);
        setModalOpen(true);
    }

    function openEdit(agent: AgentProfile) {
        setEditingAgent(agent);
        setForm({
            name: agent.name,
            avatar: agent.avatar,
            role: agent.role,
            objective: agent.objective,
            provider: agent.modelPolicy.provider,
            model: agent.modelPolicy.model,
            temperature: agent.modelPolicy.temperature,
            skillIds: [...agent.skillIds],
            toolIds: [...agent.toolIds],
            maxParallelRuns: agent.maxParallelRuns,
            canSpawnSubtasks: agent.canSpawnSubtasks,
        });
        setModalOpen(true);
    }

    async function handleSubmit(e: FormEvent<HTMLFormElement>) {
        e.preventDefault();
        if (editingAgent) {
            await onUpdateAgent({ id: editingAgent.id, ...form });
        } else {
            await onCreateAgent(form);
        }
        setModalOpen(false);
        setEditingAgent(null);
        setForm(emptyForm);
    }

    async function handleDelete() {
        if (confirmDeleteId) {
            await onDeleteAgent(confirmDeleteId);
            setConfirmDeleteId(null);
        }
    }

    function toggleArrayItem(arr: string[], item: string): string[] {
        return arr.includes(item) ? arr.filter((i) => i !== item) : [...arr, item];
    }

    return (
        <div className="flex h-full flex-col animate-in fade-in duration-500">
            {/* Header Area */}
            <div className="flex items-center justify-between border-b border-base-content/10 px-6 py-5 bg-base-100/50 backdrop-blur-md sticky top-0 z-10">
                <div>
                    <h1 className="text-xl font-bold tracking-tight">{t("agentManagement")}</h1>
                    <p className="text-sm text-base-content/50 mt-1 font-medium">{t("agentManagementDesc")}</p>
                </div>
                <div className="flex items-center gap-3">
                    <div className="relative group">
                        <i className="fas fa-search absolute left-3 top-1/2 -translate-y-1/2 text-base-content/30 group-focus-within:text-primary transition-colors text-xs" />
                        <input
                            className="input input-bordered input-sm pl-8 w-64 bg-base-200/50 border-none transition-all focus:bg-base-100 focus:ring-1 focus:ring-primary/20"
                            placeholder={t("searchPlaceholder")}
                            value={search}
                            onChange={(e: ChangeEvent<HTMLInputElement>) => setSearch(e.target.value)}
                        />
                    </div>
                    <button className="btn btn-primary btn-sm gap-2 shadow-sm shadow-primary/20" onClick={openCreate}>
                        <i className="fas fa-plus text-xs" /> {t("createAgent")}
                    </button>
                </div>
            </div>

            {/* Content Body: Table */}
            <div className="flex-1 overflow-auto px-6 py-6 overflow-x-hidden">
                <div className="bg-base-100 rounded-xl border border-base-content/10 shadow-sm overflow-hidden">
                    <table className="table w-full border-separate border-spacing-0">
                        <thead>
                            <tr className="bg-base-200/50">
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
                            {filteredAgents.map((agent) => (
                                <tr key={agent.id} className="group hover:bg-base-200/30 transition-colors">
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
                                                onClick={() => openEdit(agent)}
                                                title={t("edit")}
                                            >
                                                <i className="fas fa-pen text-[10px]" />
                                            </button>
                                            <button
                                                className="btn btn-ghost btn-xs w-8 h-8 rounded-lg hover:bg-error/10 hover:text-error transition-all p-0"
                                                onClick={() => setConfirmDeleteId(agent.id)}
                                                title={t("delete")}
                                            >
                                                <i className="fas fa-trash-alt text-[10px]" />
                                            </button>
                                        </div>
                                    </td>
                                </tr>
                            ))}
                            {filteredAgents.length === 0 && (
                                <tr>
                                    <td colSpan={7} className="text-center text-base-content/40 py-16 bg-base-100">
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
            </div>

            {/* Create/Edit Modal */}
            {modalOpen && (
                <dialog className="modal modal-open bg-base-300/40 backdrop-blur-md" onClick={() => setModalOpen(false)}>
                    <div
                        className="modal-box max-w-2xl max-h-[90vh] overflow-y-auto p-0 rounded-2xl border border-base-content/10 shadow-2xl"
                        onClick={(e) => e.stopPropagation()}
                    >
                        <div className="flex items-center justify-between px-6 py-5 border-b border-base-content/5 bg-base-200/30 sticky top-0 z-20 backdrop-blur-sm">
                            <h3 className="text-lg font-bold">
                                {editingAgent ? t("editAgent") : t("createAgent")}
                            </h3>
                            <button
                                className="btn btn-sm btn-circle btn-ghost"
                                onClick={() => setModalOpen(false)}
                            >
                                ✕
                            </button>
                        </div>

                        <form className="p-6 space-y-6" onSubmit={handleSubmit}>
                            {/* Basic Info Section */}
                            <div className="space-y-4">
                                <div className="grid gap-4 md:grid-cols-2">
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("agentName")}</span>
                                        </label>
                                        <input
                                            className="input input-bordered w-full bg-base-200/50"
                                            required
                                            value={form.name}
                                            onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                                setForm((f) => ({ ...f, name: e.target.value }))
                                            }
                                        />
                                    </div>
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("avatar")}</span>
                                        </label>
                                        <input
                                            className="input input-bordered w-full bg-base-200/50"
                                            value={form.avatar}
                                            placeholder="Emoji or Initials"
                                            onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                                setForm((f) => ({ ...f, avatar: e.target.value }))
                                            }
                                        />
                                    </div>
                                </div>

                                <div className="form-control">
                                    <label className="label">
                                        <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("role")}</span>
                                    </label>
                                    <input
                                        className="input input-bordered w-full bg-base-200/50"
                                        required
                                        value={form.role}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            setForm((f) => ({ ...f, role: e.target.value }))
                                        }
                                    />
                                </div>

                                <div className="form-control">
                                    <label className="label">
                                        <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("objective")}</span>
                                    </label>
                                    <textarea
                                        rows={3}
                                        className="textarea textarea-bordered w-full bg-base-200/50 leading-relaxed"
                                        value={form.objective}
                                        onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
                                            setForm((f) => ({ ...f, objective: e.target.value }))
                                        }
                                    />
                                </div>
                            </div>

                            {/* Model Section */}
                            <div className="bg-base-200/50 rounded-2xl p-5 border border-base-content/5 space-y-4">
                                <div className="flex items-center gap-2 text-xs font-bold text-primary uppercase tracking-widest">
                                    <i className="fas fa-microchip" /> {t("modelConfiguration")}
                                </div>
                                <div className="grid gap-4 md:grid-cols-3">
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("provider")}</span>
                                        </label>
                                        <select
                                            className="select select-bordered select-sm w-full font-medium"
                                            value={form.provider}
                                            onChange={(e: ChangeEvent<HTMLSelectElement>) => {
                                                const provider = settings.providers.find(p => p.id === e.target.value);
                                                setForm((f) => ({
                                                    ...f,
                                                    provider: e.target.value,
                                                    model: provider?.defaultModel || ""
                                                }));
                                            }}
                                        >
                                            <option disabled value="">{t("selectProvider")}</option>
                                            {settings.providers.filter(p => p.enabled).map((p) => (
                                                <option key={p.id} value={p.id}>
                                                    {p.name}
                                                </option>
                                            ))}
                                        </select>
                                    </div>
                                    <div className="form-control col-span-2">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("model")}</span>
                                        </label>
                                        <select
                                            className="select select-bordered select-sm w-full font-medium"
                                            value={form.model}
                                            onChange={(e: ChangeEvent<HTMLSelectElement>) =>
                                                setForm((f) => ({ ...f, model: e.target.value }))
                                            }
                                        >
                                            {(settings.providers.find(p => p.id === form.provider)?.models || []).map(m => (
                                                <option key={m} value={m}>{m}</option>
                                            ))}
                                            <option value={form.model}>{form.model} (Custom)</option>
                                        </select>
                                    </div>
                                </div>
                                <div className="form-control">
                                    <label className="label flex justify-between">
                                        <span className="label-text text-xs opacity-60">{t("temperature")}</span>
                                        <span className="text-xs font-mono font-bold text-primary">{form.temperature.toFixed(1)}</span>
                                    </label>
                                    <input
                                        type="range"
                                        className="range range-primary range-xs"
                                        min={0}
                                        max={2}
                                        step={0.1}
                                        value={form.temperature}
                                        onChange={(e: ChangeEvent<HTMLInputElement>) =>
                                            setForm((f) => ({ ...f, temperature: parseFloat(e.target.value) }))
                                        }
                                    />
                                </div>
                            </div>

                            {/* Capabilities Sections */}
                            <div className="grid gap-6 md:grid-cols-2">
                                <div className="space-y-4">
                                    <div className="flex items-center gap-2 text-xs font-bold text-secondary uppercase tracking-widest">
                                        <i className="fas fa-wrench" /> {t("toolBinding")}
                                    </div>
                                    <div className="flex flex-wrap gap-2 max-h-48 overflow-y-auto p-1">
                                        {tools.map((tool) => (
                                            <label
                                                key={tool.id}
                                                className={`badge cursor-pointer gap-2 py-4 px-3 border-none transition-all ${form.toolIds.includes(tool.id) ? "bg-primary text-primary-content shadow-md shadow-primary/20 scale-105" : "bg-base-200 hover:bg-base-300"
                                                    }`}
                                            >
                                                <input
                                                    type="checkbox"
                                                    className="checkbox checkbox-xs checkbox-primary hidden"
                                                    checked={form.toolIds.includes(tool.id)}
                                                    onChange={() =>
                                                        setForm((f) => ({ ...f, toolIds: toggleArrayItem(f.toolIds, tool.id) }))
                                                    }
                                                />
                                                <i className="fas fa-cube text-[10px] opacity-60" />
                                                <span className="text-xs font-medium">{tool.name}</span>
                                            </label>
                                        ))}
                                    </div>
                                </div>

                                <div className="space-y-4">
                                    <div className="flex items-center gap-2 text-xs font-bold text-accent uppercase tracking-widest">
                                        <i className="fas fa-bullseye" /> {t("skillBinding")}
                                    </div>
                                    <div className="flex flex-wrap gap-2 max-h-48 overflow-y-auto p-1">
                                        {skills.map((skill) => (
                                            <label
                                                key={skill.id}
                                                className={`badge cursor-pointer gap-2 py-4 px-3 border-none transition-all ${form.skillIds.includes(skill.id) ? "bg-secondary text-secondary-content shadow-md shadow-secondary/20 scale-105" : "bg-base-200 hover:bg-base-300"
                                                    }`}
                                            >
                                                <input
                                                    type="checkbox"
                                                    className="checkbox checkbox-xs checkbox-secondary hidden"
                                                    checked={form.skillIds.includes(skill.id)}
                                                    onChange={() =>
                                                        setForm((f) => ({ ...f, skillIds: toggleArrayItem(f.skillIds, skill.id) }))
                                                    }
                                                />
                                                <i className="fas fa-bolt text-[10px] opacity-60" />
                                                <span className="text-xs font-medium">{skill.name}</span>
                                            </label>
                                        ))}
                                    </div>
                                </div>
                            </div>

                            <div className="modal-action bg-base-200/50 p-6 -mx-6 -mb-6 mt-6 gap-2 sticky bottom-0 border-t border-base-content/5 backdrop-blur-md">
                                <button className="btn btn-ghost" type="button" onClick={() => setModalOpen(false)}>
                                    {t("cancel")}
                                </button>
                                <button className="btn btn-primary px-8" type="submit">
                                    {editingAgent ? t("updateAgent") : t("createAgent")}
                                </button>
                            </div>
                        </form>
                    </div>
                </dialog>
            )}

            {/* Confirm Delete Modal */}
            {confirmDeleteId && (
                <dialog className="modal modal-open animate-in zoom-in duration-200" onClick={() => setConfirmDeleteId(null)}>
                    <div className="modal-box max-w-sm rounded-2xl border border-error/20" onClick={(e) => e.stopPropagation()}>
                        <div className="flex flex-col items-center text-center gap-4 py-4">
                            <div className="w-16 h-16 rounded-full bg-error/10 flex items-center justify-center mb-2">
                                <i className="fas fa-exclamation-triangle text-error text-2xl" />
                            </div>
                            <h3 className="text-xl font-bold">{t("areYouSure")}</h3>
                            <p className="text-base-content/60 text-sm">
                                {t("deleteAgentConfirmHint")}
                            </p>
                            <div className="flex gap-3 w-full mt-4">
                                <button className="btn btn-ghost flex-1" onClick={() => setConfirmDeleteId(null)}>{t("cancel")}</button>
                                <button className="btn btn-error flex-1" onClick={handleDelete}>{t("delete")}</button>
                            </div>
                        </div>
                    </div>
                </dialog>
            )}
        </div>
    );
}
