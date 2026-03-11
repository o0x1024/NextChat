import { type ChangeEvent } from "react";
import { useTranslation } from "react-i18next";
import type { AIProviderConfig, CreateAgentInput } from "../../types";

type ProviderReason = "disabled" | "missingApiKey" | "unsupported" | "noModels" | null;

type ProviderAvailability = {
    provider: AIProviderConfig;
    available: boolean;
    reason: ProviderReason;
};

interface AgentBatchReviewModalProps {
    drafts: CreateAgentInput[];
    open: boolean;
    submitting: boolean;
    providerAvailability: ProviderAvailability[];
    providerAvailabilityById: Map<string, ProviderAvailability>;
    onClose: () => void;
    onChange: (index: number, nextDraft: CreateAgentInput) => void;
    onRemove: (index: number) => void;
    onSubmit: () => void;
    onProviderChange: (index: number, providerId: string) => void;
    providerReasonHint: (reason: ProviderReason) => string;
}

export function AgentBatchReviewModal({
    drafts,
    open,
    submitting,
    providerAvailability,
    providerAvailabilityById,
    onClose,
    onChange,
    onRemove,
    onSubmit,
    onProviderChange,
    providerReasonHint,
}: AgentBatchReviewModalProps) {
    const { t } = useTranslation();

    if (!open) {
        return null;
    }

    return (
        <dialog className="modal modal-open bg-base-300/40 backdrop-blur-md" onClick={onClose}>
            <div
                className="modal-box max-w-4xl max-h-[90vh] overflow-y-auto rounded-2xl border border-base-content/10 shadow-2xl"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="sticky top-0 z-10 -mx-6 -mt-6 mb-6 flex items-center justify-between border-b border-base-content/5 bg-base-100/95 px-6 py-5 backdrop-blur-sm">
                    <div>
                        <h3 className="text-lg font-bold">{t("aiReviewAgentsTitle")}</h3>
                        <p className="mt-1 text-sm text-base-content/60">
                            {t("aiReviewAgentsHint", { count: drafts.length })}
                        </p>
                    </div>
                    <button className="btn btn-sm btn-circle btn-ghost" disabled={submitting} onClick={onClose}>
                        ✕
                    </button>
                </div>

                <div className="space-y-4">
                    {drafts.map((draft, index) => {
                        const providerInfo = providerAvailabilityById.get(draft.provider) ?? null;
                        const models = providerInfo?.provider.models ?? [];
                        return (
                            <div key={`${draft.name}-${index}`} className="rounded-2xl border border-base-content/10 bg-base-100 p-5">
                                <div className="mb-4 flex items-center justify-between gap-3">
                                    <div>
                                        <div className="text-sm font-semibold">
                                            {t("aiGeneratedAgentLabel", { index: index + 1 })}
                                        </div>
                                        <div className="text-xs text-base-content/50">{draft.role || t("role")}</div>
                                    </div>
                                    <button
                                        className="btn btn-ghost btn-sm text-error"
                                        disabled={submitting || drafts.length === 1}
                                        onClick={() => onRemove(index)}
                                    >
                                        {t("delete")}
                                    </button>
                                </div>

                                <div className="grid gap-4 md:grid-cols-2">
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("agentName")}</span>
                                        </label>
                                        <input
                                            className="input input-bordered w-full bg-base-200/50"
                                            value={draft.name}
                                            onChange={(event: ChangeEvent<HTMLInputElement>) =>
                                                onChange(index, { ...draft, name: event.target.value })
                                            }
                                        />
                                    </div>
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("avatar")}</span>
                                        </label>
                                        <input
                                            className="input input-bordered w-full bg-base-200/50"
                                            value={draft.avatar}
                                            onChange={(event: ChangeEvent<HTMLInputElement>) =>
                                                onChange(index, { ...draft, avatar: event.target.value })
                                            }
                                        />
                                    </div>
                                </div>

                                <div className="mt-4 form-control">
                                    <label className="label">
                                        <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("role")}</span>
                                    </label>
                                    <input
                                        className="input input-bordered w-full bg-base-200/50"
                                        value={draft.role}
                                        onChange={(event: ChangeEvent<HTMLInputElement>) =>
                                            onChange(index, { ...draft, role: event.target.value })
                                        }
                                    />
                                </div>

                                <div className="mt-4 form-control">
                                    <label className="label">
                                        <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("objective")}</span>
                                    </label>
                                    <textarea
                                        rows={3}
                                        className="textarea textarea-bordered w-full bg-base-200/50"
                                        value={draft.objective}
                                        onChange={(event: ChangeEvent<HTMLTextAreaElement>) =>
                                            onChange(index, { ...draft, objective: event.target.value })
                                        }
                                    />
                                </div>

                                <div className="mt-4 grid gap-4 md:grid-cols-2">
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("provider")}</span>
                                        </label>
                                        <select
                                            className="select select-bordered w-full bg-base-200/50"
                                            value={draft.provider}
                                            onChange={(event) => onProviderChange(index, event.target.value)}
                                        >
                                            <option disabled value="">{t("selectProvider")}</option>
                                            {providerAvailability.map(({ provider, available, reason }) => (
                                                <option key={provider.id} value={provider.id} disabled={!available}>
                                                    {available ? provider.name : `${provider.name} (${reason ?? "unavailable"})`}
                                                </option>
                                            ))}
                                        </select>
                                    </div>
                                    <div className="form-control">
                                        <label className="label">
                                            <span className="label-text text-xs font-bold text-base-content/60 uppercase">{t("model")}</span>
                                        </label>
                                        <select
                                            className="select select-bordered w-full bg-base-200/50"
                                            value={draft.model}
                                            disabled={!providerInfo?.available}
                                            onChange={(event) =>
                                                onChange(index, { ...draft, model: event.target.value })
                                            }
                                        >
                                            {models.map((model) => (
                                                <option key={model} value={model}>{model}</option>
                                            ))}
                                            {!models.includes(draft.model) && draft.model.trim() && (
                                                <option value={draft.model}>{draft.model}</option>
                                            )}
                                        </select>
                                    </div>
                                </div>

                                <div className="mt-4 rounded-xl border border-base-content/10 bg-base-200/40 px-3 py-2 text-xs text-base-content/60">
                                    {providerReasonHint(providerInfo?.reason ?? null)}
                                </div>
                            </div>
                        );
                    })}
                </div>

                <div className="modal-action sticky bottom-0 -mx-6 -mb-6 mt-6 border-t border-base-content/5 bg-base-100/95 px-6 py-4 backdrop-blur-sm">
                    <button className="btn btn-ghost" disabled={submitting} onClick={onClose}>
                        {t("cancel")}
                    </button>
                    <button className="btn btn-primary" disabled={submitting || drafts.length === 0} onClick={onSubmit}>
                        {submitting ? t("creatingAgents") : t("createReviewedAgents", { count: drafts.length })}
                    </button>
                </div>
            </div>
        </dialog>
    );
}
