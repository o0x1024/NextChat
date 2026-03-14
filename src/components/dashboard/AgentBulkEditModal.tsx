import { type ChangeEvent } from "react";
import { useTranslation } from "react-i18next";
import type { AIProviderConfig } from "../../types";
import type { ProviderAvailability, ProviderReason } from "./agentManagementUtils";

export interface AgentBulkEditDraft {
  applyModelPolicy: boolean;
  provider: string;
  model: string;
  temperature: number;
  applyPermissionPolicy: boolean;
  allowFsRoots: string;
  allowNetworkDomains: string;
}

interface AgentBulkEditModalProps {
  open: boolean;
  submitting: boolean;
  selectedCount: number;
  draft: AgentBulkEditDraft;
  providerAvailability: ProviderAvailability[];
  selectedProvider: AIProviderConfig | null;
  onClose: () => void;
  onChange: (nextDraft: AgentBulkEditDraft) => void;
  onSubmit: () => void;
  providerReasonHint: (reason: ProviderReason) => string;
}

export function AgentBulkEditModal({
  open,
  submitting,
  selectedCount,
  draft,
  providerAvailability,
  selectedProvider,
  onClose,
  onChange,
  onSubmit,
  providerReasonHint,
}: AgentBulkEditModalProps) {
  const { t } = useTranslation();

  if (!open) {
    return null;
  }

  return (
    <dialog className="modal modal-open bg-base-300/40 backdrop-blur-md" onClick={onClose}>
      <div className="modal-box max-w-3xl rounded-2xl border border-base-content/10 shadow-2xl" onClick={(event) => event.stopPropagation()}>
        <h3 className="text-lg font-bold">{t("bulkEditAgents")}</h3>
        <p className="mt-2 text-sm text-base-content/60">{t("bulkEditAgentsHint", { count: selectedCount })}</p>

        <div className="mt-6 rounded-2xl border border-base-content/10 p-5">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-primary"
              checked={draft.applyModelPolicy}
              onChange={(event) => onChange({ ...draft, applyModelPolicy: event.target.checked })}
            />
            <span className="label-text font-semibold">{t("bulkApplyModelSettings")}</span>
          </label>

          {draft.applyModelPolicy && (
            <div className="mt-4 grid gap-4 md:grid-cols-3">
              <div className="form-control">
                <label className="label">
                  <span className="label-text text-xs opacity-60">{t("provider")}</span>
                </label>
                <select
                  className="select select-bordered w-full bg-base-200/50"
                  value={draft.provider}
                  onChange={(event) => onChange({ ...draft, provider: event.target.value, model: "" })}
                >
                  <option value="">{t("selectProvider")}</option>
                  {providerAvailability.map(({ provider, available, reason }) => (
                    <option key={provider.id} value={provider.id} disabled={!available}>
                      {available ? provider.name : `${provider.name} (${reason ?? "unavailable"})`}
                    </option>
                  ))}
                </select>
              </div>
              <div className="form-control md:col-span-2">
                <label className="label">
                  <span className="label-text text-xs opacity-60">{t("model")}</span>
                </label>
                <select
                  className="select select-bordered w-full bg-base-200/50"
                  value={draft.model}
                  disabled={!selectedProvider}
                  onChange={(event) => onChange({ ...draft, model: event.target.value })}
                >
                  <option value="">{t("selectModel")}</option>
                  {(selectedProvider?.models ?? []).map((model) => (
                    <option key={model} value={model}>{model}</option>
                  ))}
                </select>
              </div>
              <div className="form-control md:col-span-3">
                <label className="label flex justify-between">
                  <span className="label-text text-xs opacity-60">{t("temperature")}</span>
                  <span className="text-xs font-mono font-bold text-primary">{draft.temperature.toFixed(1)}</span>
                </label>
                <input
                  type="range"
                  className="range range-primary range-xs"
                  min={0}
                  max={2}
                  step={0.1}
                  value={draft.temperature}
                  onChange={(event: ChangeEvent<HTMLInputElement>) =>
                    onChange({ ...draft, temperature: Number.parseFloat(event.target.value) })
                  }
                />
              </div>
              <div className="md:col-span-3 rounded-xl border border-base-content/10 bg-base-200/40 px-3 py-2 text-xs text-base-content/60">
                {providerReasonHint(
                  providerAvailability.find((item) => item.provider.id === draft.provider)?.reason ?? null,
                )}
              </div>
            </div>
          )}
        </div>

        <div className="mt-4 rounded-2xl border border-base-content/10 p-5">
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-primary"
              checked={draft.applyPermissionPolicy}
              onChange={(event) => onChange({ ...draft, applyPermissionPolicy: event.target.checked })}
            />
            <span className="label-text font-semibold">{t("bulkApplyPermissions")}</span>
          </label>

          {draft.applyPermissionPolicy && (
            <div className="mt-4 grid gap-4 md:grid-cols-2">
              <BulkTextarea
                label={t("permissionAllowFsRoots")}
                value={draft.allowFsRoots}
                onChange={(value) => onChange({ ...draft, allowFsRoots: value })}
              />
              <div className="md:col-span-2">
                <BulkTextarea
                  label={t("permissionAllowNetworkDomains")}
                  value={draft.allowNetworkDomains}
                  onChange={(value) => onChange({ ...draft, allowNetworkDomains: value })}
                />
              </div>
            </div>
          )}
        </div>

        <div className="modal-action">
          <button className="btn btn-ghost" disabled={submitting} onClick={onClose}>
            {t("cancel")}
          </button>
          <button
            className="btn btn-primary"
            disabled={submitting || (!draft.applyModelPolicy && !draft.applyPermissionPolicy)}
            onClick={onSubmit}
          >
            {submitting ? t("processingBulkAction") : t("applyBulkChanges")}
          </button>
        </div>
      </div>
    </dialog>
  );
}

function BulkTextarea({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="form-control">
      <label className="label">
        <span className="label-text text-xs opacity-60">{label}</span>
      </label>
      <textarea
        rows={3}
        className="textarea textarea-bordered w-full bg-base-200/50 text-sm"
        value={value}
        onChange={(event: ChangeEvent<HTMLTextAreaElement>) => onChange(event.target.value)}
      />
    </div>
  );
}
