import { type ChangeEvent, type FormEvent, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
    AgentProfile,
    CreateAgentInput,
    SkillPack,
    SystemSettings,
    ToolManifest,
    UpdateAgentInput,
    WorkGroup,
} from "../../types";
import { joinPolicyList, splitPolicyList } from "./agentPermissions";
import { generateAgentProfiles } from "../../lib/tauri";
import { AgentAiCreateModal } from "./AgentAiCreateModal";
import { AgentBulkActionsBar } from "./AgentBulkActionsBar";
import { AgentBulkAddToGroupModal } from "./AgentBulkAddToGroupModal";
import { AgentBatchReviewModal } from "./AgentBatchReviewModal";
import { AgentBulkEditModal, type AgentBulkEditDraft } from "./AgentBulkEditModal";
import { AgentDeleteConfirmModal } from "./AgentDeleteConfirmModal";
import { AgentManagementTable } from "./AgentManagementTable";
import {
    buildProviderAvailability,
    type ProviderAvailability,
    buildCreateForm,
    emptyForm,
    isBuiltinGroupOwner,
    normalizeGeneratedAgentInput,
    normalizeModelForm,
    serializeAgentConfig,
} from "./agentManagementUtils";
interface AgentManagementProps {
    agents: AgentProfile[];
    workGroups: WorkGroup[];
    skills: SkillPack[];
    tools: ToolManifest[];
    settings: SystemSettings;
    onCreateAgent: (input: CreateAgentInput) => Promise<void>;
    onUpdateAgent: (input: UpdateAgentInput) => Promise<void>;
    onDeleteAgent: (id: string) => Promise<void>;
    onAddAgentToWorkGroup: (workGroupId: string, agentId: string) => Promise<void>;
}
export function AgentManagement({
    agents,
    workGroups,
    tools,
    settings,
    onCreateAgent,
    onUpdateAgent,
    onDeleteAgent,
    onAddAgentToWorkGroup,
}: AgentManagementProps) {
    const { t } = useTranslation();
    const [modalOpen, setModalOpen] = useState(false);
    const [confirmDeleteIds, setConfirmDeleteIds] = useState<string[]>([]);
    const [editingAgent, setEditingAgent] = useState<AgentProfile | null>(null);
    const [form, setForm] = useState<CreateAgentInput>(emptyForm);
    const [search, setSearch] = useState("");
    const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
    const [creatingAiAgent, setCreatingAiAgent] = useState(false);
    const [creatingReviewedAgents, setCreatingReviewedAgents] = useState(false);
    const [aiCreateModalOpen, setAiCreateModalOpen] = useState(false);
    const [aiPrompt, setAiPrompt] = useState("");
    const [aiPromptError, setAiPromptError] = useState<string | null>(null);
    const [generatedAgentDrafts, setGeneratedAgentDrafts] = useState<CreateAgentInput[]>([]);
    const [batchReviewModalOpen, setBatchReviewModalOpen] = useState(false);
    const [bulkActionPending, setBulkActionPending] = useState(false);
    const [bulkAddModalOpen, setBulkAddModalOpen] = useState(false);
    const [bulkEditModalOpen, setBulkEditModalOpen] = useState(false);
    const [bulkAddWorkGroupId, setBulkAddWorkGroupId] = useState("");
    const [bulkEditDraft, setBulkEditDraft] = useState<AgentBulkEditDraft>({
        applyModelPolicy: false,
        provider: "",
        model: "",
        temperature: emptyForm.temperature,
        applyPermissionPolicy: false,
        allowFsRoots: "",
        allowNetworkDomains: "",
    });
    const providerAvailability = useMemo<ProviderAvailability[]>(
        () => settings.providers.map(buildProviderAvailability),
        [settings.providers]
    );

    const providerAvailabilityById = useMemo(
        () => new Map(providerAvailability.map((item) => [item.provider.id, item])),
        [providerAvailability]
    );
    const selectedProviderInfo = providerAvailabilityById.get(form.provider) ?? null;
    const selectedProvider = selectedProviderInfo?.provider ?? null;
    const selectedProviderModels = selectedProvider?.models ?? [];
    const hasCustomModel = form.model.trim().length > 0 && !selectedProviderModels.includes(form.model);
    const canSubmit = Boolean(
        form.name.trim() &&
        form.role.trim() &&
        form.provider.trim() &&
        form.model.trim() &&
        selectedProviderInfo?.available
    );

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
    const filteredSelectableAgentIds = useMemo(
        () => filteredAgents.filter((agent) => !isBuiltinGroupOwner(agent)).map((agent) => agent.id),
        [filteredAgents]
    );
    const allFilteredSelected = filteredSelectableAgentIds.length > 0 &&
        filteredSelectableAgentIds.every((agentId) => selectedAgentIds.includes(agentId));
    const confirmDeleteAgents = useMemo(
        () => agents.filter((agent) => confirmDeleteIds.includes(agent.id)),
        [agents, confirmDeleteIds]
    );
    const selectedAgents = useMemo(
        () => agents.filter((agent) => selectedAgentIds.includes(agent.id)),
        [agents, selectedAgentIds]
    );
    const selectedBulkEditProvider = useMemo(
        () =>
            settings.providers.find((provider) => provider.id === bulkEditDraft.provider) ?? null,
        [settings.providers, bulkEditDraft.provider]
    );
    useEffect(() => {
        setSelectedAgentIds((current) =>
            current.filter((agentId) => {
                const agent = agents.find((item) => item.id === agentId);
                return Boolean(agent && !isBuiltinGroupOwner(agent));
            })
        );
        setConfirmDeleteIds((current) =>
            current.filter((agentId) => {
                const agent = agents.find((item) => item.id === agentId);
                return Boolean(agent && !isBuiltinGroupOwner(agent));
            })
        );
    }, [agents]);
    function openCreate() {
        setEditingAgent(null);
        setForm(buildCreateForm(settings));
        setModalOpen(true);
    }
    function openCreateWithAI() {
        setAiPrompt("");
        setAiPromptError(null);
        setAiCreateModalOpen(true);
    }
    function closeBatchReviewModal() {
        if (creatingReviewedAgents) {
            return;
        }
        setBatchReviewModalOpen(false);
        setGeneratedAgentDrafts([]);
    }
    async function handleGenerateAiAgent() {
        const prompt = aiPrompt.trim();
        if (!prompt) {
            setAiPromptError(t("aiCreateAgentEmptyPrompt"));
            return;
        }
        setCreatingAiAgent(true);
        setAiPromptError(null);
        try {
            const generatedProfiles = await generateAgentProfiles(prompt);
            if (generatedProfiles.length === 0) {
                throw new Error("empty result");
            }
            if (generatedProfiles.length === 1) {
                setEditingAgent(null);
                setForm(normalizeGeneratedAgentInput(settings, generatedProfiles[0]));
                setAiCreateModalOpen(false);
                setModalOpen(true);
                return;
            }
            setGeneratedAgentDrafts(
                generatedProfiles.map((generated) => normalizeGeneratedAgentInput(settings, generated))
            );
            setBatchReviewModalOpen(true);
            setAiCreateModalOpen(false);
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setAiPromptError(t("aiCreateAgentFailed", { message }));
        } finally {
            setCreatingAiAgent(false);
        }
    }
    function updateGeneratedAgentDraft(index: number, nextDraft: CreateAgentInput) {
        setGeneratedAgentDrafts((current) =>
            current.map((draft, draftIndex) => (draftIndex === index ? nextDraft : draft))
        );
    }
    function removeGeneratedAgentDraft(index: number) {
        setGeneratedAgentDrafts((current) => current.filter((_, draftIndex) => draftIndex !== index));
    }
    function updateGeneratedAgentProvider(index: number, providerId: string) {
        setGeneratedAgentDrafts((current) =>
            current.map((draft, draftIndex) => {
                if (draftIndex !== index) {
                    return draft;
                }
                return {
                    ...draft,
                    ...normalizeModelForm(settings, {
                        provider: providerId,
                        temperature: draft.temperature,
                    }),
                };
            })
        );
    }
    async function handleCreateReviewedAgents() {
        const drafts = generatedAgentDrafts.filter(
            (draft) =>
                draft.name.trim() &&
                draft.role.trim() &&
                draft.provider.trim() &&
                draft.model.trim()
        );
        if (drafts.length === 0) {
            return;
        }
        setCreatingReviewedAgents(true);
        try {
            for (const draft of drafts) {
                await onCreateAgent(draft);
            }
            setBatchReviewModalOpen(false);
            setGeneratedAgentDrafts([]);
        } finally {
            setCreatingReviewedAgents(false);
        }
    }
    function openBulkAddToGroup() {
        setBulkAddWorkGroupId("");
        setBulkAddModalOpen(true);
    }

    function openBulkEdit() {
        const modelForm = normalizeModelForm(settings);
        setBulkEditDraft({
            applyModelPolicy: false,
            provider: modelForm.provider,
            model: modelForm.model,
            temperature: modelForm.temperature,
            applyPermissionPolicy: false,
            allowFsRoots: "",
            allowNetworkDomains: "",
        });
        setBulkEditModalOpen(true);
    }

    async function handleBulkAddToGroup() {
        if (!bulkAddWorkGroupId) {
            return;
        }
        const workGroup = workGroups.find((item) => item.id === bulkAddWorkGroupId);
        if (!workGroup) {
            return;
        }
        setBulkActionPending(true);
        try {
            for (const agent of selectedAgents) {
                if (!workGroup.memberAgentIds.includes(agent.id)) {
                    await onAddAgentToWorkGroup(workGroup.id, agent.id);
                }
            }
            setBulkAddModalOpen(false);
            setBulkAddWorkGroupId("");
        } finally {
            setBulkActionPending(false);
        }
    }
    async function handleBulkEditSubmit() {
        if (!bulkEditDraft.applyModelPolicy && !bulkEditDraft.applyPermissionPolicy) {
            return;
        }
        setBulkActionPending(true);
        try {
            for (const agent of selectedAgents) {
                const modelPolicy = bulkEditDraft.applyModelPolicy
                    ? normalizeModelForm(settings, {
                          provider: bulkEditDraft.provider,
                          model: bulkEditDraft.model,
                          temperature: bulkEditDraft.temperature,
                      })
                    : {
                          provider: agent.modelPolicy.provider,
                          model: agent.modelPolicy.model,
                          temperature: agent.modelPolicy.temperature,
                      };
                const permissionPolicy = bulkEditDraft.applyPermissionPolicy
                    ? {
                          allowToolIds: [...agent.permissionPolicy.allowToolIds],
                          denyToolIds: [...agent.permissionPolicy.denyToolIds],
                          requireApprovalToolIds: [...agent.permissionPolicy.requireApprovalToolIds],
                          allowFsRoots: splitPolicyList(bulkEditDraft.allowFsRoots),
                          allowNetworkDomains: splitPolicyList(bulkEditDraft.allowNetworkDomains),
                      }
                    : agent.permissionPolicy;

                await onUpdateAgent({
                    id: agent.id,
                    name: agent.name,
                    avatar: agent.avatar,
                    role: agent.role,
                    objective: agent.objective,
                    provider: modelPolicy.provider,
                    model: modelPolicy.model,
                    temperature: modelPolicy.temperature,
                    toolIds: [...agent.toolIds],
                    maxParallelRuns: agent.maxParallelRuns,
                    canSpawnSubtasks: agent.canSpawnSubtasks,
                    memoryPolicy: {
                        readScope: [...agent.memoryPolicy.readScope],
                        writeScope: [...agent.memoryPolicy.writeScope],
                        pinnedMemoryIds: [...agent.memoryPolicy.pinnedMemoryIds],
                    },
                    permissionPolicy,
                });
            }
            setBulkEditModalOpen(false);
        } finally {
            setBulkActionPending(false);
        }
    }
    async function handleCopySelectedConfig() {
        const payload = JSON.stringify(selectedAgents.map(serializeAgentConfig), null, 2);
        await navigator.clipboard.writeText(payload);
    }
    function handleExportSelectedConfig() {
        const payload = JSON.stringify(selectedAgents.map(serializeAgentConfig), null, 2);
        const blob = new Blob([payload], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = `nextchat-agents-${new Date().toISOString().slice(0, 10)}.json`;
        link.click();
        URL.revokeObjectURL(url);
    }
    function openEdit(agent: AgentProfile) {
        setEditingAgent(agent);
        const modelForm = normalizeModelForm(settings, {
            provider: agent.modelPolicy.provider,
            model: agent.modelPolicy.model,
            temperature: agent.modelPolicy.temperature,
        });
        setForm({
            name: agent.name,
            avatar: agent.avatar,
            role: agent.role,
            objective: agent.objective,
            ...modelForm,
            toolIds: [...agent.toolIds],
            maxParallelRuns: agent.maxParallelRuns,
            canSpawnSubtasks: agent.canSpawnSubtasks,
            memoryPolicy: {
                readScope: [...agent.memoryPolicy.readScope],
                writeScope: [...agent.memoryPolicy.writeScope],
                pinnedMemoryIds: [...agent.memoryPolicy.pinnedMemoryIds],
            },
            permissionPolicy: {
                allowToolIds: [...agent.permissionPolicy.allowToolIds],
                denyToolIds: [...agent.permissionPolicy.denyToolIds],
                requireApprovalToolIds: [...agent.permissionPolicy.requireApprovalToolIds],
                allowFsRoots: [...agent.permissionPolicy.allowFsRoots],
                allowNetworkDomains: [...agent.permissionPolicy.allowNetworkDomains],
            },
        });
        setModalOpen(true);
    }

    async function handleSubmit(e: FormEvent<HTMLFormElement>) {
        e.preventDefault();
        if (!canSubmit) {
            return;
        }
        if (editingAgent) {
            await onUpdateAgent({ id: editingAgent.id, ...form });
        } else {
            await onCreateAgent(form);
        }
        setModalOpen(false);
        setEditingAgent(null);
        setForm(emptyForm);
    }

    function requestDelete(agentIds: string[]) {
        const nextIds = agentIds.filter((agentId, index, array) => {
            if (array.indexOf(agentId) !== index) {
                return false;
            }
            const agent = agents.find((item) => item.id === agentId);
            return Boolean(agent && !isBuiltinGroupOwner(agent));
        });
        if (nextIds.length > 0) {
            setConfirmDeleteIds(nextIds);
        }
    }

    async function handleDelete() {
        if (confirmDeleteIds.length === 0) {
            return;
        }
        for (const agentId of confirmDeleteIds) {
            await onDeleteAgent(agentId);
        }
        setSelectedAgentIds((current) => current.filter((agentId) => !confirmDeleteIds.includes(agentId)));
        setConfirmDeleteIds([]);
    }

    function toggleAgentSelection(agentId: string) {
        setSelectedAgentIds((current) =>
            current.includes(agentId)
                ? current.filter((item) => item !== agentId)
                : [...current, agentId]
        );
    }

    function toggleSelectAllFilteredAgents() {
        setSelectedAgentIds((current) => {
            if (allFilteredSelected) {
                return current.filter((agentId) => !filteredSelectableAgentIds.includes(agentId));
            }
            return Array.from(new Set([...current, ...filteredSelectableAgentIds]));
        });
    }

    function toggleArrayItem(arr: string[], item: string): string[] {
        return arr.includes(item) ? arr.filter((i) => i !== item) : [...arr, item];
    }

    function providerReasonLabel(reason: ProviderAvailability["reason"]): string {
        switch (reason) {
            case "disabled":
                return t("disabled");
            case "missingApiKey":
                return t("providerMissingApiKey");
            case "unsupported":
                return t("providerUnsupportedRuntime");
            case "noModels":
                return t("providerNoModels");
            default:
                return "";
        }
    }

    function providerReasonHint(reason: ProviderAvailability["reason"]): string {
        switch (reason) {
            case "disabled":
                return t("providerDisabledHint");
            case "missingApiKey":
                return t("providerMissingApiKeyHint");
            case "unsupported":
                return t("providerUnsupportedRuntimeHint");
            case "noModels":
                return t("providerNoModelsHint");
            default:
                return t("providerReadyHint");
        }
    }

    function providerRuntimeLabel(providerId: string): string {
        const providerInfo = providerAvailabilityById.get(providerId);
        if (providerInfo?.available) {
            return t("realModelReady");
        }

        return t("fallbackExecution");
    }

    return (
        <div className="flex h-full flex-col animate-in fade-in duration-500">
            <div className="flex items-center justify-between border-b border-base-content/10 px-6 py-5 bg-base-100/50 backdrop-blur-md sticky top-0 z-10">
                <div>
                    <h1 className="text-xl font-bold tracking-tight">{t("agentManagement")}</h1>
                    <p className="text-sm text-base-content/50 mt-1 font-medium">{t("agentManagementDesc")}</p>
                </div>
                <div className="flex items-center gap-3">
                    <AgentBulkActionsBar
                        selectedCount={selectedAgentIds.length}
                        onClearSelection={() => setSelectedAgentIds([])}
                        onDelete={() => requestDelete(selectedAgentIds)}
                        onAddToGroup={openBulkAddToGroup}
                        onBulkEdit={openBulkEdit}
                        onCopyConfig={() => void handleCopySelectedConfig()}
                        onExportConfig={handleExportSelectedConfig}
                    />
                    <div className="relative group">
                        <i className="fas fa-search absolute left-3 top-1/2 -translate-y-1/2 text-base-content/30 group-focus-within:text-primary transition-colors text-xs" />
                        <input
                            className="input input-bordered input-sm pl-8 w-64 bg-base-200/50 border-none transition-all focus:bg-base-100 focus:ring-1 focus:ring-primary/20"
                            placeholder={t("searchPlaceholder")}
                            value={search}
                            onChange={(e: ChangeEvent<HTMLInputElement>) => setSearch(e.target.value)}
                        />
                    </div>
                    <button
                        className="btn btn-secondary btn-sm gap-2 shadow-sm"
                        onClick={openCreateWithAI}
                        disabled={creatingAiAgent}
                    >
                        <i className={`fas ${creatingAiAgent ? "fa-spinner fa-spin" : "fa-wand-magic-sparkles"} text-xs`} />
                        {creatingAiAgent ? t("aiCreatingAgent") : t("aiCreateAgent")}
                    </button>
                    <button className="btn btn-primary btn-sm gap-2 shadow-sm shadow-primary/20" onClick={openCreate}>
                        <i className="fas fa-plus text-xs" /> {t("createAgent")}
                    </button>
                </div>
            </div>
            <div className="flex-1 overflow-auto px-6 py-6 overflow-x-hidden">
                <AgentManagementTable
                    agents={filteredAgents}
                    selectedAgentIds={selectedAgentIds}
                    allVisibleSelected={allFilteredSelected}
                    onToggleSelectAll={toggleSelectAllFilteredAgents}
                    onToggleSelect={toggleAgentSelection}
                    onEdit={openEdit}
                    onDelete={(agent) => requestDelete([agent.id])}
                    isBuiltinGroupOwner={isBuiltinGroupOwner}
                    providerInfoById={providerAvailabilityById}
                    providerReasonHint={providerReasonHint}
                    providerRuntimeLabel={providerRuntimeLabel}
                />
            </div>
            <AgentAiCreateModal
                open={aiCreateModalOpen}
                prompt={aiPrompt}
                error={aiPromptError}
                loading={creatingAiAgent}
                onClose={() => setAiCreateModalOpen(false)}
                onChangePrompt={setAiPrompt}
                onSubmit={() => void handleGenerateAiAgent()}
            />
            <AgentBatchReviewModal
                drafts={generatedAgentDrafts}
                open={batchReviewModalOpen}
                submitting={creatingReviewedAgents}
                providerAvailability={providerAvailability}
                providerAvailabilityById={providerAvailabilityById}
                onClose={closeBatchReviewModal}
                onChange={updateGeneratedAgentDraft}
                onRemove={removeGeneratedAgentDraft}
                onSubmit={handleCreateReviewedAgents}
                onProviderChange={updateGeneratedAgentProvider}
                providerReasonHint={providerReasonHint}
            />
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
                                                const modelForm = normalizeModelForm(settings, {
                                                    provider: e.target.value,
                                                });
                                                setForm((f) => ({
                                                    ...f,
                                                    ...modelForm,
                                                }));
                                            }}
                                        >
                                            <option disabled value="">{t("selectProvider")}</option>
                                            {providerAvailability.map(({ provider, available, reason }) => (
                                                <option key={provider.id} value={provider.id} disabled={!available}>
                                                    {available
                                                        ? provider.name
                                                        : `${provider.name} (${providerReasonLabel(reason)})`}
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
                                            disabled={!selectedProviderInfo?.available}
                                            onChange={(e: ChangeEvent<HTMLSelectElement>) =>
                                                setForm((f) => ({ ...f, model: e.target.value }))
                                            }
                                        >
                                            {selectedProviderModels.map(m => (
                                                <option key={m} value={m}>{m}</option>
                                            ))}
                                            {hasCustomModel && <option value={form.model}>{form.model} (Custom)</option>}
                                        </select>
                                    </div>
                                </div>
                                <div
                                    className={`rounded-xl border px-3 py-2 text-xs ${selectedProviderInfo?.available
                                        ? "border-success/20 bg-success/5 text-success"
                                        : "border-warning/20 bg-warning/10 text-warning"
                                        }`}
                                >
                                    {selectedProviderInfo
                                        ? providerReasonHint(selectedProviderInfo.reason)
                                        : t("providerSelectionRequiredHint")}
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

                            <div className="bg-base-200/50 rounded-2xl p-5 border border-base-content/5 space-y-4">
                                <div className="flex items-center gap-2 text-xs font-bold text-info uppercase tracking-widest">
                                    <i className="fas fa-brain" /> {t("memoryPolicyTitle")}
                                </div>
                                <div className="grid gap-4 md:grid-cols-2">
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("readScope")}</span>
                                        </label>
                                        <textarea
                                            rows={2}
                                            className="textarea textarea-bordered w-full bg-base-100/70 text-sm"
                                            placeholder={t("readScopeHint")}
                                            value={joinPolicyList(form.memoryPolicy.readScope)}
                                            onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
                                                setForm((f) => ({
                                                    ...f,
                                                    memoryPolicy: {
                                                        ...f.memoryPolicy,
                                                        readScope: splitPolicyList(e.target.value),
                                                    },
                                                }))
                                            }
                                        />
                                    </div>
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("writeScope")}</span>
                                        </label>
                                        <textarea
                                            rows={2}
                                            className="textarea textarea-bordered w-full bg-base-100/70 text-sm"
                                            placeholder={t("writeScopeHint")}
                                            value={joinPolicyList(form.memoryPolicy.writeScope)}
                                            onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
                                                setForm((f) => ({
                                                    ...f,
                                                    memoryPolicy: {
                                                        ...f.memoryPolicy,
                                                        writeScope: splitPolicyList(e.target.value),
                                                    },
                                                }))
                                            }
                                        />
                                    </div>
                                    <div className="form-control md:col-span-2">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("pinnedMemory")}</span>
                                        </label>
                                        <textarea
                                            rows={2}
                                            className="textarea textarea-bordered w-full bg-base-100/70 text-sm"
                                            placeholder={t("pinnedMemoryHint")}
                                            value={joinPolicyList(form.memoryPolicy.pinnedMemoryIds)}
                                            onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
                                                setForm((f) => ({
                                                    ...f,
                                                    memoryPolicy: {
                                                        ...f.memoryPolicy,
                                                        pinnedMemoryIds: splitPolicyList(e.target.value),
                                                    },
                                                }))
                                            }
                                        />
                                    </div>
                                </div>
                                <div className="rounded-xl border border-info/20 bg-info/10 px-3 py-2 text-xs text-info">
                                    {t("memoryPolicyHint")}
                                </div>
                            </div>

                            <div className="bg-base-200/50 rounded-2xl p-5 border border-base-content/5 space-y-4">
                                <div className="flex items-center gap-2 text-xs font-bold text-warning uppercase tracking-widest">
                                    <i className="fas fa-shield-halved" /> {t("permissions")}
                                </div>
                                <div className="grid gap-4 md:grid-cols-2">
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("permissionAllowFsRoots")}</span>
                                        </label>
                                        <textarea
                                            rows={2}
                                            className="textarea textarea-bordered w-full bg-base-100/70 text-sm"
                                            placeholder={t("permissionAllowFsRootsHint")}
                                            value={joinPolicyList(form.permissionPolicy.allowFsRoots)}
                                            onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
                                                setForm((f) => ({
                                                    ...f,
                                                    permissionPolicy: {
                                                        ...f.permissionPolicy,
                                                        allowFsRoots: splitPolicyList(e.target.value),
                                                    },
                                                }))
                                            }
                                        />
                                    </div>
                                    <div className="form-control md:col-span-2">
                                        <label className="label">
                                            <span className="label-text text-xs opacity-60">{t("permissionAllowNetworkDomains")}</span>
                                        </label>
                                        <textarea
                                            rows={2}
                                            className="textarea textarea-bordered w-full bg-base-100/70 text-sm"
                                            placeholder={t("permissionAllowNetworkDomainsHint")}
                                            value={joinPolicyList(form.permissionPolicy.allowNetworkDomains)}
                                            onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
                                                setForm((f) => ({
                                                    ...f,
                                                    permissionPolicy: {
                                                        ...f.permissionPolicy,
                                                        allowNetworkDomains: splitPolicyList(e.target.value),
                                                    },
                                                }))
                                            }
                                        />
                                    </div>
                                </div>
                                <div className="rounded-xl border border-warning/20 bg-warning/10 px-3 py-2 text-xs text-warning">
                                    {t("permissionPolicyHint")}
                                </div>
                            </div>

                            <div className="modal-action bg-base-200/50 p-6 -mx-6 -mb-6 mt-6 gap-2 sticky bottom-0 border-t border-base-content/5 backdrop-blur-md">
                                <button className="btn btn-ghost" type="button" onClick={() => setModalOpen(false)}>
                                    {t("cancel")}
                                </button>
                                <button className="btn btn-primary px-8" type="submit" disabled={!canSubmit}>
                                    {editingAgent ? t("updateAgent") : t("createAgent")}
                                </button>
                            </div>
                        </form>
                    </div>
                </dialog>
            )}

            <AgentDeleteConfirmModal
                open={confirmDeleteIds.length > 0}
                agents={confirmDeleteAgents}
                onClose={() => setConfirmDeleteIds([])}
                onConfirm={() => void handleDelete()}
            />
            <AgentBulkAddToGroupModal
                open={bulkAddModalOpen}
                submitting={bulkActionPending}
                selectedCount={selectedAgentIds.length}
                workGroups={workGroups}
                selectedWorkGroupId={bulkAddWorkGroupId}
                onClose={() => !bulkActionPending && setBulkAddModalOpen(false)}
                onSelectWorkGroup={setBulkAddWorkGroupId}
                onSubmit={() => void handleBulkAddToGroup()}
            />
            <AgentBulkEditModal
                open={bulkEditModalOpen}
                submitting={bulkActionPending}
                selectedCount={selectedAgentIds.length}
                draft={bulkEditDraft}
                providerAvailability={providerAvailability}
                selectedProvider={selectedBulkEditProvider}
                onClose={() => !bulkActionPending && setBulkEditModalOpen(false)}
                onChange={setBulkEditDraft}
                onSubmit={() => void handleBulkEditSubmit()}
                providerReasonHint={providerReasonHint}
            />
        </div>
    );
}
